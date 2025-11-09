//! M3U8 分片下载模块，支持AES-128加密流媒体解密
//! - 多线程并发下载
//! - 断点续传
//! - 自定请求头

#![allow(deprecated)]
use crate::download_monitor::{run_monitor_task, DownloadMetrics};
use crate::merge::merge_files;
use anyhow::{anyhow, Result};
use aes::Aes128;
use cipher::{
    BlockDecryptMut, KeyIvInit, block_padding::Pkcs7
};
use cipher::generic_array::GenericArray;
use cbc::Decryptor;
use reqwest::Client;
use std::collections::HashSet;
use std::time::Duration;
use std::{
    sync::{
        atomic::Ordering,
        Arc,
    },
};
use std::path::Path;
use serde::{Serialize, Deserialize};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use tauri::AppHandle;
use std::sync::atomic::AtomicBool;
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, Semaphore},
};

/// 加密信息结构体
/// 用于存储解密TS分片所需的密钥信息
#[derive(Clone, Serialize, Deserialize)]
struct EncryptionInfo {
    key: Vec<u8>,        // AES-128加密密钥（16字节）
    iv: Option<Vec<u8>>, // 初始化向量（16字节），None时使用默认全零IV
}

/// 十六进制字符串转字节向量
/// 示例：hex_to_bytes("0011ff") -> Ok(vec![0x00, 0x11, 0xff])
fn hex_to_bytes(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(anyhow::anyhow!("Hex string has odd length"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow::anyhow!("Invalid hex: {}", e))
        })
        .collect()
}

/// 解析M3U8的EXT-X-KEY标签
/// 返回元组：(加密方法, 密钥URI, IV值)
/// 示例输入："METHOD=AES-128,URI="key.php",IV=0X112233..."
fn parse_ext_x_key(line: &str) -> Result<(String, String, Option<String>)> {
    let content = line.trim_start_matches("#EXT-X-KEY:").trim();
    let mut method = String::new();
    let mut uri = String::new();
    let mut iv = None;

    // 分割键值对
    for part in content.split(',') {
        let mut kv = part.splitn(2, '=');
        let key = kv
            .next()
            .ok_or_else(|| anyhow::anyhow!("Invalid EXT-X-KEY line"))?
            .trim();
        let value = kv
            .next()
            .ok_or_else(|| anyhow::anyhow!("Invalid EXT-X-KEY line"))?
            .trim()
            .trim_matches('"');
        match key {
            "METHOD" => method = value.to_string(),
            "URI" => uri = value.to_string(),
            "IV" => iv = Some(value.to_string()),
            _ => {}
        }
    }
    Ok((method, uri, iv))
}

use std::collections::HashMap;
use reqwest::header::{HeaderName, HeaderValue};

/// 自定义下载请求头选项
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub headers: HashMap<String, String>,
}

impl DownloadOptions {
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
        }
    }
}

pub enum DownloadResult {
    Success(String),   // 成功并且是有效 ts 文件
    Skipped(String),   // 下载成功，但内容无效或空，未写入磁盘
    Cancelled(String), // 因用户取消而中断下载
}

/// 自定义下载请求头
fn preprocess_headers(headers: &HashMap<String, String>) -> reqwest::header::HeaderMap {
    let mut valid_headers = reqwest::header::HeaderMap::new();
    for (key, value) in headers {
        // 尝试添加自定义请求头，如果格式不正确则跳过
        match (HeaderName::from_bytes(key.as_bytes()), HeaderValue::from_str(value)) {
            (Ok(header_name), Ok(header_value)) => {
                valid_headers.insert(header_name, header_value);
            }
            (Err(_), _) => {
                log::warn!("无效的请求头名称，已跳过: {}", key);
            }
            (_, Err(_)) => {
                log::warn!("无效的请求头值，已跳过: {}={}", key, value);
            }
        }
    }
    valid_headers
}

/// 下载单个TS文件（支持加密内容解密）
async fn download_file(
    client: &Client,
    url: &str,
    output_path: &str,
    cancelled: &Arc<AtomicBool>,
    encryption: Option<EncryptionInfo>,
    metrics: Arc<DownloadMetrics>, // metrics参数
    headers: &reqwest::header::HeaderMap, // 预处理后的有效请求头
) -> Result<DownloadResult> {
    // 构建带自定义请求头的请求
    let request = client.get(url).headers(headers.clone());
    
    let mut response = request.send().await?;
    let mut buffer = Vec::new();

    while let Some(chunk) = response.chunk().await? {
        // 每次下载数据块后立即检查取消
        if cancelled.load(Ordering::Relaxed) {
            // 主动清理已下载的部分文件
            fs::remove_file(output_path).await.ok();
            return Ok(DownloadResult::Cancelled(url.to_string()));
        }

        // 记录下载数据
        let chunk_len = chunk.len();
        buffer.extend_from_slice(&chunk);
        metrics.record_chunk(chunk_len).await; // 替换原有的计数器更新
    }

    // 判断是否为空
    if buffer.is_empty() {
        log::warn!("⚠️ [{}] 返回空数据，标记为 Skipped", url);
        return Ok(DownloadResult::Skipped(url.to_string()));
    }
    // 检查是否 HTML/XML 内容
    let content_type = response.headers()
        .get("Content-Type")
        .and_then(|ct| ct.to_str().ok())
        .unwrap_or("");

    if content_type.starts_with("text/html") || content_type.contains("xml") {
        log::warn!("⚠️ [{}] 是 HTML 内容，标记为 Skipped", url);
        return Ok(DownloadResult::Skipped(url.to_string()));
    }

    // AES-128解密处理
    let data: Vec<u8> = if let Some(enc) = encryption {
        let iv_vec = enc.iv.unwrap_or_else(|| vec![0; 16]); // 默认IV处理
        let key = GenericArray::from_slice(&enc.key);
        let iv = GenericArray::from_slice(&iv_vec);

        let decryptor = Decryptor::<Aes128>::new(key, iv);

        let mut buffer_clone = buffer.clone();

        let decrypted = decryptor
            .decrypt_padded_mut::<Pkcs7>(&mut buffer_clone)
            .map_err(|e| anyhow!("Decryption failed: {:?}", e))?;

        decrypted.to_vec()
    } else {
        buffer
    };

    // 写入解密后的文件
    let mut file = fs::File::create(output_path).await?;
    file.write_all(&data).await?;
    Ok(DownloadResult::Success(output_path.to_string()))
}

/// 分片信息结构
#[derive(Serialize, Deserialize)]
struct SegmentMetadata {
    url: String,
    local_path: String,
    encryption: Option<EncryptionInfo>,
}

/// M3U8下载主函数
pub async fn download_m3u8(
    id: String,                       // 下载任务唯一标识
    url: &str,                        // M3U8文件URL
    name: &str,                       // 输出文件名
    temp_dir: &str,                   // ts文件下载目录
    output_dir: &str,                 // MP4视频输出目录
    concurrency: usize,               // 并发线程数
    cancelled: Arc<AtomicBool>,       // 取消标志
    app_handle: AppHandle,            // Tauri应用句柄
    options: DownloadOptions,         // 下载选项（包含自定义headers等）
) -> Result<()> {
    // 创建输出目录
    fs::create_dir_all(temp_dir).await?;
    
    let client = Client::new();
    // 预处理headers，只验证一次
    let valid_headers = preprocess_headers(&options.headers);
    log::info!("headers: {:#?}", valid_headers);
    
    // 分片元数据文件路径
    let segments_metadata_path = format!("{}/segments.json", temp_dir);
    let mut all_ts_segments = Vec::new();
    
    // 尝试从保存的元数据文件中加载分片信息
    if tokio::fs::metadata(&segments_metadata_path).await.is_ok() {
        log::info!("📥 从本地加载分片元数据: {}", segments_metadata_path);
        let metadata_content = tokio::fs::read_to_string(&segments_metadata_path).await?;
        let segments_metadata: Vec<SegmentMetadata> = serde_json::from_str(&metadata_content)?;
        
        // 转换为原始格式
        for segment in segments_metadata {
            all_ts_segments.push((segment.url, segment.local_path, segment.encryption));
        }
    } else {
        // 第一次下载，需要解析M3U8文件

        // 解析M3U8文件内容
        let m3u8_response = client.get(url).headers(valid_headers.clone()).send().await?.text().await?;

        // --- 步骤 1: 解析M3U8，收集所有分片信息 ---
        let mut current_encryption = None;

        for (index, line) in m3u8_response.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("#EXT-X-KEY:") {
                // 处理加密信息
                let (method, key_uri, iv_str) = parse_ext_x_key(line)?;
                if method.to_uppercase() == "AES-128" {
                    // 构建完整密钥URL
                    let key_url = if key_uri.starts_with("http") {
                        key_uri.clone()
                    } else if key_uri.starts_with('/') {
                        // 处理绝对路径（以/开头）- 相对于域名根目录解析
                        let base_url = url.split("/").take(3).collect::<Vec<&str>>().join("/");
                        format!("{}{}", base_url, key_uri)
                    } else {
                        // 处理相对路径 - 相对于M3U8文件所在目录解析
                        format!("{}/{}", url.rsplit_once('/').unwrap().0, key_uri)
                    };

                    // 下载密钥文件
                    let key_response = client.get(&key_url).headers(valid_headers.clone()).send().await?.bytes().await?;
                    let key = key_response.to_vec();

                    // 解析IV值
                    let iv = iv_str.as_ref().and_then(|iv_raw| {
                        let hex = iv_raw.strip_prefix("0x").unwrap_or(iv_raw);
                        hex_to_bytes(hex).ok()
                    });

                    current_encryption = Some(EncryptionInfo { key, iv });
                } else {
                    current_encryption = None;
                }
                continue;
            }

            // 收集TS分片任务
            if line.ends_with(".ts") {
                let ts_url = if line.starts_with("http") {
                    line.to_string()
                } else if line.starts_with('/') {
                    // 处理绝对路径（以/开头）- 相对于域名根目录解析
                    let base_url = url.split("/").take(3).collect::<Vec<&str>>().join("/");
                    format!("{}{}", base_url, line)
                } else {
                    // 处理相对路径 - 相对于M3U8文件所在目录解析
                    format!("{}/{}", url.rsplit_once('/').unwrap().0, line)
                };
                let filename = format!("{}/part_{}.ts", temp_dir, index);
                all_ts_segments.push((ts_url, filename, current_encryption.clone()));
            }
        }
        
        // 保存分片元数据到文件，供后续断点续传使用
        let segments_metadata: Vec<SegmentMetadata> = all_ts_segments
            .iter()
            .map(|(url, local_path, encryption)| SegmentMetadata {
                url: url.clone(),
                local_path: local_path.clone(),
                encryption: encryption.clone(),
            })
            .collect();
        
        let metadata_json = serde_json::to_string(&segments_metadata)?;
        tokio::fs::write(&segments_metadata_path, metadata_json).await?;
        log::info!("💾 已保存分片元数据到: {}", segments_metadata_path);
    }

    if all_ts_segments.is_empty() {
        log::warn!("M3U8 [{} {}] 中未找到 .ts 分片", id, name);
        return Err(anyhow::anyhow!("M3U8中未找到任何.ts分片"));
    }

    // --- 步骤 2: 断点续传检查 (基于 Manifest 文件) ---
    let total_chunks = all_ts_segments.len();
    let ts_files = Arc::new(Mutex::new(Vec::with_capacity(total_chunks))); // 存储 *所有* 最终用于合并的ts文件路径
    let metrics = Arc::new(DownloadMetrics::new(total_chunks));
    let mut pending_downloads = Vec::new(); // 存储 *真正需要下载* 的任务

    // 加载清单文件
    let manifest_path = format!("{}/progress.dat", temp_dir);
    let mut completed_segment_names = HashSet::new();

    if let Ok(file) = tokio::fs::File::open(&manifest_path).await {
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            if !line.trim().is_empty() {
                completed_segment_names.insert(line);
            }
        }
    }
    log::info!("任务 [{}]: 从清单文件中加载了 {} 条已完成记录", id, completed_segment_names.len());

    {
        let mut ts_files_lock = ts_files.lock().await;
        for (ts_url, filename, encryption) in all_ts_segments {
            // 获取相对文件名，例如 "part_123.ts"
            let relative_name = match Path::new(&filename).file_name().and_then(|s| s.to_str()) {
                Some(name) => { name.to_string() }
                None => { continue; } // 路径无效，跳过
            };

            // 检查清单中是否存在
            if completed_segment_names.contains(&relative_name) {
                // 存在，则检查本地文件并更新进度
                match tokio::fs::metadata(&filename).await {
                    Ok(metadata) if metadata.len() > 0 => {
                        // 文件有效，视为已下载
                        ts_files_lock.push(filename); // 直接加入待合并列表

                        // 更新进度
                        let file_size = metadata.len() as usize;
                        metrics.completed_chunks.fetch_add(1, Ordering::Relaxed);
                        metrics.downloaded_bytes.fetch_add(file_size, Ordering::Relaxed);
                        metrics.update_total_bytes(file_size); // 更新总字节数
                    }
                    _ => {
                        // 清单存在，但文件丢失/为空，重新下载
                        pending_downloads.push((ts_url, filename, encryption));
                    }
                }
            } else {
                // 清单不存在，加入下载队列
                pending_downloads.push((ts_url, filename, encryption));
            }
        }
    } // 释放 ts_files_lock

    log::info!(
        "任务 [{}]: 总分片 {}, 已完成 {}, 待下载 {}",
        id,
        total_chunks,
        total_chunks - pending_downloads.len(),
        pending_downloads.len()
    );

    // --- 步骤 3: 启动速度监控任务 ---
    let speed_handle = run_monitor_task(
        id.clone(),
        Arc::clone(&cancelled),
        Arc::clone(&metrics),
        app_handle.clone(),
    ).await;

    // --- 步骤 4: 启动下载任务 (只下载 pending_downloads) ---
    // 创建一个线程安全的清单文件写入器
    let manifest_writer = Arc::new(Mutex::new(
        tokio::fs::File::options()
            .append(true)
            .create(true)
            .open(&manifest_path)
            .await?,
    ));

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::new();
    for (ts_url, filename, encryption) in pending_downloads {
        let client = client.clone();
        let ts_files = Arc::clone(&ts_files);
        let semaphore = Arc::clone(&semaphore);
        let cancelled = Arc::clone(&cancelled);
        let metrics = Arc::clone(&metrics);
        let manifest_writer = Arc::clone(&manifest_writer);
        let headers = valid_headers.clone();

        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;

            const MAX_RETRIES: usize = 15;
            for attempt in 1..=MAX_RETRIES {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok::<(), anyhow::Error>(());
                }
                let result = download_file(
                    &client,
                    &ts_url,
                    &filename,
                    &cancelled,
                    encryption.clone(),
                    metrics.clone(),
                    &headers,
                )
                    .await;

                match result {
                    Ok(DownloadResult::Success(f)) => {
                        log::debug!("✅ 分片 [{}] 下载成功（尝试次数 {}）", f, attempt);

                        if let Some(relative_name) = Path::new(&f).file_name().and_then(|s| s.to_str()) {
                            let mut writer = manifest_writer.lock().await;
                            writer.write_all(format!("{}\n", relative_name).as_bytes()).await?;
                            writer.flush().await?; // 立即刷新缓冲区，确保数据持久化
                        }

                        metrics.completed_chunks.fetch_add(1, Ordering::Relaxed);
                        ts_files.lock().await.push(f);
                        return Ok(());
                    }
                    Ok(DownloadResult::Skipped(f)) => {
                        log::warn!("🗑️ 分片 [{}] 内容无效，已跳过", f);
                        return Ok(());
                    }
                    Ok(DownloadResult::Cancelled(f)) => {
                        log::debug!("⏹️ 分片 [{}] 因取消而中断", f);
                        return Ok(());
                    }
                    Err(e) => {
                        log::error!("⚠️ 分片 [{}] 第 {} 次下载失败，原因：{}", filename, attempt, e);
                        if attempt < MAX_RETRIES {
                            // 优化点 1: 实现指数退避和随机抖动
                            // 计算基础延迟: 2^attempt 秒，最大不超过 10 秒
                            let base_delay_secs = (1 << (attempt - 1)).min(10);

                            // 引入随机抖动: 延迟在 [base_delay_secs, base_delay_secs + 1] 之间
                            let mut rng = SmallRng::from_entropy();
                            let random_millis = rng.gen_range(0..1000);

                            let total_delay = Duration::from_secs(base_delay_secs as u64)
                                + Duration::from_millis(random_millis);

                            log::info!("➡️ 分片 [{}] 正在退避，等待 {:?}", filename, total_delay);
                            tokio::time::sleep(total_delay).await;
                        } else {
                            log::error!("❌ 分片 [{}] 所有重试失败: {:?}, 尝试取消任务", filename, e);
                            cancelled.store(true, Ordering::SeqCst); // 触发取消
                        }
                    }
                }
            }
            // 返回 Err 表示该 task 最终失败
            Err(anyhow::anyhow!("分片 [{}] 所有尝试均失败", filename))
        }));
    }

    // --- 步骤 5: 等待所有下载任务完成 ---
    for handle in handles {
        handle.await??;
    }

    // 检查是否所有分片都已就绪（包括已存在和刚下载的）
    let final_ts_files = Arc::try_unwrap(ts_files).unwrap().into_inner();
    if final_ts_files.len() != total_chunks {
        if cancelled.load(Ordering::Relaxed) {
            // 用户主动取消
            log::info!(
                "任务 [{}] 未完成下载。预期: {}, 已完成: {}. 任务已被取消。",
                id,
                total_chunks,
                final_ts_files.len()
            );
        } else {
            // 下载失败
            log::error!(
                "任务 [{}] 未能集齐所有分片。预期: {}, 实际: {}. 下载失败。",
                id,
                total_chunks,
                final_ts_files.len()
            );
            // 强制取消
            cancelled.store(true, Ordering::SeqCst);
            // 等待速度监控任务退出
            speed_handle.await?;
            return Err(anyhow::anyhow!("下载失败，部分分片缺失"));
        }
    } else {
        log::info!("任务 [{}] 所有分片均已就绪，准备合并。", id);
    }

    // 等待速度监控任务退出
    speed_handle.await?;

    // 如果任务被取消，则跳过合并
    if cancelled.load(Ordering::Relaxed) {
        log::warn!("任务 [{}] 已被取消，跳过合并。", id);
        return Ok(());
    }

    // --- 步骤 6: 合并 TS 文件为 MP4 ---
    merge_files(
        id.clone(),
        &name,
        final_ts_files,
        &temp_dir,
        &output_dir,
        app_handle.clone(),
    )
        .await?;

    Ok(())
}

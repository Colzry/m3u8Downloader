//! M3U8 分片下载模块，支持AES-128加密流媒体解密
//! 核心特性：
//! - 多线程并发下载
//! - 暂停/恢复控制
//! - 断点续传（基于 manifest 文件，性能更高）

use crate::download_monitor::{run_monitor_task, DownloadMetrics};
use crate::download_manager::DownloadControl;
use crate::merge::merge_files;
use anyhow::Result;
use openssl::symm::{decrypt, Cipher};
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
use tauri::AppHandle;
use tokio::sync::Notify;
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, Semaphore},
};

/// 加密信息结构体
/// 用于存储解密TS分片所需的密钥信息
#[derive(Clone)]
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


/// 下载结果状态枚举
#[derive(Debug, Clone)]
pub enum DownloadResult {
    Success(String),   // 成功并且是有效 ts 文件
    Skipped(String),   // 下载成功，但内容无效或空，未写入磁盘
}

/// 下载单个TS文件（支持加密内容解密）
/// 关键改进点：
/// 1. 实时更新全局下载字节计数器
/// 2. 改进的暂停处理机制
async fn download_file(
    client: &Client,
    url: &str,
    output_path: &str,
    control: &DownloadControl,
    encryption: Option<EncryptionInfo>,
    pause_notify: Arc<Notify>,     // 暂停通知通道
    metrics: Arc<DownloadMetrics>, // metrics参数
) -> Result<DownloadResult> {
    let mut response = client.get(url).send().await?;
    let mut buffer = Vec::new();

    while let Some(chunk) = response.chunk().await? {
        // 每次下载数据块后立即检查取消
        if control.is_cancelled() {
            // 主动清理已下载的部分文件
            fs::remove_file(output_path).await.ok();
            return Ok(DownloadResult::Skipped(url.to_string()));
        }

        // 处理暂停状态（支持取消中断）
        while control.is_paused() {
            // 使用带超时的等待避免永久阻塞
            tokio::select! {
                _ = pause_notify.notified() => {},
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if control.is_cancelled() {
                        fs::remove_file(output_path).await.ok();
                        return Ok(DownloadResult::Skipped(url.to_string()));
                    }
                }
            }
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
    let data = if let Some(enc) = encryption {
        let iv = enc.iv.unwrap_or_else(|| vec![0; 16]); // 默认IV处理
        let cipher = Cipher::aes_128_cbc();
        decrypt(cipher, &enc.key, Some(&iv), &buffer)?
    } else {
        buffer
    };

    // 写入解密后的文件
    let mut file = fs::File::create(output_path).await?;
    file.write_all(&data).await?;
    Ok(DownloadResult::Success(output_path.to_string()))
}

/// M3U8下载主函数
/// 改进点：
/// 1. 新增全局下载速度监控
/// 2. 更精确的进度计算
/// 3. 完善的状态报告机制
pub async fn download_m3u8(
    id: String,                    // 下载任务唯一标识
    url: &str,                     // M3U8文件URL
    name: &str,                    // 输出文件名
    temp_dir: &str,                // ts文件下载目录
    output_dir: &str,              // MP4视频输出目录
    concurrency: usize,            // 并发线程数
    control: Arc<DownloadControl>, // 下载控制对象
    app_handle: AppHandle,         // Tauri应用句柄
) -> Result<()> {
    // 创建输出目录
    fs::create_dir_all(temp_dir).await?;
    let client = Client::new();
    let pause_notify = control.get_notify();

    // 解析M3U8文件内容
    let m3u8_response = client.get(url).send().await?.text().await?;

    // --- 步骤 1: 解析M3U8，收集所有分片信息 ---
    let mut all_ts_segments = Vec::new();
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
                } else {
                    format!("{}/{}", url.rsplit_once('/').unwrap().0, key_uri)
                };

                // 下载密钥文件
                let key_response = client.get(&key_url).send().await?.bytes().await?;
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
            } else {
                format!("{}/{}", url.rsplit_once('/').unwrap().0, line)
            };
            let filename = format!("{}/part_{}.ts", temp_dir, index);
            // 存储元组 (URL, 本地路径, 加密信息)
            all_ts_segments.push((ts_url, filename, current_encryption.clone()));
        }
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
                Some(name) => name.to_string(),
                None => continue, // 路径无效，跳过
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


    // --- 步骤 3: 预获取 待下载 分片的大小 ---
    let pre_semaphore = Arc::new(Semaphore::new(concurrency));
    let mut pre_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // 只对需要下载的分片执行 HEAD 请求
    for (ts_url, _, _) in &pending_downloads {
        let client = client.clone();
        let metrics = metrics.clone();
        let ts_url = ts_url.clone();
        let permit = pre_semaphore.clone();

        pre_handles.push(tokio::spawn(async move {
            let _permit = match permit.acquire().await {
                Ok(p) => p,
                Err(_) => return, // Semaphore
            };

            let _ = client
                .head(&ts_url)
                .send()
                .await
                .map(|resp| {
                    resp.headers()
                        .get("Content-Length")
                        .and_then(|hv| hv.to_str().ok())
                        .and_then(|s| s.parse::<usize>().ok())
                        .map(|size| metrics.update_total_bytes(size))
                        .unwrap_or_else(|| metrics.mark_total_bytes_invalid());
                })
                .map_err(|_| metrics.mark_total_bytes_invalid());
        }));
    }
    // 等待所有预请求完成
    for handle in pre_handles {
        handle.await?;
    }

    // --- 步骤 4: 启动速度监控任务 ---
    let speed_handle = run_monitor_task(
        id.clone(),
        Arc::clone(&control),
        Arc::clone(&metrics),
        app_handle.clone(),
    ).await;

    // --- 步骤 5: 启动下载任务 (只下载 pending_downloads) ---

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
        let control = Arc::clone(&control);
        let pause_notify = Arc::clone(&pause_notify);
        let metrics = Arc::clone(&metrics);
        let manifest_writer = Arc::clone(&manifest_writer);

        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await?;

            const MAX_RETRIES: usize = 5;
            for attempt in 1..=MAX_RETRIES {
                if control.is_cancelled() {
                    return Ok::<(), anyhow::Error>(());
                }
                let result = download_file(
                    &client,
                    &ts_url,
                    &filename,
                    &control,
                    encryption.clone(),
                    pause_notify.clone(),
                    metrics.clone(),
                )
                    .await;

                match result {
                    Ok(DownloadResult::Success(f)) => {
                        log::debug!("✅ 分片 [{}] 下载成功（尝试次数 {}）", f, attempt);

                        if let Some(relative_name) = Path::new(&f).file_name().and_then(|s| s.to_str()) {
                            let mut writer = manifest_writer.lock().await;
                            writer.write_all(format!("{}\n", relative_name).as_bytes()).await?;
                        }

                        metrics.completed_chunks.fetch_add(1, Ordering::Relaxed);
                        ts_files.lock().await.push(f);
                        return Ok(());
                    }
                    Ok(DownloadResult::Skipped(f)) => {
                        log::warn!("🗑️ 分片 [{}] 已被跳过，不再重试", f);
                        return Ok(());
                    }
                    Err(e) => {
                        log::error!("⚠️ 分片 [{}] 第 {} 次下载失败，原因：{}", filename, attempt, e);
                        if attempt < MAX_RETRIES {
                            tokio::time::sleep(Duration::from_millis((attempt * 100) as u64)).await;
                        } else {
                            log::error!("❌ 分片 [{}] 所有重试失败: {:?}, 尝试取消任务", filename, e);
                            control.cancel(); // 触发取消
                        }
                    }
                }
            }
            // 返回 Err 表示该 task 最终失败
            Err(anyhow::anyhow!("分片 [{}] 所有尝试均失败", filename))
        }));
    }

    // --- 步骤 6: 等待所有下载任务完成 ---
    // (逻辑微调，以处理下载失败)
    for handle in handles {
        handle.await??;
    }

    // 检查是否所有分片都已就绪（包括已存在和刚下载的）
    let final_ts_files = Arc::try_unwrap(ts_files).unwrap().into_inner();
    if final_ts_files.len() != total_chunks {
        log::error!(
            "任务 [{} {}] 未能集齐所有分片。预期: {}, 实际: {}. 可能已被取消或下载失败。",
            id,
            name,
            total_chunks,
            final_ts_files.len()
        );

        if !control.is_cancelled() {
            // 如果不是用户主动取消，而是下载失败，则强制取消
            control.cancel();
            // 等待速度监控任务退出
            speed_handle.await?;
            return Err(anyhow::anyhow!("下载失败，部分分片缺失"));
        }
    } else {
        log::info!("任务 [{} {}] 所有分片均已就绪，准备合并。", id, name);
    }

    // 等待速度监控任务退出
    speed_handle.await?;

    // 如果任务被取消，则跳过合并
    if control.is_cancelled() {
        log::warn!("任务 [{} {}] 已被取消，跳过合并。", id, name);
        return Ok(());
    }

    // --- 步骤 7: 合并 TS 文件为 MP4 ---
    merge_files(
        id.clone(),
        &name,
        final_ts_files,
        &temp_dir,
        &output_dir,
        app_handle.clone(),
    )
        .await?;

    // [!] 合并成功后，可以考虑删除清单文件，但保留它也无妨
    // tokio::fs::remove_file(manifest_path).await.ok();

    Ok(())
}

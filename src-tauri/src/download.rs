//! M3U8 分片下载模块，支持AES-128加密流媒体解密
//! 核心特性：
//! - 多线程并发下载
//! - 实时下载速度计算（平滑处理）
//! - 双维度进度显示（分片/字节）
//! - 智能速度单位转换
//! - 暂停/恢复控制

use crate::download_manager::DownloadControl;
use crate::merge::merge_files;
use anyhow::Result;
use openssl::symm::{decrypt, Cipher};
use reqwest::Client;
use std::collections::VecDeque;
use std::time::Duration;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{Mutex, Semaphore},
};
/// 下载指标跟踪结构体（增强版）
#[derive(Clone)]
struct DownloadMetrics {
    total_chunks: usize,
    total_bytes: Arc<AtomicUsize>,
    downloaded_bytes: Arc<AtomicUsize>,
    completed_chunks: Arc<AtomicUsize>,
    speed_samples: Arc<Mutex<VecDeque<(Instant, usize)>>>, // 原始采样数据
    total_bytes_valid: Arc<AtomicBool>,                    // 是否有总字节数
}
impl DownloadMetrics {
    fn new(total_chunks: usize) -> Self {
        Self {
            total_chunks,
            total_bytes: Arc::new(AtomicUsize::new(0)),
            downloaded_bytes: Arc::new(AtomicUsize::new(0)),
            completed_chunks: Arc::new(AtomicUsize::new(0)),
            speed_samples: Arc::new(Mutex::new(VecDeque::with_capacity(10))),
            total_bytes_valid: Arc::new(AtomicBool::new(true)),
        }
    }

    fn mark_total_bytes_invalid(&self) {
        self.total_bytes_valid.store(false, Ordering::Relaxed);
    }
    fn update_total_bytes(&self, size: usize) {
        self.total_bytes.fetch_add(size, Ordering::Relaxed);
    }
    async fn record_chunk(&self, size: usize) {
        let now = Instant::now();
        let mut samples = self.speed_samples.lock().await;
        samples.push_back((now, size));
        if samples.len() > 3200 {
            samples.pop_front();
        }
        drop(samples); // 提前释放锁，减少竞争
        self.downloaded_bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// 获取窗口平均速度（如过去1秒）
    async fn get_windowed_speed(&self) -> (f64, &'static str) {
        let now = Instant::now();
        let samples = self.speed_samples.lock().await;
        let cutoff = now - Duration::from_secs(1);
        let relevant: Vec<_> = samples.iter().filter(|(t, _)| *t >= cutoff).collect();
        if relevant.is_empty() {
            return (0.0, "KB/s");
        }
        let total_bytes: usize = relevant.iter().map(|&(_, size)| size).sum();
        let duration = now.duration_since(cutoff).as_secs_f64().max(0.5); // 避免除零
        let bytes_per_second = total_bytes as f64 / duration;
        let speed_kb = bytes_per_second / 1024.0;
        if speed_kb >= 1024.0 {
            (speed_kb / 1024.0, "MB/s")
        } else {
            (speed_kb, "KB/s")
        }
    }

    /// 获取双维度进度
    async fn get_progress(&self) -> f64 {
        if self.total_bytes_valid.load(Ordering::Relaxed) {
            let total = self.total_bytes.load(Ordering::Relaxed) as f64;
            let done = self.downloaded_bytes.load(Ordering::Relaxed) as f64;
            (done / total * 100.0).clamp(0.0, 100.0)
        } else {
            let chunks = self.completed_chunks.load(Ordering::Relaxed) as f64;
            (chunks / self.total_chunks as f64 * 100.0).clamp(0.0, 100.0)
        }
    }
}

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
    metrics: Arc<DownloadMetrics>, // 新增metrics参数
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
    let mut tasks = Vec::new();
    let mut current_encryption = None;

    // 解析M3U8获取分片列表
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
            tasks.push((ts_url, filename, current_encryption.clone()));
        }
    }

    // 并发控制相关初始化
    let total_chunks = tasks.len();
    let ts_files = Arc::new(Mutex::new(Vec::with_capacity(total_chunks)));
    let semaphore = Arc::new(Semaphore::new(concurrency));

    // 初始化下载指标
    // 预获取所有分片大小
    let metrics = Arc::new(DownloadMetrics::new(tasks.len()));
    let pre_semaphore = Arc::new(Semaphore::new(concurrency));
    let mut pre_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    for (ts_url, _, _) in &tasks {
        let client = client.clone();
        let metrics = metrics.clone();
        let ts_url = ts_url.clone();
        let permit = pre_semaphore.clone();

        pre_handles.push(tokio::spawn(async move {
            let _ = permit.acquire().await; // 忽略Semaphore错误

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

    // 启动速度监控任务
    let speed_handle = {
        let app_handle = app_handle.clone();
        let control = Arc::clone(&control);
        let metrics = Arc::clone(&metrics);
        let id = id.clone();

        tokio::spawn(async move {
            // 创建定时器并消耗初始触发
            let mut interval = tokio::time::interval(Duration::from_millis(200));
            interval.tick().await;
            let mut last_send_time = Instant::now(); // 新增时间记录
            let mut needs_reset = false;

            // 上次发送的数据缓存
            let mut last_data: Option<serde_json::Value> = None;
            loop {
                // 暂停时进入等待状态
                while control.is_paused() {
                    // 挂起监控任务直到恢复
                    control.get_notify().notified().await;
                    needs_reset = true;
                }

                // 恢复后重置定时器
                if needs_reset {
                    interval.reset();
                    needs_reset = false;
                }
                // 等待下一个周期
                interval.tick().await;
                // 检查实际间隔（防止处理逻辑耗时影响）
                let now = Instant::now();
                if now.duration_since(last_send_time) < Duration::from_millis(200) {
                    continue;
                }
                last_send_time = now;

                // 获取状态数据
                let is_cancelled = control.is_cancelled();
                let is_paused = control.is_paused();
                // 获取速度
                let (speed_val, speed_unit) = metrics.get_windowed_speed().await;
                // 获取进度
                let progress = metrics.get_progress().await;

                // 生成合并标志（增加进度保护）
                let is_merge = metrics.total_chunks > 0
                    && metrics.completed_chunks.load(Ordering::Relaxed) == metrics.total_chunks;

                // 构建状态元数据
                let status_info = match (is_cancelled, is_paused, is_merge) {
                    (true, _, _) => (0, "已取消"),          // cancelled
                    (false, true, _) => (1, "已暂停"),      // paused
                    (false, false, false) => (2, "下载中"), // downloading
                    (_, _, true) => (3, "正在合并"),        // merge
                };

                // 生成当前事件数据
                let current_data = serde_json::json!({
                    "id": id,
                    "progress": progress.round() as u32,
                        "speed": format!("{:.2} {}", speed_val, speed_unit),
                    "status": status_info.0,
                    "message": status_info.1,
                    "isMerge": is_merge,
                    "details": {
                        "chunks": metrics.completed_chunks.load(Ordering::Relaxed),
                        "total_chunks": metrics.total_chunks,
                        "downloaded": metrics.downloaded_bytes.load(Ordering::Relaxed),
                        "total_bytes": metrics.total_bytes.load(Ordering::Relaxed),
                    }
                });

                // 发送事件 去重检查
                if last_data.as_ref() != Some(&current_data) {
                    app_handle
                        .emit("download_progress", current_data.clone())
                        .ok();
                    last_data = Some(current_data);
                }

                // 退出条件
                if is_cancelled || is_merge {
                    break;
                }
            }
        })
    };

    // 启动下载任务 ------------------------------------------------------------
    let mut handles = Vec::new();
    for (ts_url, filename, encryption) in tasks {
        let client = client.clone();
        let ts_files = Arc::clone(&ts_files);
        let semaphore = Arc::clone(&semaphore);
        let control = Arc::clone(&control);
        let pause_notify = Arc::clone(&pause_notify);
        let metrics = Arc::clone(&metrics);

        handles.push(tokio::spawn(async move {
            // 获取并发许可
            let _permit = semaphore.acquire().await?;

            const MAX_RETRIES: usize = 5;
            for attempt in 1..=MAX_RETRIES {
                if control.is_cancelled() {
                    return Ok::<(), anyhow::Error>(());
                }
                // 调用真正的下载函数
                let result = download_file(
                    &client,
                    &ts_url,
                    &filename,
                    &control,
                    encryption.clone(),
                    pause_notify.clone(),
                    metrics.clone(),
                ).await;

                match result {
                    Ok(DownloadResult::Success(f)) => {
                        log::info!("✅ 分片 [{}] 下载成功（尝试次数 {}）", f, attempt);
                        metrics.completed_chunks.fetch_add(1, Ordering::Relaxed);
                        ts_files.lock().await.push(f); // 只推送真正成功的
                        return Ok(());
                    }
                    Ok(DownloadResult::Skipped(f)) => {
                        log::warn!("🗑️ 分片 [{}] 已被跳过，不再重试", f);
                        return Ok(());
                    }
                    Err(e) => {
                        log::error!("⚠️ 分片 [{}] 第 {} 次下载失败，原因：{}", filename, attempt, e);
                        if attempt < MAX_RETRIES {
                            tokio::time::sleep(Duration::from_secs((attempt * 2) as u64)).await;
                        } else {
                            log::error!("❌ 分片 [{}] 所有重试失败: {:?}", filename, e);
                        }
                    }
                }
            }
            // 返回 Err 表示该 task 最终失败
            Err(anyhow::anyhow!("分片 [{}] 所有尝试均失败", filename))
        }));
    }

    // 等待所有下载任务完成
    for handle in handles {
        handle.await??;
    }

    // 合并 TS 文件为 MP4
    merge_files(
        id.clone(),
        &name,
        Arc::try_unwrap(ts_files).unwrap().into_inner(),
        &temp_dir,
        &output_dir,
        app_handle.clone(),
    )
    .await?;
    // merge_ts_to_mp4(id.clone(), &name, ts_files, &output_dir, app_handle.clone())
    //     .await
    //     .map_err(|e| e.to_string())?;

    // 等待速度监控任务退出
    speed_handle.await?;

    Ok(())
}

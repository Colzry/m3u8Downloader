//! 下载监控模块
//! 负责实时计算下载速度、检查任务状态（取消）
//! 并通过 Tauri 事件（`download_progress`）向前端报告状态。

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use serde_json::json;

/// 下载指标跟踪结构体（增强版）
/// 负责存储下载过程中的所有实时数据。
#[derive(Clone)]
pub struct DownloadMetrics {
    pub total_chunks: usize,
    pub total_bytes: Arc<AtomicUsize>,
    pub downloaded_bytes: Arc<AtomicUsize>,
    pub completed_chunks: Arc<AtomicUsize>,
    speed_samples: Arc<Mutex<VecDeque<(Instant, usize)>>>, // 原始采样数据 (Instant, bytes)
}

impl DownloadMetrics {
    pub fn new(total_chunks: usize) -> Self {
        Self {
            total_chunks,
            total_bytes: Arc::new(AtomicUsize::new(0)),
            downloaded_bytes: Arc::new(AtomicUsize::new(0)),
            completed_chunks: Arc::new(AtomicUsize::new(0)),
            speed_samples: Arc::new(Mutex::new(VecDeque::with_capacity(10)))
        }
    }

    /// 累加预估的总字节数
    pub fn update_total_bytes(&self, size: usize) {
        self.total_bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// 记录已下载的数据块，用于计算速度。
    pub async fn record_chunk(&self, size: usize) {
        let now = Instant::now();
        let mut samples = self.speed_samples.lock().await;
        samples.push_back((now, size));
        // 限制采样数量，防止内存爆炸
        if samples.len() > 3200 {
            samples.pop_front();
        }
        drop(samples);
        self.downloaded_bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// 获取窗口平均速度（如过去1秒）
    async fn get_windowed_speed(&self) -> (f64, &'static str) {
        let now = Instant::now();
        let samples = self.speed_samples.lock().await;
        // 只考虑过去 1 秒的采样
        let cutoff = now - Duration::from_secs(1);
        let relevant: Vec<_> = samples.iter().filter(|(t, _)| *t >= cutoff).collect();
        if relevant.is_empty() {
            return (0.0, "KB/s");
        }
        let total_bytes: usize = relevant.iter().map(|&(_, size)| size).sum();
        let duration = now.duration_since(cutoff).as_secs_f64().max(0.5); // 避免除零
        let bytes_per_second = total_bytes as f64 / duration;
        let speed_kb = bytes_per_second / 1024.0;

        // 速度单位转换
        if speed_kb >= 1024.0 {
            (speed_kb / 1024.0, "MB/s")
        } else {
            (speed_kb, "KB/s")
        }
    }

    /// 获取进度百分比
    async fn get_progress(&self) -> f64 {
        if self.total_chunks == 0 {
            0.0
        } else {
            // total_chunks: M3U8中总分片数
            // completed_chunks: 已完成（无论是本次还是上次）的分片数
            let chunks = self.completed_chunks.load(Ordering::Relaxed) as f64;

            // 进度 = (已完成分片数 / 总分片数) * 100
            (chunks / self.total_chunks as f64 * 100.0).clamp(0.0, 100.0)
        }
    }
}


/// 运行下载监控任务
/// 这是一个独立的 Tokio 任务，持续监听下载指标并向前端发送事件。
pub async fn run_monitor_task(
    id: String,
    cancelled: Arc<AtomicBool>,
    metrics: Arc<DownloadMetrics>,
    app_handle: AppHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // 创建定时器并消耗初始触发
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        interval.tick().await;

        let mut last_data: Option<serde_json::Value> = None;
        loop {
            // 等待下一个周期
            interval.tick().await;

            // --- 获取并构建状态数据 ---
            let is_cancelled = cancelled.load(Ordering::Relaxed);
            let (speed_val, speed_unit) = metrics.get_windowed_speed().await;
            let progress = metrics.get_progress().await;

            let chunks_completed = metrics.completed_chunks.load(Ordering::Relaxed);
            let chunks_total = metrics.total_chunks;

            // 如果所有分片都已完成，则状态为"正在合并"
            let is_downloaded = chunks_total > 0 && chunks_completed == chunks_total;

            // 构建状态元数据
            let status_info = match (is_cancelled, is_downloaded) {
                (true, _) => (0, "已取消"),          // cancelled
                (false, false) => (2, "下载中"),     // downloading
                (false, true) => (3, "下载完成"),    // merging
            };

            /* status 0-已取消 1-等待中 2-下载中 3-下载完成 4-合并中 5-合并完成 10-初始化或新添加 400-合并失败 */

            // 生成当前事件数据
            let current_data = json!({
                "id": id,
                "progress": progress.round() as u32,
                "speed": format!("{:.2} {}", speed_val, speed_unit),
                "status": status_info.0,
                "message": status_info.1,
                "isMerged": false,
                "details": {
                    "chunks": chunks_completed,
                    "total_chunks": chunks_total,
                    "downloaded": metrics.downloaded_bytes.load(Ordering::Relaxed),
                    "total_bytes": metrics.total_bytes.load(Ordering::Relaxed),
                }
            });

            // 发送事件 (去重检查)
            if last_data.as_ref() != Some(&current_data) {
                app_handle
                    .emit("download_progress", current_data.clone())
                    .ok();
                last_data = Some(current_data);
            }

            // 退出条件：任务被取消或进入合并状态
            if is_cancelled || is_downloaded {
                break;
            }
        }
    })
}
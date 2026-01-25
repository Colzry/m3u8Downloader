use std::sync::atomic::{AtomicBool, Ordering};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

/// 运行时下载任务的句柄
///
/// 存储在 DownloadManager 中，用于关联一个 ID 和它的实时控制器。
pub struct DownloadTask {
    pub cancelled: Arc<AtomicBool>,
    pub temp_dir: String,
    // 如果需要，还可以保存下载任务的 JoinHandle
}

impl DownloadTask {
    pub fn new(temp_dir: String) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            temp_dir,
        }
    }

    /// 取消下载
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// 检查取消状态
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 获取取消标志的克隆
    pub fn get_cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

/// 全局下载管理器（运行时）
///
/// 这是一个全局（Tauri State）单例，用于管理 *所有当前活动* 的下载任务。
///
/// [!] 职责：
/// 1. 注册新的下载任务（`add_task`）。
/// 2. 响应Tauri命令，对 *正在运行* 的任务进行操作（取消并删除）。
pub struct DownloadManager {
    pub tasks: Mutex<HashMap<String, DownloadTask>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    /// 添加任务
    pub async fn add_task(&self, id: String, task: DownloadTask) -> anyhow::Result<()> {
        let mut tasks = self.tasks.lock().await;
        
        // 检查是否已经存在同ID的任务
        if tasks.contains_key(&id) {
            log::warn!("任务 [{}] 已存在，拒绝重复添加。", id);
            return Ok(());
        }
        
        tasks.insert(id.clone(), task);
        log::info!("任务 [{}] 已添加", id);
        
        Ok(())
    }

    /// 取消任务
    /// 
    /// 取消正在运行的下载任务，但保留临时目录以支持断点续传
    pub async fn cancel_task(&self, id: &str) -> anyhow::Result<()>  {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(id) {
            // 设置取消标志
            task.cancel();
            log::info!("任务 [{}] 已取消", id);
        } else {
            log::warn!("任务 [{}] 不存在，无法取消", id);
        }
        Ok(())
    }

    /// 删除任务并清除临时目录
    /// 
    /// 删除任务和清理所有临时文件
    pub async fn delete_task(&self, id: &str) -> anyhow::Result<()> {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(id) {
            // 删除临时目录
            if tokio::fs::try_exists(&task.temp_dir).await.unwrap_or(false) {
                tokio::fs::remove_dir_all(&task.temp_dir).await?;
                log::info!("任务 [{}] 临时下载目录: {} 已删除", id, task.temp_dir);
            }
            log::info!("任务 [{}] 已删除", id);
        } else {
            log::warn!("任务 [{}] 不存在", id);
        }
        Ok(())
    }

    /// 检查任务是否存在
    pub async fn task_exists(&self, id: &str) -> bool {
        self.tasks.lock().await.contains_key(id)
    }

    /// 获取任务的取消标志
    pub async fn get_cancel_flag(&self, id: &str) -> Option<Arc<AtomicBool>> {
        self.tasks.lock().await.get(id).map(|t| t.get_cancel_flag())
    }
}
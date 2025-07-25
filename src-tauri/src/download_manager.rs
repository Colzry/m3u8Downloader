use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, Notify};

#[derive(Default)]
pub struct DownloadControl {
    paused: Arc<AtomicUsize>,    // 0: running, 1: paused
    cancelled: Arc<AtomicUsize>, // 0: 未取消, 1: 已取消
    pause_notify: Arc<Notify>,   // 用于暂停和恢复的通知
}

impl DownloadControl {
    pub fn new() -> Self {
        Self {
            paused: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(AtomicUsize::new(0)),
            pause_notify: Arc::new(Notify::new()), // 初始化 Notify
        }
    }
    // 暂停下载
    pub fn pause(&self) {
        self.paused.store(1, Ordering::SeqCst);
    }

    // 恢复下载
    pub fn resume(&self) {
        self.paused.store(0, Ordering::SeqCst);
        self.pause_notify.notify_waiters();
    }

    // 取消下载
    pub fn cancel(&self) {
        self.cancelled.store(1, Ordering::SeqCst);
        self.pause_notify.notify_waiters(); // 唤醒所有等待任务
    }

    // 检查暂停状态
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst) == 1
    }

    // 检查取消状态
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst) == 1
    }

    // 获取通知器
    pub fn get_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.pause_notify)
    }
}

pub struct DownloadTask {
    pub control: Arc<DownloadControl>,
    pub temp_dir: String,
    // 如果需要，还可以保存下载任务的 JoinHandle
}

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
    pub async fn add_task(&self, id: String, task: DownloadTask) {
        self.tasks.lock().await.insert(id.clone(), task);
        log::debug!("添加下载任务 {}", id);
    }

    /// 暂停任务
    pub async fn pause_task(&self, id: &str) {
        if let Some(task) = self.tasks.lock().await.get(id) {
            task.control.pause();
            log::debug!("{} 暂停下载", id);
        }
    }

    /// 恢复任务
    pub async fn resume_task(&self, id: &str) {
        if let Some(task) = self.tasks.lock().await.get(id) {
            task.control.resume();
            log::debug!("{} 恢复下载", id);
        }
    }

    /// 取消任务
    pub async fn cancel_task(&self, id: &str) {
        if let Some(task) = self.tasks.lock().await.get(id) {
            task.control.cancel();
        }
    }

    /// 删除任务并清除临时目录
    pub async fn delete_task(&self, id: &str) -> anyhow::Result<()> {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(id) {
            // 确保任务已终止
            task.control.cancel();
            // 删除临时目录
            tokio::fs::remove_dir_all(task.temp_dir).await?;
            log::info!("{} 删除临时下载目录", id);
        }
        log::debug!("删除任务 {}", id);
        Ok(())
    }
}

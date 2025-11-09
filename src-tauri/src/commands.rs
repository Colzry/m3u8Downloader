use crate::download::{download_m3u8, DownloadOptions};
use crate::download_manager::{DownloadManager, DownloadTask};
use anyhow::Result;
use std::fs;
use sysinfo::{System, SystemExt};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

#[tauri::command]
pub async fn start_download(
    id: String,
    url: String,
    name: String,
    output_dir: String,
    thread_count: usize,
    app_handle: AppHandle,
    manager: tauri::State<'_, DownloadManager>, // 注入全局管理器
    headers: Option<std::collections::HashMap<String, String>>, // 自定义请求头
) -> Result<(), String> {
    let temp_dir = format!("{}/temp_{}", output_dir, id);

    log::info!("Name: [{}], URL: [{}], ID: [{}] - 开始下载", name, url, id);

    // 检查临时目录是否存在，不存在则创建
    let temp_dir_exists = tokio::fs::try_exists(&temp_dir).await.unwrap_or(false);
    if !temp_dir_exists {
        fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;
        app_handle
            .emit(
                "create_temp_directory",
                serde_json::json!({
                                "id": id,
                                "isCreatedTempDir": true,
                                "message": "已创建临时下载目录",
                }),
            )
            .ok();
        log::info!("任务 [{}] 已创建临时目录: {}", id, &temp_dir);
    } else {
        log::info!("任务 [{}] 临时目录已存在，继续下载: {}", id, &temp_dir);
    }
    
    // 创建任务并添加到管理器
    let task = DownloadTask::new(temp_dir.clone());
    let cancelled = task.get_cancel_flag();
    
    manager
        .add_task(id.clone(), task)
        .await
        .map_err(|e| e.to_string())?;

    // 创建下载选项
    let mut options = DownloadOptions::new();
    if let Some(headers_map) = headers {
        options.headers = headers_map;
    }
    
    // 开始下载 TS 文件到临时目录
    let download_result = download_m3u8(
        id.clone(),
        &url,
        &name,
        &temp_dir,
        &output_dir,
        thread_count,
        cancelled.clone(),
        app_handle.clone(),
        options,
    )
    .await;

    // 下载完成后，从管理器中移除任务
    if let Err(e) = &download_result {
        log::error!("{} 下载失败: {}", id, e);
        // 下载失败，从管理器移除任务（保留临时目录用于断点续传）
        manager.cancel_task(&id)
            .await
            .map_err(|e| e.to_string())?;
    }

    // 检查是否是因为取消而结束的
    // 如果是取消，任务已经从管理器中移除了，不需要再次删除
    // 如果是正常完成，需要删除临时目录
    if !cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        // 下载正常完成（未取消），删除任务并清理临时目录
        manager
            .delete_task(&id)
            .await
            .map_err(|e| format!("删除临时目录失败: {}", e))?;
    }

    // 根据取消标志输出不同的日志
    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        log::info!("任务 [{}] 已取消下载", id);
    } else {
        log::info!("任务 [{}] 已下载完成", id);
    }

    Ok(())
}

/// 取消下载任务
/// 
/// 1. 取消正在运行的下载任务
/// 2. 从管理器中移除任务
/// 3. 保留临时目录以支持断点续传
#[tauri::command]
pub async fn cancel_download(
    id: String,
    manager: tauri::State<'_, DownloadManager>,
) -> Result<(), String> {
    log::info!("取消下载任务: {} (保留临时目录)", id);
    manager.cancel_task(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 删除下载任务并清理临时目录
/// 
/// 1. 取消正在运行的任务（如果存在）
/// 2. 从管理器中移除任务
/// 3. 删除临时目录和所有下载进度
#[tauri::command]
pub async fn delete_download(
    id: String,
    output_dir: String,
    manager: tauri::State<'_, DownloadManager>,
) -> Result<(), String> {
    log::info!("删除下载任务: {}", id);
    
    // 1. 先检查任务是否在管理器中
    let task_exists = manager.task_exists(&id).await;
    
    if task_exists {
        // 任务正在运行，调用 delete_task（会取消并删除临时目录）
        manager.delete_task(&id).await
            .map_err(|e| format!("删除任务失败: {}", e))?;
    } else {
        // 任务不在管理器中（已完成或未开始），直接删除临时目录
        log::info!("任务不在管理器中，直接删除临时目录");
        let temp_dir = format!("{}/temp_{}", output_dir, id);
        
        if tokio::fs::try_exists(&temp_dir).await.unwrap_or(false) {
            tokio::fs::remove_dir_all(&temp_dir)
                .await
                .map_err(|e| format!("删除临时目录失败 ({}): {}", temp_dir, e))?;
            log::info!("已删除临时目录: {}", temp_dir);
        }
    }

    Ok(())
}

/// 获取物理核心数和逻辑线程数
#[tauri::command]
pub fn get_cpu_info() -> (usize, usize) {
    let mut sys = System::new();
    sys.refresh_cpu(); // 刷新CPU信息
    let physical_cores = sys.physical_core_count().unwrap_or(0);
    let logical_cores = sys.cpus().len();
    (physical_cores, logical_cores)
}

/// 删除指定文件
#[tauri::command]
pub async fn delete_file(file_path: String) -> Result<(), String> {
    tokio::fs::remove_file(file_path.clone())
        .await
        .map_err(|e| format!("删除{}文件失败: {}", file_path, e))?;
    Ok(())
}

#[tauri::command]
pub async fn set_minimize_on_close(
    minimize_on_close: bool,
    app_handle: AppHandle,
) -> Result<(), String> {
    let store = app_handle.store("settings.dat").unwrap();
    let old_minimize_on_close: bool = store
        .get("minimize_on_close")
        .and_then(|v| v.as_bool())
        .unwrap_or(true); // 默认值 true
    if minimize_on_close != old_minimize_on_close {
        store.set("minimize_on_close", minimize_on_close);
        store.save().expect("保存Store配置失败");
    }
    Ok(())
}
use crate::download::download_m3u8;
use crate::download_manager::{DownloadControl, DownloadManager, DownloadTask};
use anyhow::Result;
use std::fs;
use std::sync::Arc;
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
) -> Result<(), String> {
    let temp_dir = format!("{}/temp_{}", output_dir, id);

    log::info!("ID: {}, URL: {}, Name: {} - 开始下载", id, url, name);
    println!("ID: {}, URL: {}, Name: {}", id, url, name);

    // 创建临时目录
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
    log::info!("{} 创建临时目录: {}", id, &temp_dir);
    let control = Arc::new(DownloadControl::default());
    // 将任务信息添加到全局管理器
    manager
        .add_task(
            id.clone(),
            DownloadTask {
                control: control.clone(),
                temp_dir: temp_dir.clone(),
            },
        )
        .await;

    // 开始下载 TS 文件到临时目录
    download_m3u8(
        id.clone(),
        &url,
        &name,
        &temp_dir,
        &output_dir,
        thread_count,
        control.clone(),
        app_handle.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;

    // 删除临时目录
    manager
        .delete_task(&id)
        .await
        .expect("临时下载目录删除失败");

    log::info!("{} 下载完成", id);

    Ok(())
}

/// 暂停下载
#[tauri::command]
pub async fn pause_download(
    id: String,
    manager: tauri::State<'_, DownloadManager>,
) -> Result<(), String> {
    manager.pause_task(&id).await;
    Ok(())
}

/// 恢复下载
#[tauri::command]
pub async fn resume_download(
    id: String,
    manager: tauri::State<'_, DownloadManager>,
) -> Result<(), String> {
    manager.resume_task(&id).await;
    Ok(())
}

/// 删除下载
#[tauri::command]
pub async fn delete_download(
    id: String,
    output_dir: String,
    manager: tauri::State<'_, DownloadManager>,
) -> Result<(), String> {
    // 1. 尝试从管理器中移除（如果任务正在运行）
    //    我们将调用 manager.delete_task，它会停止任务并删除目录。
    //    如果任务不存在（例如重启后），它会返回 Ok(()) (根据 download_manager.rs 实现)。
    //    [!] 假设 delete_task 找不到时不会返回 Err
    let _ = manager.delete_task(&id).await;

    // 2. 无论 manager 做了什么，我们都再次尝试删除临时目录
    //    这确保了即使在重启后，目录也会被删除。
    let temp_dir = format!("{}/temp_{}", output_dir, id);

    log::info!("(delete_download) 正在清理临时目录: {}", temp_dir);

    // tokio::fs::remove_dir_all 会在目录不存在时返回 Ok，这是幂等的
    tokio::fs::remove_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("删除临时目录失败 ({}): {}", temp_dir, e))?;

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

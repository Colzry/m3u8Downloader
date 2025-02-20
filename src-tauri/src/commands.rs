use crate::download::download_m3u8;
use crate::download_manager::{DownloadControl, DownloadManager, DownloadTask};
#[warn(unused_imports)]
use crate::merge::{merge_files, merge_ts_to_mp4, sort_ts_files};
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
    let temp_dir = format!("{}/temp_{}_{}", output_dir, name, id);

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
    let mut ts_files = download_m3u8(
        id.clone(),
        &url,
        &name,
        &temp_dir,
        thread_count,
        control.clone(),
        app_handle.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;

    // 将下载好的TS 文件排好序，防止合并的视频播放异常
    sort_ts_files(&mut ts_files);

    // 合并 TS 文件为 MP4
    merge_files(
        id.clone(),
        &name,
        ts_files,
        &temp_dir,
        &output_dir,
        app_handle.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;
    // merge_ts_to_mp4(id.clone(), &name, ts_files, &output_dir, app_handle.clone())
    //     .await
    //     .map_err(|e| e.to_string())?;

    // 删除临时目录
    manager
        .delete_task(&id)
        .await
        .expect("临时下载目录删除失败");

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
    manager: tauri::State<'_, DownloadManager>,
) -> Result<(), String> {
    manager
        .delete_task(&id)
        .await
        .map_err(|e| format!("删除任务失败: {}", e))?;
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
    tokio::fs::remove_file(file_path.clone()).await.map_err(|e| format!("删除{}文件失败: {}", file_path, e))?;
    Ok(())
}


#[tauri::command]
pub async fn set_minimize_on_close(minimize_on_close: bool, app_handle: AppHandle,) -> Result<(), String> {
    let store =  app_handle.store("settings.dat").unwrap();
    let old_minimize_on_close: bool = store.get("minimize_on_close")
        .and_then(|v| v.as_bool())
        .unwrap_or(true); // 默认值 true
    if minimize_on_close != old_minimize_on_close {
        store.set("minimize_on_close", minimize_on_close);
        store.save().expect("保存Store配置失败");
    }
    Ok(())
}
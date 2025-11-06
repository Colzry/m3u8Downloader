use std::path::PathBuf;
use anyhow::Result;
use tauri::{AppHandle, Emitter, Manager};
use tauri::path::BaseDirectory;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 根据当前平台和架构，从 Tauri 资源中解析 ffmpeg 可执行文件的绝对路径。
/// 如果是 Linux/macOS，则将其复制到 AppData 目录并设置执行权限。
pub async fn resolve_ffmpeg_path_and_prepare(handle: &AppHandle) -> Result<PathBuf> {
    // 1. 根据平台和架构确定资源名称
    #[cfg(target_os = "windows")]
    let resource_name = "bin/ffmpeg.exe";
    #[cfg(not(target_os = "windows"))]
    let resource_name = "bin/ffmpeg";

    // 如果没有匹配的平台/架构配置，则抛出错误
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    return Err(anyhow::anyhow!("不支持的操作系统或架构"));

    // 2. 获取打包资源的运行时绝对路径
    let resource_path = handle
        .path()
        .resolve(resource_name, BaseDirectory::Resource)
        .map_err(|e| anyhow::anyhow!("无法解析 ffmpeg 资源路径 ({}): {}", resource_name, e))?;

    // 3. Windows 直接返回资源路径，不需要复制
    #[cfg(target_os = "windows")]
    {
        return Ok(resource_path);
    }

    // 4. Linux/macOS: 复制到 AppData 目录并设置执行权限
    #[cfg(not(target_os = "windows"))]
    {
        let app_data_dir = handle.path().app_data_dir()
            .map_err(|e| anyhow::anyhow!("无法获取 AppData 目录: {}", e))?;

        // 确保目录存在
        fs::create_dir_all(&app_data_dir).await?;

        let target_name = resource_path.file_name().ok_or_else(|| anyhow::anyhow!("无效的 ffmpeg 文件名"))?;
        let target_path = app_data_dir.join(target_name);

        // 复制文件（仅当目标文件不存在时才复制和设置权限）
        if !target_path.exists() {
            // 使用 tokio::fs::copy 进行异步复制
            tokio::fs::copy(&resource_path, &target_path).await?;
            log::info!("已将 ffmpeg 资源文件复制到: {}", target_path.display());

            // 设置 Linux/macOS 执行权限
            use std::os::unix::fs::PermissionsExt;
            // 设置权限为 rwxr-xr-x (0o755)
            let perms = std::fs::Permissions::from_mode(0o755);
            // 这里使用 std::fs::set_permissions 因为它不是 I/O 密集型操作
            std::fs::set_permissions(&target_path, perms)
                .map_err(|e| anyhow::anyhow!("无法为 ffmpeg 设置执行权限 ({}): {}", target_path.display(), e))?;
            log::info!("已设置 ffmpeg 执行权限: {}", target_path.display());
        }

        Ok(target_path)
    }
}

// 下载的ts文件排序
fn sort_ts_files(ts_files: &mut Vec<String>) {
    ts_files.sort_by(|a, b| {
        let extract_index = |s: &str| {
            s.rsplit('_')
                .next()
                .and_then(|part| part.strip_suffix(".ts"))
                .and_then(|num| num.parse::<usize>().ok())
                .unwrap_or(0)
        };
        extract_index(a).cmp(&extract_index(b))
    });
}

// 使用ffmpeg合并ts
pub async fn merge_files(
    id: String,
    name: &str,
    mut ts_files: Vec<String>,
    temp_dir: &str,
    output_dir: &str,
    app_handle: AppHandle,
) -> Result<()> {
    // 创建 concat.txt 文件路径
    let concat_file_path = format!("{}/concat.txt", temp_dir);
    let mut concat_file = File::create(&concat_file_path).await?;

    // 将下载好的TS 文件排好序，防止合并的视频播放异常
    sort_ts_files(&mut ts_files);

    // 异步写入 TS 文件列表到 concat.txt
    for ts_file in ts_files {
        concat_file
            .write_all(format!("file '{}'\n", ts_file).as_bytes())
            .await?;
    }

    // 关闭文件以确保写入完成
    drop(concat_file);

    // 输出文件路径
    let output_file = format!("{}/{}.mp4", output_dir, name);

    // 获取可执行文件所在的目录，并进行复制和设置权限
    let ffmpeg_path = resolve_ffmpeg_path_and_prepare(&app_handle).await?;

    log::info!("ffmpeg {} -> {}", output_file, ffmpeg_path.to_str().unwrap());

    // 将 PathBuf 转换为 &str，用于后续命令
    let ffmpeg = ffmpeg_path.to_str()
        .ok_or_else(|| anyhow::anyhow!("无效的 ffmpeg 路径 (包含非UTF8字符)"))?;

    // 检查 ffmpeg 是否存在
    if !std::path::Path::new(ffmpeg).exists() {
        return Err(anyhow::anyhow!("ffmpeg binary not found at {}", ffmpeg));
    }



    // 创建 Command
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new(ffmpeg);
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // 隐藏窗口
    #[cfg(not(target_os = "windows"))]
    let cmd = std::process::Command::new(ffmpeg);

    // 通知前端开始合并 status 10 - 开始合并  11 - 合并成功  12 - 合并失败
    app_handle
        .emit(
            "start_merge_video",
            serde_json::json!({
                "id": id,
                "isMerge": false,
                "status": 10,
                "message": "开始合并",
            }),
        )
        .ok();
    log::info!("{} 开始合并", id);

    let status = tokio::process::Command::from(cmd)
        .args(&[
            "-y", // 覆盖输出文件
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &concat_file_path,
            "-c",
            "copy",
            &output_file,
        ])
        .status()
        .await?;

    // 检查 FFmpeg 状态
    if !status.success() {
        app_handle
            .emit(
                "merge_video",
                serde_json::json!({
                    "id": id,
                    "isMerge": false,
                    "status": 12,
                    "message": "合并失败",
                }),
            )
            .ok();
        log::error!("{} 合并失败", id);
        return Err(anyhow::anyhow!("FFmpeg merge failed"));
    }

    // 通知前端合并完成
    app_handle
        .emit(
            "merge_video",
            serde_json::json!({
                "id": id,
                "isMerge": true, // 合并完成
                "status": 11,
                "message": "合并成功",
                "file": output_file,
            }),
        )
        .ok();
    log::info!("{} 合并完成", id);
    Ok(())
}

#[allow(dead_code)]
pub async fn merge_ts_to_mp4(
    id: String,
    name: &str,
    ts_files: Vec<String>,
    output_dir: &str,
    app_handle: AppHandle,
) -> Result<()> {
    // 输出文件路径
    let output_file_path = format!("{}/{}.mp4", output_dir, name);
    let mut output_file = File::create(&output_file_path).await?;

    let total_files = ts_files.len();
    let mut completed_files = 0;

    for ts_file in ts_files {
        let mut input_file = File::open(ts_file).await?;
        let mut buffer = Vec::new();

        // 异步读取 TS 文件内容
        input_file.read_to_end(&mut buffer).await?;
        output_file.write_all(&buffer).await?;

        completed_files += 1;
        let progress = (completed_files as f32 / total_files as f32) * 100.0;

        // 发送进度更新到前端
        app_handle
            .emit(
                "merge_video",
                serde_json::json!({
                    "id": id,
                    "status": 11,
                    "isMerge": false, // 是否合并完成
                    "progress": progress.floor() as u32,  // 向下取整
                }),
            )
            .ok();
    }

    // 合并完成通知
    app_handle
        .emit(
            "merge_video",
            serde_json::json!({
                "id": id,
                "isMerge": true,
                "status": 12,
                "message": "合并成功",
                "file": output_file_path,
            }),
        )
        .ok();

    Ok(())
}

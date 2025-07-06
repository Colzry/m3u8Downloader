use anyhow::Result;
use tauri::{AppHandle, Emitter};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    log::info!("{} 开始合并TS文件", id);
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

    // 构造跨平台命令
    #[cfg(target_os = "windows")]
    let ffmpeg = "bin/win/ffmpeg.exe";
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let ffmpeg = "bin/linux/ffmpeg";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let ffmpeg = "bin/darwin/arm64/ffmpeg";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    let ffmpeg = "bin/darwin/x64/ffmpeg";

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
                    "status": 0,
                    "message": "合并失败",
                }),
            )
            .ok();
        log::error!("{} 视频合并失败", id);
        return Err(anyhow::anyhow!("FFmpeg merge failed"));
    }

    // 通知前端合并完成
    app_handle
        .emit(
            "merge_video",
            serde_json::json!({
                "id": id,
                "isMerge": true, // 合并完成
                "status": 0,
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
                    "status": 0,
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
                "status": 0,
                "message": "合并成功",
                "file": output_file_path,
            }),
        )
        .ok();

    Ok(())
}

use anyhow::Result;
use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::process;

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
        let app_data_dir = handle
            .path()
            .app_data_dir()
            .map_err(|e| anyhow::anyhow!("无法获取 AppData 目录: {}", e))?;

        // 确保目录存在
        tokio::fs::create_dir_all(&app_data_dir).await?;

        let target_name = resource_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("无效的 ffmpeg 文件名"))?;
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
            std::fs::set_permissions(&target_path, perms).map_err(|e| {
                anyhow::anyhow!(
                    "无法为 ffmpeg 设置执行权限 ({}): {}",
                    target_path.display(),
                    e
                )
            })?;
            log::info!("已设置 ffmpeg 执行权限: {}", target_path.display());
        }

        Ok(target_path)
    }
}

/// 创建带平台特性的 Command
#[cfg(target_os = "windows")]
fn create_ffmpeg_command(ffmpeg: &str) -> process::Command {
    let mut cmd = process::Command::new(ffmpeg);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

#[cfg(not(target_os = "windows"))]
fn create_ffmpeg_command(ffmpeg: &str) -> process::Command {
    process::Command::new(ffmpeg)
}

/// 合并失败统一处理
async fn fail_merge(app_handle: &AppHandle, id: &str) -> Result<()> {
    app_handle.emit(
        "merge_video",
        serde_json::json!({
            "id": id,
            "isMerged": false,
            "status": 400,
            "message": "合并失败"
        }),
    )?;
    Ok(())
}

/// 移除文件名中的非法字符，以确保文件名在操作系统层面合法。
fn sanitize_filename(name: &str) -> String {
    // Windows 文件系统不允许的字符集： \ / : * ? " < > |
    let illegal_chars = r#"\/*:?"<>|"#;

    // 此外，Windows 不允许以空格或点开头/结尾
    // 也不允许使用 CON, PRN, AUX, NUL, COMx, LPTx 作为文件名（无论大小写）

    // 移除非法字符
    let sanitized: String = name
        .chars()
        .filter(|c| !illegal_chars.contains(*c))
        .collect();

    // 移除开头/结尾的空格或点
    let mut sanitized = sanitized.trim_matches(|c| c == ' ' || c == '.').to_string();

    // 检查 Windows 保留名（虽然大部分都会被前面的非法字符和 trim 处理，但为了健壮性保留）
    #[cfg(target_os = "windows")]
    {
        // Windows 保留文件名（无论大小写，无扩展名时）
        let reserved_names = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        // 获取文件名（不含路径）和扩展名
        let name_path = std::path::Path::new(&sanitized);
        if let Some(file_name) = name_path.file_name().and_then(|n| n.to_str()) {
            let name_upper = file_name.to_uppercase();
            // 检查无扩展名的部分
            if let Some(name_no_ext) = name_upper.split('.').next() {
                if reserved_names
                    .iter()
                    .any(|&reserved| reserved == name_no_ext)
                {
                    // 如果匹配保留名，则在文件名后附加一个下划线
                    if let Some(ext) = name_path.extension().and_then(|e| e.to_str()) {
                        // 有扩展名
                        sanitized = format!("{}_{}.{}", name_no_ext, "_", ext);
                    } else {
                        // 无扩展名
                        sanitized = format!("{}_", name_no_ext);
                    }
                }
            }
        }
    }

    // 如果净化后为空，返回一个默认值
    if sanitized.is_empty() {
        "output".to_string()
    } else {
        sanitized
    }
}

/// 使用ffmpeg合并ts
pub async fn merge_files(
    id: String,
    name: &str,
    ts_files: Vec<String>,
    temp_dir: &str,
    output_dir: &str,
    app_handle: AppHandle,
) -> Result<()> {
    // 1. 创建 concat.txt
    let concat_file_path = format!("{}/concat.txt", temp_dir);
    let mut concat_file = File::create(&concat_file_path).await?;
    
    for ts_file in &ts_files {
        // 正确转义单引号
        let escaped = ts_file.replace('\\', r"\\").replace('\'', r"\'");
        concat_file
            .write_all(format!("file '{}'\n", escaped).as_bytes())
            .await?;
    }
    concat_file.flush().await?;
    drop(concat_file);

    // 2. 净化文件名并构建输出路径
    let sanitized_name = sanitize_filename(name);
    let output_path = std::path::Path::new(output_dir).join(format!("{}.mp4", sanitized_name));
    let output_file_str = output_path.to_string_lossy();

    // 3. 获取 ffmpeg
    let ffmpeg_path = resolve_ffmpeg_path_and_prepare(&app_handle).await?;
    let ffmpeg = ffmpeg_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("ffmpeg 路径无效"))?;

    // 通知开始
    app_handle.emit(
        "start_merge_video",
        serde_json::json!({
            "id": &id,
            "isMerged": false,
            "status": 4,
            "message": "开始合并"
        }),
    )?;

    let status = create_ffmpeg_command(ffmpeg)
        .args([
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            &concat_file_path,
            "-c",
            "copy",
            "-map",
            "0",
            "-avoid_negative_ts",
            "make_zero",
            "-bsf:a",
            "aac_adtstoasc",
            &output_file_str,
        ])
        .status()
        .await?;

    if !status.success() {
        fail_merge(&app_handle, &id).await?;
        return Err(anyhow::anyhow!("FFmpeg 合并失败"));
    }

    // 成功
    app_handle.emit(
        "merge_video",
        serde_json::json!({
            "id": id,
            "isMerged": true,
            "status": 5,
            "message": "合并成功",
            "file": output_file_str,
        }),
    )?;

    log::info!("{} 合并完成 → {}", id, output_file_str);
    Ok(())
}

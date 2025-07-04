use fern::Dispatch;
use log::LevelFilter;
use chrono::Local;
use std::path::PathBuf;

pub mod rotate;

/// 获取应用程序安装路径（或当前运行目录作为回退方案）
pub fn get_install_dir() -> Result<PathBuf, String> {
    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;

    let install_dir = exe_path.parent()
        .ok_or("找不到父目录")?
        .to_path_buf();

    Ok(install_dir)
}

/// 初始化带有日志滚动的 logging 系统
pub fn setup_logging() -> Result<(), String> {
    // 创建 logs 文件夹（如果不存在）
    let log_dir = rotate::get_log_dir_path();
    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    }

    // 初始化一次后，清除历史旧日志
    rotate::clean_old_logs();

    // 根据当前日期构建日志文件名（每天一个）
    let mut log_file_path = log_dir;
    log_file_path.push(rotate::get_today_log_file_name());

    // 打开文件追加写入
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)
        .map_err(|e| e.to_string())?;

    Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(LevelFilter::Info)
        .chain(std::io::stdout())
        .chain(log_file)
        .apply()
        .map_err(|e| e.to_string())?;

    log::info!("✅ 日志模块加载成功");

    Ok(())
}

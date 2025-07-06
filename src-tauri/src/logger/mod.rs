use std::ffi::OsStr;
use std::fs;
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

/// 从安装目录查找是否含有标记日志级别的 flag 文件（无视大小写、后缀）
fn detect_log_level_from_files(install_dir: &PathBuf) -> LevelFilter {
    // 按照优先级排列的关键字列表，越靠前越高优先级
    const LEVEL_MAP: &[(&str, LevelFilter)] = &[
        ("off", LevelFilter::Off),
        ("error", LevelFilter::Error),
        ("warn", LevelFilter::Warn),
        ("info", LevelFilter::Info),
        ("debug", LevelFilter::Debug),
        ("trace", LevelFilter::Trace),
    ];
    // 收集当前目录下所有文件名（不包括子目录）
    let entries = match fs::read_dir(install_dir) {
        Ok(entries) => entries,
        Err(_) => return LevelFilter::Info,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match path.file_stem().and_then(OsStr::to_str) {
            Some(name) => name.to_lowercase(),
            None => continue,
        };
        for (keyword, level) in LEVEL_MAP {
            if filename == *keyword {
                return *level;
            }
        }
    }
    LevelFilter::Info // 默认级别
}

/// 初始化带有日志滚动的 logging 系统
pub fn setup_logging() -> Result<(), String> {
    let install_dir = get_install_dir()?;

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

    // 读取配置文件获取当前日志等级
    let level = detect_log_level_from_files(&install_dir);

    Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout())
        .chain(log_file)
        .apply()
        .map_err(|e| e.to_string())?;

    log::info!("✅ 日志模块加载成功");
    log::info!("ℹ️ 当前日志级别为: {:?}", level);

    Ok(())
}
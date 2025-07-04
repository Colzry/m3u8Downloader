use chrono::Local;
use std::fs;
use std::path::PathBuf;

const MAX_LOG_KEEP_DAYS: i64 = 30; // 只保存最近30天日志

// 获取当日的日志文件名（如 logs/2025-04-05.log）
pub fn get_today_log_file_name() -> String {
    Local::now().format("%Y-%m-%d").to_string() + ".log"
}

// 获取 log 路径，同时创建目录（如果需要）
pub fn get_log_dir_path() -> PathBuf {
    super::get_install_dir().unwrap_or_else(|_| std::env::current_dir().unwrap()).join("logs")
}

// 清除旧日志（大于30天前）
pub fn clean_old_logs() {
    let log_dir = get_log_dir_path();

    // 判断目录是否存在
    if !log_dir.exists() {
        eprintln!("📁 日志目录不存在，跳过清理");
        return;
    }

    // 获取 ReadDir 迭代器
    let entries = match fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("⚠️ 无法读取日志目录 {}: {:?}", log_dir.display(), e);
            return;
        }
    };

    let now = Local::now();

    for entry in entries {
        let path = match entry {
            Ok(e) => e.path(),
            Err(e) => {
                println!("⚠️ 无法读取文件项: {}", e);
                continue;
            }
        };

        let meta = match fs::metadata(&path) {
            Ok(meta) => meta,
            Err(e) => {
                println!("⚠️ 无法获取元数据 {}: {}", path.display(), e);
                continue;
            }
        };

        let modified_time = match meta.modified() {
            Ok(modified) => chrono::DateTime::<Local>::from(modified),
            Err(e) => {
                println!("⚠️ 无法获取修改时间 {}: {}", path.display(), e);
                continue;
            }
        };

        if now.signed_duration_since(modified_time).num_days() > MAX_LOG_KEEP_DAYS {
            if let Err(e) = fs::remove_file(&path) {
                eprintln!("⚠️ 删除失败 {}: {}", path.display(), e);
            } else {
                println!("🗑️ 已删除旧日志: {}", path.display());
            }
        }
    }
}


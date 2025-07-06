use crate::commands::{
    delete_download, delete_file, get_cpu_info, pause_download, resume_download,
    set_minimize_on_close, start_download,
};
use crate::download_manager::DownloadManager;
use tauri::tray::{MouseButton, TrayIconEvent};
use tauri::{async_runtime, Emitter, Manager, WindowEvent};
use tauri_plugin_store::StoreExt;
pub mod commands;
mod download;
mod download_manager;
mod merge;
mod logger;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 创建 Tokio 运行时
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // 设置 Tauri 的异步运行时
    async_runtime::set(runtime.handle().clone());

    if let Err(e) = logger::setup_logging() {
        eprintln!("⚠️ 初始化日志失败：{}", e);
    }

    // 启动 Tauri 应用程序
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DownloadManager::new()) // 注册下载全局状态管理
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            enable_tray(app)?;
            // 初始化 store 并读取配置
            let store = app.store("settings.dat")?;
            // 监听窗口关闭事件
            let main_window = app.get_webview_window("main").unwrap();

            main_window.clone().on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if store
                        .get("minimize_on_close")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true)
                    {
                        api.prevent_close(); // 阻止默认关闭行为
                        main_window.hide().unwrap(); // 隐藏窗口
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            pause_download,
            resume_download,
            delete_download,
            get_cpu_info,
            delete_file,
            set_minimize_on_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn enable_tray(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::{
        menu::{MenuBuilder, MenuItem},
        tray::TrayIconBuilder,
    };

    // 打开按钮
    let open_i = MenuItem::with_id(app, "open", "打开", true, None::<&str>)?;
    // 退出按钮
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    // 设置按钮
    let settings_i = MenuItem::with_id(app, "settings", "设置", true, None::<&str>).unwrap();

    // 按照一定顺序 把按钮 放到 菜单里
    let menu = MenuBuilder::new(app)
        .item(&open_i)
        .separator() // 分割线
        .item(&settings_i)
        .separator() // 分割线
        .item(&quit_i)
        .build()
        .unwrap();

    let _tray = TrayIconBuilder::with_id("tray")
        .show_menu_on_left_click(false)
        .icon(app.default_window_icon().unwrap().clone()) // 默认的图片
        // .icon(Image::from_bytes(include_bytes!("../icons/light@2x.png")).expect("REASON")) // 自定义的图片
        .menu(&menu)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } => {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                let window = app.get_webview_window("main").unwrap();
                window.show().unwrap();
                window.set_focus().unwrap();
            }
            "settings" => {
                // windows failed to open second window, issue: https://github.com/tauri-apps/tauri/issues/11144 https://github.com/tauri-apps/tauri/issues/8196
                let _ = app.emit("open_settings", "");
                let window = app.get_webview_window("main").unwrap();
                window.show().unwrap();
                window.set_focus().unwrap();
            }
            "quit" => {
                app.exit(0);
            }
            _ => {
                println!("menu item {:?} not handled", event.id);
            }
        })
        .build(app);
    Ok(())
}

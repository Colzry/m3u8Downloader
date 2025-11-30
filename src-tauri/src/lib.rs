use crate::commands::{
    cancel_download, check_update, delete_download, delete_file, get_cpu_info, save_settings,
    save_store_file, start_download,
};
use crate::download_manager::DownloadManager;
use tauri::tray::{MouseButton, TrayIconEvent};
use tauri::{async_runtime, Emitter, Manager, WindowEvent};
use tauri_plugin_store::StoreExt;
pub mod commands;
mod download;
mod download_manager;
mod download_monitor;
mod logger;
mod merge;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 创建 Tokio 运行时
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // 设置 Tauri 的异步运行时
    async_runtime::set(runtime.handle().clone());

    // 启动 Tauri 应用程序
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 多次启动聚焦主窗口
            if let Some(window) = app.get_webview_window("main") {
                // 1. 检查是否最小化，如果是则恢复
                if let Ok(true) = window.is_minimized() {
                    if let Err(e) = window.unminimize() {
                        eprintln!("无法恢复窗口: {}", e);
                    }
                }
                // 2. 检查是否隐藏，如果是则显示
                if let Ok(false) = window.is_visible() {
                    if let Err(e) = window.show() {
                        eprintln!("无法显示窗口: {}", e);
                    }
                }

                // 3. 最后，将窗口带到前台并聚焦
                if let Err(e) = window.set_focus() {
                    eprintln!("无法聚焦窗口: {}", e);
                }
            }
        }))
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(DownloadManager::new()) // 注册下载全局状态管理
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            if let Err(e) = logger::setup_logging(&app.handle()) {
                eprintln!("⚠️ 初始化Tauri日志失败：{}", e);
            }

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

                        // 在 Linux 上，只最小化，不隐藏，避免窗口控件失效
                        #[cfg(target_os = "linux")]
                        {
                            let _ = main_window.minimize(); // 最小化即可
                        }

                        // 在 Windows/macOS 上，可以 hide（或 minimize + hide）
                        #[cfg(not(target_os = "linux"))]
                        {
                            let _ = main_window.hide();
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            cancel_download,
            delete_download,
            get_cpu_info,
            delete_file,
            save_settings,
            check_update,
            save_store_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn restore_window(window: &tauri::WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
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
        .build()?;

    let _tray = TrayIconBuilder::with_id("tray")
        .show_menu_on_left_click(false)
        .icon(app.default_window_icon().unwrap().clone()) // 默认的图片
        // .icon(Image::from_bytes(include_bytes!("../icons/light@2x.png")).expect("REASON")) // 自定义的图片
        .tooltip("m3u8下载器")
        .menu(&menu)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } => {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    restore_window(&window);
                }
            }
            _ => {}
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                let window = app.get_webview_window("main").unwrap();
                restore_window(&window);
            }
            "settings" => {
                // windows failed to open second window, issue: https://github.com/tauri-apps/tauri/issues/11144 https://github.com/tauri-apps/tauri/issues/8196
                let _ = app.emit("open_settings", "");
                let window = app.get_webview_window("main").unwrap();
                restore_window(&window);
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

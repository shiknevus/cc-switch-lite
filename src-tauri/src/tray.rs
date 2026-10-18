//! 系统托盘：关窗后留在托盘继续后台同步，托盘菜单负责唤回主窗口、立即同步、退出。
//!
//! 轻量版没有完整版的供应商切换菜单，托盘只有三项固定动作，因此这里不用维护
//! 菜单模型，只在创建时挂一次菜单和事件处理。

use tauri::menu::{Menu, MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::store::AppState;

/// 托盘图标 id，退出/重建时按它查回同一个托盘实例。
pub const TRAY_ID: &str = "cc-switch-lite";

/// 菜单项 id。
pub const MENU_SHOW: &str = "tray_show_main";
pub const MENU_SYNC_NOW: &str = "tray_sync_now";
pub const MENU_QUIT: &str = "tray_quit";

/// 托盘悬停提示；Windows 的提示缓冲区是 128 个 UTF-16 单元，这里的长度远够。
const TRAY_TOOLTIP: &str = "CC Switch Lite";

/// 按界面语言取托盘文案：设置存在数据库里，托盘在 setup 阶段建，早于前端读设置，
/// 这里直接读一次库，拿不到就按系统语言猜。
fn tray_texts(app: &AppHandle) -> (String, String, String) {
    let language = app
        .try_state::<AppState>()
        .and_then(|state| crate::settings::get_settings_from_db(&state.db).ok())
        .map(|settings| settings.language)
        .unwrap_or_else(|| "zh".to_string());

    if language.starts_with("zh") {
        (
            "显示主窗口".to_string(),
            "立即同步用量".to_string(),
            "退出 CC Switch Lite".to_string(),
        )
    } else if language.starts_with("ja") {
        (
            "メインウィンドウを表示".to_string(),
            "使用量を今すぐ同期".to_string(),
            "CC Switch Lite を終了".to_string(),
        )
    } else {
        (
            "Show Main Window".to_string(),
            "Sync Usage Now".to_string(),
            "Quit CC Switch Lite".to_string(),
        )
    }
}

/// 建托盘和固定菜单。重复调用会先移掉旧托盘，避免留两个图标。
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let (show, sync_now, quit) = tray_texts(app);

    let show_item = MenuItem::with_id(app, MENU_SHOW, &show, true, None::<&str>)?;
    let sync_item = MenuItem::with_id(app, MENU_SYNC_NOW, &sync_now, true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, MENU_QUIT, &quit, true, None::<&str>)?;
    let menu: Menu<_> = MenuBuilder::new(app)
        .item(&show_item)
        .item(&sync_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TRAY_TOOLTIP)
        .menu(&menu)
        // 左键单击显示主窗口，菜单只在右键弹（下面的 on_tray_icon_event 负责左键）
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu_event(app, &event.id.0))
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = tray_icon(app) {
        builder = builder.icon(icon.0).icon_as_template(icon.1);
    }

    let _ = app.remove_tray_by_id(TRAY_ID);
    builder.build(app)?;
    log::info!("托盘已创建");
    Ok(())
}

/// macOS 用单色模板图跟随菜单栏深浅色，其它平台用应用图标。
fn tray_icon(app: &AppHandle) -> Option<(tauri::image::Image<'static>, bool)> {
    #[cfg(target_os = "macos")]
    {
        const ICON_BYTES: &[u8] = include_bytes!("../icons/tray/macos/statusbar_template_3x.png");
        match tauri::image::Image::from_bytes(ICON_BYTES) {
            Ok(icon) => return Some((icon, true)),
            Err(err) => log::warn!("读取 macOS 托盘图标失败，退回应用图标: {err}"),
        }
    }
    let _ = app;
    app.default_window_icon()
        .map(|icon| (icon.clone().to_owned(), false))
}

/// 显示并聚焦主窗口。关到托盘时窗口只是 hide，这里把它恢复出来。
pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("唤回主窗口失败：找不到 label=main 的窗口");
        return;
    };
    #[cfg(target_os = "windows")]
    {
        // hide 到托盘时设过 skip_taskbar，只 show 不会把它改回来，
        // 否则窗口可见但不在任务栏，最小化后就找不到了。
        let _ = window.set_skip_taskbar(false);
    }
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        MENU_SHOW => show_main_window(app),
        MENU_SYNC_NOW => {
            let db = app
                .try_state::<AppState>()
                .map(|state| state.db.clone())
                .expect("托盘同步时 AppState 尚未注册");
            tauri::async_runtime::spawn(async move {
                // 用户明确要求同步：有扫描在跑就排在它后面，而不是静默丢掉这次点击。
                let _guard = crate::services::session_usage::session_sync_mutex().lock().await;
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    crate::services::session_usage::sync_all_unlocked(&db)
                })
                .await;
                log::info!("托盘「立即同步」完成");
            });
        }
        MENU_QUIT => {
            log::info!("托盘菜单退出应用");
            app.exit(0);
        }
        other => log::debug!("未处理的托盘菜单事件: {other}"),
    }
}

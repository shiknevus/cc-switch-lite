//! 桌面悬浮顶栏/侧栏管理模块
//!
//! 负责 float_bar 独立窗口的生命周期、位置计算、贴边隐藏与悬停展开控制。
//! 默认贴顶居中常驻，未激活时仅露出一小条微光胶囊，鼠标悬停时平滑伸展为单行用量条。

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};

use crate::settings;
use crate::store::AppState;

pub const FLOAT_BAR_LABEL: &str = "float_bar";
pub const FLOAT_BAR_WIDTH: f64 = 640.0;
pub const FLOAT_BAR_HEIGHT_EXPANDED: f64 = 56.0;
pub const FLOAT_BAR_HEIGHT_COLLAPSED: f64 = 10.0;

/// 将悬浮条对齐到当前/主显示器顶部位置
pub fn align_float_bar_to_screen(window: &WebviewWindow, position_mode: &str) {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let mon_pos = monitor.position();
        let mon_size = monitor.size();

        let mon_x = mon_pos.x as f64 / scale;
        let mon_y = mon_pos.y as f64 / scale;
        let mon_w = mon_size.width as f64 / scale;

        let target_x = match position_mode {
            "top_right" => mon_x + mon_w - FLOAT_BAR_WIDTH - 24.0,
            "top_left" => mon_x + 24.0,
            _ => mon_x + (mon_w - FLOAT_BAR_WIDTH) / 2.0, // top_center default
        };
        let target_y = mon_y;

        let _ = window.set_position(LogicalPosition::new(target_x, target_y));
    }
}

/// 启动时初始化悬浮条窗口
pub fn setup_float_bar(app: &AppHandle) {
    let Some(window) = app.get_webview_window(FLOAT_BAR_LABEL) else {
        log::warn!("找不到 label={FLOAT_BAR_LABEL} 的窗口，跳过悬浮顶栏初始化");
        return;
    };

    let _ = window.set_decorations(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_skip_taskbar(true);

    let (enabled, pinned, position) = app
        .try_state::<AppState>()
        .and_then(|state| settings::get_settings_from_db(&state.db).ok())
        .map(|s| (s.float_bar_enabled, s.float_bar_pinned, s.float_bar_position))
        .unwrap_or((true, false, "top_center".to_string()));

    align_float_bar_to_screen(&window, &position);

    let initial_height = if pinned {
        FLOAT_BAR_HEIGHT_EXPANDED
    } else {
        FLOAT_BAR_HEIGHT_COLLAPSED
    };
    let _ = window.set_size(LogicalSize::new(FLOAT_BAR_WIDTH, initial_height));

    if enabled {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        log::info!("桌面悬浮顶栏已初始化并显示");
    } else {
        let _ = window.hide();
        log::info!("桌面悬浮顶栏设置关闭，保持隐藏");
    }
}

/// 前端悬停/离开时动态调整窗口高度
#[tauri::command]
pub async fn set_float_bar_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window(FLOAT_BAR_LABEL) else {
        return Err("float_bar window not found".to_string());
    };

    let pinned = app
        .try_state::<AppState>()
        .and_then(|state| settings::get_settings_from_db(&state.db).ok())
        .map(|s| s.float_bar_pinned)
        .unwrap_or(false);

    // 如果处于钉住状态，保持完全展开高度
    let target_height = if pinned || expanded {
        FLOAT_BAR_HEIGHT_EXPANDED
    } else {
        FLOAT_BAR_HEIGHT_COLLAPSED
    };

    let _ = window.set_size(LogicalSize::new(FLOAT_BAR_WIDTH, target_height));
    let _ = window.set_always_on_top(true);
    Ok(())
}

/// 切换悬浮条可见性
#[tauri::command]
pub async fn toggle_float_bar(app: AppHandle, enabled: Option<bool>) -> Result<bool, String> {
    let Some(window) = app.get_webview_window(FLOAT_BAR_LABEL) else {
        return Err("float_bar window not found".to_string());
    };

    let (current_settings, db) = app
        .try_state::<AppState>()
        .map(|state| (settings::get_settings_from_db(&state.db).ok(), state.db.clone()))
        .unwrap_or((None, crate::database::Database::init().map(std::sync::Arc::new).unwrap()));

    let mut current_settings = current_settings.unwrap_or_default();
    let new_enabled = enabled.unwrap_or(!current_settings.float_bar_enabled);
    current_settings.float_bar_enabled = new_enabled;

    if let Err(e) = settings::save_settings_to_db(&db, &current_settings) {
        log::warn!("保存悬浮条状态失败: {e}");
    }

    if new_enabled {
        align_float_bar_to_screen(&window, &current_settings.float_bar_position);
        let height = if current_settings.float_bar_pinned {
            FLOAT_BAR_HEIGHT_EXPANDED
        } else {
            FLOAT_BAR_HEIGHT_COLLAPSED
        };
        let _ = window.set_size(LogicalSize::new(FLOAT_BAR_WIDTH, height));
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        let _ = window.set_skip_taskbar(true);
    } else {
        let _ = window.hide();
    }

    let _ = app.emit("float-bar-status-changed", new_enabled);
    Ok(new_enabled)
}

/// 重新校准悬浮条位置（居中 / 靠右 / 靠左）
#[tauri::command]
pub async fn reposition_float_bar(app: AppHandle, position: Option<String>) -> Result<(), String> {
    let Some(window) = app.get_webview_window(FLOAT_BAR_LABEL) else {
        return Err("float_bar window not found".to_string());
    };

    let pos = position.unwrap_or_else(|| "top_center".to_string());
    align_float_bar_to_screen(&window, &pos);
    Ok(())
}

/// 从悬浮条唤起主窗口
#[tauri::command]
pub async fn open_main_window_from_float(app: AppHandle) -> Result<(), String> {
    crate::tray::show_main_window(&app);
    Ok(())
}


//! 桌面悬浮顶栏/侧栏管理模块
//!
//! 负责 float_bar 独立窗口的生命周期、位置计算、贴边隐藏与悬停展开控制。
//! 默认贴顶居中常驻，未激活时仅露出一小条微光胶囊，鼠标悬停时平滑伸展为单行用量条。

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};

use crate::settings;
use crate::store::AppState;

pub const FLOAT_BAR_LABEL: &str = "float_bar";
pub const FLOAT_BAR_WIDTH_EXPANDED: f64 = 890.0;
pub const FLOAT_BAR_WIDTH_COLLAPSED: f64 = 184.0;
pub const FLOAT_BAR_HEIGHT_EXPANDED: f64 = 60.0;
pub const FLOAT_BAR_HEIGHT_COLLAPSED: f64 = 12.0;

/// 缓存用户拖动后的中心 X 坐标，收缩与展开时围绕该中心点等距伸缩，绝不跳回屏幕正中央
static FLOAT_BAR_CENTER_X: Mutex<Option<f64>> = Mutex::new(None);

/// 将悬浮条对齐并根据展开状态调整窗口大小与屏幕位置
pub fn align_float_bar_to_screen(window: &WebviewWindow, position_mode: &str, expanded: bool) {
    let width = if expanded {
        FLOAT_BAR_WIDTH_EXPANDED
    } else {
        FLOAT_BAR_WIDTH_COLLAPSED
    };
    let height = if expanded {
        FLOAT_BAR_HEIGHT_EXPANDED
    } else {
        FLOAT_BAR_HEIGHT_COLLAPSED
    };

    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else { return; };

    let scale = monitor.scale_factor();
    let mon_pos = monitor.position();
    let mon_size = monitor.size();

    let mon_x = mon_pos.x as f64 / scale;
    let mon_y = mon_pos.y as f64 / scale;
    let mon_w = mon_size.width as f64 / scale;

    let mut saved_center = FLOAT_BAR_CENTER_X.lock().unwrap();

    // 检查窗口当前实际物理位置（用户可能刚拖拽放置）
    let current_center = if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
        let curr_x = pos.x as f64 / scale;
        let curr_w = size.width as f64 / scale;
        if curr_w > 50.0 && curr_x >= (mon_x - 300.0) && curr_x <= (mon_x + mon_w + 300.0) {
            Some(curr_x + curr_w / 2.0)
        } else {
            None
        }
    } else {
        None
    };

    let center_x = current_center
        .or(*saved_center)
        .unwrap_or_else(|| {
            match position_mode {
                "top_right" => mon_x + mon_w - width / 2.0 - 24.0,
                "top_left" => mon_x + width / 2.0 + 24.0,
                _ => mon_x + mon_w / 2.0, // default top_center
            }
        });

    *saved_center = Some(center_x);

    let target_x = (center_x - width / 2.0).clamp(mon_x, (mon_x + mon_w - width).max(mon_x));
    let target_y = mon_y;

    let _ = window.set_size(LogicalSize::new(width, height));
    let _ = window.set_position(LogicalPosition::new(target_x, target_y));
}

/// 启动时初始化悬浮条窗口（默认开启显示）
pub fn setup_float_bar(app: &AppHandle) {
    let Some(window) = app.get_webview_window(FLOAT_BAR_LABEL) else {
        log::warn!("找不到 label={FLOAT_BAR_LABEL} 的窗口，跳过悬浮顶栏初始化");
        return;
    };

    let _ = window.set_decorations(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_skip_taskbar(true);

    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(Some(saved)) = state.db.get_setting("float_bar_center_x") {
            if let Ok(x) = saved.parse::<f64>() {
                let mut lock = FLOAT_BAR_CENTER_X.lock().unwrap();
                *lock = Some(x);
            }
        }
    }

    let (enabled, pinned, position) = app
        .try_state::<AppState>()
        .and_then(|state| settings::get_settings_from_db(&state.db).ok())
        .map(|s| (s.float_bar_enabled, s.float_bar_pinned, s.float_bar_position))
        .unwrap_or((true, false, "top_center".to_string()));

    // 默认开机及启动时悬浮条就生效并精确定位大小
    align_float_bar_to_screen(&window, &position, pinned);

    if enabled {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        log::info!("桌面悬浮顶栏已初始化并默认显示");
    } else {
        let _ = window.hide();
        log::info!("桌面悬浮顶栏设置关闭，保持隐藏");
    }
}

/// 前端悬停/离开时动态调整窗口大小与位置
#[tauri::command]
pub async fn set_float_bar_expanded(
    app: AppHandle,
    expanded: bool,
    pinned: Option<bool>,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window(FLOAT_BAR_LABEL) else {
        return Err("float_bar window not found".to_string());
    };

    let (db_pinned, position) = app
        .try_state::<AppState>()
        .and_then(|state| settings::get_settings_from_db(&state.db).ok())
        .map(|s| (s.float_bar_pinned, s.float_bar_position))
        .unwrap_or((false, "top_center".to_string()));

    let is_pinned = pinned.unwrap_or(db_pinned);
    let is_expanded = is_pinned || expanded;
    align_float_bar_to_screen(&window, &position, is_expanded);
    let _ = window.set_always_on_top(true);

    // 持久化保存当前中心点 X 坐标
    let center_x_opt = *FLOAT_BAR_CENTER_X.lock().unwrap();
    if let (Some(center_x), Some(state)) = (center_x_opt, app.try_state::<AppState>()) {
        let db = state.db.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let _ = db.set_setting("float_bar_center_x", &center_x.to_string());
        });
    }

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
        align_float_bar_to_screen(&window, &current_settings.float_bar_position, current_settings.float_bar_pinned);
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

    {
        let mut saved = FLOAT_BAR_CENTER_X.lock().unwrap();
        *saved = None;
    }

    let pinned = app
        .try_state::<AppState>()
        .and_then(|state| settings::get_settings_from_db(&state.db).ok())
        .map(|s| s.float_bar_pinned)
        .unwrap_or(false);

    let pos = position.unwrap_or_else(|| "top_center".to_string());
    align_float_bar_to_screen(&window, &pos, pinned);
    Ok(())
}

/// 从悬浮条唤起主窗口
#[tauri::command]
pub async fn open_main_window_from_float(app: AppHandle) -> Result<(), String> {
    crate::tray::show_main_window(&app);
    Ok(())
}

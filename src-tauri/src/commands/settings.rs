use crate::settings::{get_settings_from_db, save_settings_to_db, AppSettings};
use crate::store::AppState;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let db = state.db.clone();
    let settings = tauri::async_runtime::spawn_blocking(move || {
        let s = get_settings_from_db(&db);
        if let Ok(ref current) = s {
            crate::reconcile_auto_launch(current);
        }
        s
    })
    .await
    .map_err(|error| format!("读取设置任务失败: {error}"))?
    .map_err(|error| error.to_string())?;

    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<bool, String> {
    let db = state.db.clone();
    let old_lang = tauri::async_runtime::spawn_blocking({
        let db = db.clone();
        move || get_settings_from_db(&db).map(|s| s.language).unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    let settings_clone = settings.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let res = save_settings_to_db(&db, &settings_clone);
        crate::reconcile_auto_launch(&settings_clone);
        res
    })
    .await
    .map_err(|error| format!("保存设置任务失败: {error}"))?
    .map_err(|e| e.to_string())?;

    if old_lang != settings.language {
        let lang_payload = serde_json::json!({ "language": settings.language });
        let _ = app.emit("app-language-changed", lang_payload.clone());
        let _ = app.emit_to("float_bar", "app-language-changed", lang_payload);

        if let Err(error) = crate::tray::create_tray(&app) {
            log::warn!("刷新系统托盘菜单失败: {error}");
        }
    }

    Ok(true)
}

#[tauri::command]
pub async fn get_pricing_model_source(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<String, String> {
    let db = state.db.clone();
    let key = format!("pricing_model_source_{app_type}");
    tauri::async_runtime::spawn_blocking(move || {
        match db.get_setting(&key) {
            Ok(Some(val)) => Ok(val),
            Ok(None) => Ok("response".to_string()),
            Err(e) => Err(e.to_string()),
        }
    })
    .await
    .map_err(|error| format!("读取定价模型来源任务失败: {error}"))?
}

#[tauri::command]
pub async fn set_pricing_model_source(
    state: State<'_, AppState>,
    app_type: String,
    value: String,
) -> Result<(), String> {
    let db = state.db.clone();
    let key = format!("pricing_model_source_{app_type}");
    tauri::async_runtime::spawn_blocking(move || {
        db.set_setting(&key, &value).map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| format!("设置定价模型来源任务失败: {error}"))?
}

/// 开关开机自启。`silent` 决定启动项里带不带 `--silent`。
///
/// 单独一个命令而不是并进 `save_settings`：注册表/登录项是系统状态，可能失败
/// （被组策略挡住、macOS 拒绝授权），要让前端拿到明确的错误去回滚开关。
#[tauri::command]
pub async fn set_auto_launch(enabled: bool, silent: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if enabled {
            crate::auto_launch::enable_auto_launch(silent).map_err(|e| e.to_string())?;
        } else {
            crate::auto_launch::disable_auto_launch().map_err(|e| e.to_string())?;
        }
        Ok(true)
    })
    .await
    .map_err(|error| format!("设置开机自启任务失败: {error}"))?
}

/// 读系统里实际的自启状态：设置存的是意图，这里问的是系统真有没有登记。
#[tauri::command]
pub async fn get_auto_launch_status() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::auto_launch::is_auto_launch_enabled().map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| format!("读取开机自启状态任务失败: {error}"))?
}

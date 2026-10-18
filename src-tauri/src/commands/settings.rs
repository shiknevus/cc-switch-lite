use crate::settings::{get_settings_from_db, save_settings_to_db, AppSettings};
use crate::store::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(get_settings_from_db(&state.db))
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<bool, String> {
    save_settings_to_db(&state.db, &settings).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn get_pricing_model_source(
    state: State<'_, AppState>,
    app_type: String,
) -> Result<String, String> {
    let key = format!("pricing_model_source_{app_type}");
    match state.db.get_setting(&key) {
        Ok(Some(val)) => Ok(val),
        Ok(None) => Ok("response".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn set_pricing_model_source(
    state: State<'_, AppState>,
    app_type: String,
    value: String,
) -> Result<(), String> {
    let key = format!("pricing_model_source_{app_type}");
    state.db.set_setting(&key, &value).map_err(|e| e.to_string())?;
    Ok(())
}


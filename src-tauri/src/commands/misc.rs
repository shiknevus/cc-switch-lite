use crate::error::AppError;

#[tauri::command]
pub fn is_silent_startup() -> bool {
    crate::is_silent_startup()
}

#[tauri::command]
pub fn open_dir(path: String) -> Result<(), AppError> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::Message(format!("Path does not exist: {path}")));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Message(format!("Unable to open Explorer: {e}")))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Message(format!("Unable to open Finder: {e}")))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Message(format!("Unable to open file manager: {e}")))?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_app_config_dir() -> Result<(), AppError> {
    open_dir(crate::config::get_app_config_dir().to_string_lossy().into_owned())
}

#[tauri::command]
pub fn set_window_theme(
    window: tauri::WebviewWindow,
    theme: String,
) -> Result<(), AppError> {
    let native_theme = match theme.as_str() {
        "dark" => Some(tauri_utils::Theme::Dark),
        "light" => Some(tauri_utils::Theme::Light),
        "system" => None,
        _ => return Err(AppError::InvalidInput(format!("Unknown theme: {theme}"))),
    };
    window
        .set_theme(native_theme)
        .map_err(|error| AppError::Message(format!("Failed to set window theme: {error}")))
}

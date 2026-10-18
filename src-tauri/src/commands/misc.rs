use crate::error::AppError;

#[tauri::command]
pub fn get_init_error() -> Option<serde_json::Value> {
    None
}

#[tauri::command]
pub fn open_dir(path: String) -> Result<(), AppError> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::Message(format!("路径不存在: {path}")));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Message(format!("无法打开资源管理器: {e}")))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Message(format!("无法打开 Finder: {e}")))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Message(format!("无法打开文件管理器: {e}")))?;
    }
    Ok(())
}

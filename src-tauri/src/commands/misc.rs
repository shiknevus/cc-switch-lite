use crate::error::AppError;

#[tauri::command]
pub fn get_init_error() -> Option<serde_json::Value> {
    None
}

/// 本次启动是否处于静默模式（开机自启 + 设置里开了静默启动）。
///
/// 前端渲染完成后本来会主动 `show()` 主窗口兜底；静默启动时不能再这么做，
/// 否则窗口会在页面前端加载完后自己弹出来，静默设置就失效了。
#[tauri::command]
pub fn is_silent_startup() -> bool {
    crate::is_silent_startup()
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

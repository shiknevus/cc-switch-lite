use crate::error::AppError;
use std::path::{Path, PathBuf};

fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or_else(|| path.to_path_buf())
}

fn is_within(path: &Path, root: &Path) -> bool {
    let p = strip_verbatim(path);
    let r = strip_verbatim(root);
    #[cfg(target_os = "windows")]
    {
        let p_str = p.to_string_lossy().to_lowercase();
        let r_str = r.to_string_lossy().to_lowercase();
        Path::new(&p_str).starts_with(Path::new(&r_str))
    }
    #[cfg(not(target_os = "windows"))]
    {
        p.starts_with(r)
    }
}

#[tauri::command]
pub fn is_silent_startup() -> bool {
    crate::is_silent_startup()
}

#[tauri::command]
pub async fn open_dir(path: String) -> Result<(), AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err(AppError::Message(format!("Path does not exist: {path}")));
        }
        let canonical = p.canonicalize().map_err(|error| AppError::io(p, error))?;
        let home = crate::config::get_home_dir();
        let home_canonical = home.canonicalize().unwrap_or(home);
        if !is_within(&canonical, &home_canonical) {
            return Err(AppError::InvalidInput(
                "只能打开用户主目录下的路径".to_string(),
            ));
        }
        let target = strip_verbatim(&canonical);
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(&target)
                .spawn()
                .map_err(|e| AppError::Message(format!("Unable to open Explorer: {e}")))?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&target)
                .spawn()
                .map_err(|e| AppError::Message(format!("Unable to open Finder: {e}")))?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&target)
                .spawn()
                .map_err(|e| AppError::Message(format!("Unable to open file manager: {e}")))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Message(format!("打开目录任务失败: {e}")))?
}

#[tauri::command]
pub async fn open_app_config_dir() -> Result<(), AppError> {
    open_dir(
        crate::config::get_app_config_dir()
            .to_string_lossy()
            .into_owned(),
    )
    .await
}

#[tauri::command]
pub fn set_window_theme(window: tauri::WebviewWindow, theme: String) -> Result<(), AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_within() {
        #[cfg(target_os = "windows")]
        {
            let root = Path::new(r"C:\Users\username");
            let inside_mixed_case =
                Path::new(r"\\?\c:\users\username\.cc-switch-lite\cc-switch.db");
            assert!(is_within(inside_mixed_case, root));

            let outside = Path::new(r"D:\other\path");
            assert!(!is_within(outside, root));
        }
        #[cfg(not(target_os = "windows"))]
        {
            let root = Path::new("/home/username");
            let inside = Path::new("/home/username/.cc-switch-lite/cc-switch.db");
            assert!(is_within(inside, root));

            let outside = Path::new("/etc/passwd");
            assert!(!is_within(outside, root));
        }
    }
}

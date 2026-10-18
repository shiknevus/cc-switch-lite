use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppError;

fn test_home_override() -> Option<PathBuf> {
    let raw = std::env::var_os("CC_SWITCH_TEST_HOME")?;
    let lossy = raw.to_string_lossy();
    let trimmed = lossy.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

pub fn get_home_dir() -> PathBuf {
    if let Some(home) = test_home_override() {
        return home;
    }

    dirs::home_dir().unwrap_or_else(|| {
        log::warn!("无法获取用户主目录，回退到当前目录");
        PathBuf::from(".")
    })
}

pub fn is_wsl_path(_path: &Path) -> bool {
    false
}

pub fn get_app_config_dir() -> PathBuf {
    let home = get_home_dir();
    let legacy_dir = home.join(".cc-switch");
    if legacy_dir.join("cc-switch.db").exists() {
        return legacy_dir;
    }
    let lite_dir = home.join(".cc-switch-lite");
    if !lite_dir.exists() {
        let _ = fs::create_dir_all(&lite_dir);
    }
    lite_dir
}

pub fn get_claude_config_dir() -> PathBuf {
    get_home_dir().join(".claude")
}

pub fn get_codex_config_dir() -> PathBuf {
    get_home_dir().join(".codex")
}

pub fn get_gemini_dir() -> PathBuf {
    let agy_dir = get_home_dir().join(".gemini").join("antigravity-cli");
    if agy_dir.exists() {
        return agy_dir;
    }
    let legacy_dir = get_home_dir().join(".gemini");
    if legacy_dir.join("settings.json").exists() || legacy_dir.join(".env").exists() {
        return legacy_dir;
    }
    agy_dir
}

pub fn get_grokbuild_config_dir() -> PathBuf {
    get_home_dir().join(".grok")
}

pub fn get_opencode_db_path() -> PathBuf {
    if let Ok(custom_path) = std::env::var("OPENCODE_DB") {
        if !custom_path.is_empty() {
            let path = PathBuf::from(&custom_path);
            if path.is_absolute() {
                return path;
            }
            return get_opencode_data_dir().join(path);
        }
    }
    get_opencode_data_dir().join("opencode.db")
}

fn get_opencode_data_dir() -> PathBuf {
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        if !xdg_data.is_empty() {
            return PathBuf::from(xdg_data).join("opencode");
        }
    }
    get_home_dir().join(".local").join("share").join("opencode")
}

pub fn get_pi_dir() -> PathBuf {
    get_home_dir().join(".pi")
}

pub fn get_mcode_data_dir() -> PathBuf {
    get_home_dir().join(".minimax")
}

pub fn atomic_write<P: AsRef<Path>>(path: P, content: &[u8]) -> Result<(), AppError> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;

    let mut temp_file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| AppError::io(parent, e))?;

    temp_file
        .write_all(content)
        .map_err(|e| AppError::io(temp_file.path(), e))?;
    temp_file
        .flush()
        .map_err(|e| AppError::io(temp_file.path(), e))?;

    temp_file
        .persist(path)
        .map_err(|e| AppError::io(path, e.error))?;

    Ok(())
}

pub fn write_text_file<P: AsRef<Path>>(path: P, content: &str) -> Result<(), AppError> {
    atomic_write(path, content.as_bytes())
}

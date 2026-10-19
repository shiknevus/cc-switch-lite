use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::database::{lock_conn, Database};
use crate::error::AppError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDisplaySettings {
    pub currency: Option<String>,
    pub exchange_rates: Option<HashMap<String, f64>>,
    pub sidebar_today_items: Option<Vec<String>>,
    pub request_log_columns: Option<HashMap<String, bool>>,
    pub trend_chart_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_refresh_interval")]
    pub usage_dashboard_refresh_interval_ms: u64,
    #[serde(default = "default_true")]
    pub session_auto_sync_enabled: bool,
    /// 点关闭按钮时隐藏到系统托盘继续在后台同步，而不是退出进程。
    #[serde(default = "default_true")]
    pub minimize_to_tray_on_close: bool,
    /// 随系统启动自动运行。真正的启停由 `auto_launch` 模块落到系统里，这里只存意图。
    #[serde(default)]
    pub launch_on_startup: bool,
    /// 开机自启拉起来时只驻留托盘，不显示主窗口。
    #[serde(default)]
    pub silent_startup: bool,
    /// 桌面悬浮顶栏是否启用
    #[serde(default = "default_true")]
    pub float_bar_enabled: bool,
    /// 桌面悬浮顶栏是否贴边自动隐藏
    #[serde(default = "default_true")]
    pub float_bar_auto_hide: bool,
    /// 桌面悬浮顶栏位置: "top_center" | "top_right" | "top_left"
    #[serde(default = "default_float_bar_position")]
    pub float_bar_position: String,
    /// 桌面悬浮顶栏是否钉住（固定展开不自动收起）
    #[serde(default)]
    pub float_bar_pinned: bool,
    pub usage_display: Option<UsageDisplaySettings>,
}

fn default_float_bar_position() -> String {
    "top_center".to_string()
}

fn default_language() -> String {
    "zh".to_string()
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_refresh_interval() -> u64 {
    60000
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: default_language(),
            theme: default_theme(),
            usage_dashboard_refresh_interval_ms: default_refresh_interval(),
            session_auto_sync_enabled: true,
            minimize_to_tray_on_close: true,
            launch_on_startup: false,
            silent_startup: false,
            float_bar_enabled: true,
            float_bar_auto_hide: true,
            float_bar_position: default_float_bar_position(),
            float_bar_pinned: false,
            usage_display: Some(UsageDisplaySettings {
                currency: Some("USD".to_string()),
                exchange_rates: None,
                sidebar_today_items: None,
                request_log_columns: None,
                trend_chart_type: Some("tokens".to_string()),
            }),
        }
    }
}

fn parse_stored_bool(value: &str, default: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" => false,
        _ => default,
    }
}

pub fn get_settings_from_db(db: &Database) -> Result<AppSettings, AppError> {
    let mut settings = AppSettings::default();
    let map = db.get_all_settings()?;

    if let Some(lang) = map.get("language") {
        settings.language = lang.clone();
    }
    if let Some(theme) = map.get("theme") {
        settings.theme = theme.clone();
    }
    if let Some(interval) = map.get("usage_dashboard_refresh_interval_ms") {
        if let Ok(val) = interval.parse::<u64>() {
            settings.usage_dashboard_refresh_interval_ms = val;
        }
    }
    if let Some(auto_sync) = map.get("session_auto_sync_enabled") {
        settings.session_auto_sync_enabled = parse_stored_bool(auto_sync, true);
    }
    if let Some(minimize) = map.get("minimize_to_tray_on_close") {
        settings.minimize_to_tray_on_close = parse_stored_bool(minimize, true);
    }
    if let Some(launch) = map.get("launch_on_startup") {
        settings.launch_on_startup = parse_stored_bool(launch, false);
    }
    if let Some(silent) = map.get("silent_startup") {
        settings.silent_startup = parse_stored_bool(silent, false);
    }
    if let Some(enabled) = map.get("float_bar_enabled") {
        settings.float_bar_enabled = parse_stored_bool(enabled, true);
    }
    if let Some(auto_hide) = map.get("float_bar_auto_hide") {
        settings.float_bar_auto_hide = parse_stored_bool(auto_hide, true);
    }
    if let Some(pos) = map.get("float_bar_position") {
        settings.float_bar_position = pos.clone();
    }
    if let Some(pinned) = map.get("float_bar_pinned") {
        settings.float_bar_pinned = parse_stored_bool(pinned, false);
    }
    if let Some(display_json) = map.get("usage_display") {
        if let Ok(display) = serde_json::from_str::<UsageDisplaySettings>(display_json) {
            settings.usage_display = Some(display);
        }
    }

    Ok(settings)
}

pub fn save_settings_to_db(db: &Database, settings: &AppSettings) -> Result<(), AppError> {
    let mut conn = lock_conn!(db.conn);
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params!["language", &settings.language],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params!["theme", &settings.theme],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "usage_dashboard_refresh_interval_ms",
            settings.usage_dashboard_refresh_interval_ms.to_string()
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "session_auto_sync_enabled",
            if settings.session_auto_sync_enabled {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "minimize_to_tray_on_close",
            if settings.minimize_to_tray_on_close {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "launch_on_startup",
            if settings.launch_on_startup {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "silent_startup",
            if settings.silent_startup {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "float_bar_enabled",
            if settings.float_bar_enabled {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "float_bar_auto_hide",
            if settings.float_bar_auto_hide {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params!["float_bar_position", &settings.float_bar_position],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "float_bar_pinned",
            if settings.float_bar_pinned {
                "true"
            } else {
                "false"
            }
        ],
    )?;
    if let Some(ref display) = settings.usage_display {
        let json = serde_json::to_string(display)
            .map_err(|error| AppError::JsonSerialize { source: error })?;
        tx.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params!["usage_display", json],
        )?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::database::Database;

    /// 没存过的旧库要按「关窗隐到托盘」处理：这是新加的默认行为，缺了它会退回直接退出进程。
    #[test]
    fn minimize_to_tray_defaults_on_for_existing_db() {
        let db = Database::memory().expect("内存库");
        assert!(
            get_settings_from_db(&db)
                .expect("读取设置")
                .minimize_to_tray_on_close
        );
    }

    /// 开关关掉后要能存下来、读回来，否则用户取消勾选后关窗仍然不退出。
    #[test]
    fn minimize_to_tray_round_trips_through_db() {
        let db = Database::memory().expect("内存库");

        let mut settings = AppSettings::default();
        settings.minimize_to_tray_on_close = false;
        save_settings_to_db(&db, &settings).expect("保存设置");
        assert!(
            !get_settings_from_db(&db)
                .expect("读取设置")
                .minimize_to_tray_on_close
        );

        settings.minimize_to_tray_on_close = true;
        save_settings_to_db(&db, &settings).expect("保存设置");
        assert!(
            get_settings_from_db(&db)
                .expect("读取设置")
                .minimize_to_tray_on_close
        );
    }

    /// 前端传 camelCase、库里存 "true"/"false" 字符串，两边都要认。
    #[test]
    fn minimize_to_tray_reads_camel_case_json_and_stored_string() {
        let db = Database::memory().expect("内存库");
        db.set_setting("minimize_to_tray_on_close", "false")
            .expect("写设置");
        assert!(
            !get_settings_from_db(&db)
                .expect("读取设置")
                .minimize_to_tray_on_close
        );

        let parsed: AppSettings =
            serde_json::from_str(r#"{"minimizeToTrayOnClose":false}"#).expect("解析设置");
        assert!(!parsed.minimize_to_tray_on_close);
    }

    /// 开机自启与静默启动默认都不开：默认帮着登记启动项是越权，用户没点过就不该有。
    #[test]
    fn auto_launch_defaults_off() {
        let db = Database::memory().expect("内存库");
        let settings = get_settings_from_db(&db).expect("读取设置");
        assert!(!settings.launch_on_startup);
        assert!(!settings.silent_startup);
    }

    /// 两个新开关都要能存下来读回来，否则重启后开机自启/静默会丢。
    #[test]
    fn auto_launch_round_trips_through_db() {
        let db = Database::memory().expect("内存库");

        let mut settings = AppSettings::default();
        settings.launch_on_startup = true;
        settings.silent_startup = true;
        save_settings_to_db(&db, &settings).expect("保存设置");

        let loaded = get_settings_from_db(&db).expect("读取设置");
        assert!(loaded.launch_on_startup);
        assert!(loaded.silent_startup);

        settings.launch_on_startup = false;
        settings.silent_startup = false;
        save_settings_to_db(&db, &settings).expect("保存设置");

        let loaded = get_settings_from_db(&db).expect("读取设置");
        assert!(!loaded.launch_on_startup);
        assert!(!loaded.silent_startup);
    }

    #[test]
    fn stored_bool_accepts_zero_and_ignores_unknown_text() {
        let db = Database::memory().expect("内存库");
        db.set_setting("launch_on_startup", "0").expect("写设置");
        db.set_setting("silent_startup", "maybe").expect("写设置");
        let settings = get_settings_from_db(&db).expect("读取设置");
        assert!(!settings.launch_on_startup);
        assert!(!settings.silent_startup);
    }
}

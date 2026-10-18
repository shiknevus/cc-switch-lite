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
    pub usage_display: Option<UsageDisplaySettings>,
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

pub fn get_settings_from_db(db: &Database) -> AppSettings {
    let mut settings = AppSettings::default();

    if let Ok(Some(lang)) = db.get_setting("language") {
        settings.language = lang;
    }
    if let Ok(Some(theme)) = db.get_setting("theme") {
        settings.theme = theme;
    }
    if let Ok(Some(interval)) = db.get_setting("usage_dashboard_refresh_interval_ms") {
        if let Ok(val) = interval.parse::<u64>() {
            settings.usage_dashboard_refresh_interval_ms = val;
        }
    }
    if let Ok(Some(auto_sync)) = db.get_setting("session_auto_sync_enabled") {
        settings.session_auto_sync_enabled = auto_sync != "false";
    }
    if let Ok(Some(minimize)) = db.get_setting("minimize_to_tray_on_close") {
        settings.minimize_to_tray_on_close = minimize != "false";
    }
    if let Ok(Some(launch)) = db.get_setting("launch_on_startup") {
        settings.launch_on_startup = launch != "false";
    }
    if let Ok(Some(silent)) = db.get_setting("silent_startup") {
        settings.silent_startup = silent != "false";
    }
    if let Ok(Some(display_json)) = db.get_setting("usage_display") {
        if let Ok(display) = serde_json::from_str::<UsageDisplaySettings>(&display_json) {
            settings.usage_display = Some(display);
        }
    }

    settings
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
            if settings.session_auto_sync_enabled { "true" } else { "false" }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "minimize_to_tray_on_close",
            if settings.minimize_to_tray_on_close { "true" } else { "false" }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "launch_on_startup",
            if settings.launch_on_startup { "true" } else { "false" }
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![
            "silent_startup",
            if settings.silent_startup { "true" } else { "false" }
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
mod tests {
    use super::*;
    use crate::database::Database;

    /// 没存过的旧库要按「关窗隐到托盘」处理：这是新加的默认行为，缺了它会退回直接退出进程。
    #[test]
    fn minimize_to_tray_defaults_on_for_existing_db() {
        let db = Database::memory().expect("内存库");
        assert!(get_settings_from_db(&db).minimize_to_tray_on_close);
    }

    /// 开关关掉后要能存下来、读回来，否则用户取消勾选后关窗仍然不退出。
    #[test]
    fn minimize_to_tray_round_trips_through_db() {
        let db = Database::memory().expect("内存库");

        let mut settings = AppSettings::default();
        settings.minimize_to_tray_on_close = false;
        save_settings_to_db(&db, &settings).expect("保存设置");
        assert!(!get_settings_from_db(&db).minimize_to_tray_on_close);

        settings.minimize_to_tray_on_close = true;
        save_settings_to_db(&db, &settings).expect("保存设置");
        assert!(get_settings_from_db(&db).minimize_to_tray_on_close);
    }

    /// 前端传 camelCase、库里存 "true"/"false" 字符串，两边都要认。
    #[test]
    fn minimize_to_tray_reads_camel_case_json_and_stored_string() {
        let db = Database::memory().expect("内存库");
        db.set_setting("minimize_to_tray_on_close", "false")
            .expect("写设置");
        assert!(!get_settings_from_db(&db).minimize_to_tray_on_close);

        let parsed: AppSettings =
            serde_json::from_str(r#"{"minimizeToTrayOnClose":false}"#).expect("解析设置");
        assert!(!parsed.minimize_to_tray_on_close);
    }

    /// 开机自启与静默启动默认都不开：默认帮着登记启动项是越权，用户没点过就不该有。
    #[test]
    fn auto_launch_defaults_off() {
        let db = Database::memory().expect("内存库");
        let settings = get_settings_from_db(&db);
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

        let loaded = get_settings_from_db(&db);
        assert!(loaded.launch_on_startup);
        assert!(loaded.silent_startup);

        settings.launch_on_startup = false;
        settings.silent_startup = false;
        save_settings_to_db(&db, &settings).expect("保存设置");

        let loaded = get_settings_from_db(&db);
        assert!(!loaded.launch_on_startup);
        assert!(!loaded.silent_startup);
    }
}

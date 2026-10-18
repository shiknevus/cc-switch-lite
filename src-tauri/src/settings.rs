use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::database::Database;
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
    if let Ok(Some(display_json)) = db.get_setting("usage_display") {
        if let Ok(display) = serde_json::from_str::<UsageDisplaySettings>(&display_json) {
            settings.usage_display = Some(display);
        }
    }

    settings
}

pub fn save_settings_to_db(db: &Database, settings: &AppSettings) -> Result<(), AppError> {
    db.set_setting("language", &settings.language)?;
    db.set_setting("theme", &settings.theme)?;
    db.set_setting(
        "usage_dashboard_refresh_interval_ms",
        &settings.usage_dashboard_refresh_interval_ms.to_string(),
    )?;
    db.set_setting(
        "session_auto_sync_enabled",
        if settings.session_auto_sync_enabled { "true" } else { "false" },
    )?;
    db.set_setting(
        "minimize_to_tray_on_close",
        if settings.minimize_to_tray_on_close { "true" } else { "false" },
    )?;
    if let Some(ref display) = settings.usage_display {
        if let Ok(json) = serde_json::to_string(display) {
            db.set_setting("usage_display", &json)?;
        }
    }
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
}

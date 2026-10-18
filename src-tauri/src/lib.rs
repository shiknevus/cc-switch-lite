pub mod claude_desktop_config;
pub mod codex_config;
pub mod codex_rollout_file;
pub mod commands;
pub mod config;
pub mod database;
pub mod error;
pub mod gemini_config;
pub mod mcode_config;
pub mod opencode_config;
pub mod pi_config;
pub mod proxy;
pub mod services;
pub mod session_manager;
pub mod settings;
pub mod store;
pub mod usage_events;

use std::sync::Arc;
use tauri::Manager;

use crate::database::Database;
use crate::store::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            usage_events::init(app.handle().clone());

            let db = Arc::new(Database::init().expect("数据库初始化失败"));
            app.manage(AppState::new(db.clone()));

            let sync_db = db.clone();
            tauri::async_runtime::spawn(async move {
                let _guard = services::session_usage::session_sync_mutex().lock().await;
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    services::session_usage::sync_all_unlocked(&sync_db)
                })
                .await;
            });

            let ticker_db = db.clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
                let mut last_sync = std::time::Instant::now();
                loop {
                    interval.tick().await;
                    let settings = settings::get_settings_from_db(&ticker_db);
                    if settings.session_auto_sync_enabled {
                        let interval_ms = settings.usage_dashboard_refresh_interval_ms.max(1000);
                        if last_sync.elapsed().as_millis() as u64 >= interval_ms {
                            last_sync = std::time::Instant::now();
                            let scan_db = ticker_db.clone();
                            if let Ok(_guard) = services::session_usage::session_sync_mutex().try_lock() {
                                let _ = tauri::async_runtime::spawn_blocking(move || {
                                    services::session_usage::sync_all_unlocked(&scan_db)
                                })
                                .await;
                            }
                        }
                    }
                }
            });

            if let Some(main_window) = app.get_webview_window("main") {
                // Enforce the borderless window before first paint; window-state restoration
                // can otherwise bring back the native light title bar.
                let _ = main_window.set_decorations(false);
                let _ = main_window.show();
                let _ = main_window.set_focus();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::usage::get_usage_summary,
            commands::usage::get_session_usage_summary,
            commands::usage::get_usage_summary_by_app,
            commands::usage::get_usage_trends,
            commands::usage::get_usage_first_date,
            commands::usage::get_provider_stats,
            commands::usage::get_model_stats,
            commands::usage::get_request_logs,
            commands::usage::get_request_detail,
            commands::usage::get_model_pricing,
            commands::usage::update_model_pricing,
            commands::usage::update_model_pricing_batch,
            commands::usage::delete_model_pricing,
            commands::usage::get_models_dev_sync_config,
            commands::usage::save_models_dev_sync_config,
            commands::usage::record_models_dev_sync_result,
            commands::usage::check_provider_limits,
            commands::usage::sync_session_usage,
            commands::usage::get_session_usage_last_sync,
            commands::usage::rebuild_codex_usage,
            commands::usage::rebuild_gemini_usage,
            commands::usage::rebuild_agy_usage,
            commands::usage::rebuild_dsh_usage,
            commands::usage::get_usage_data_sources,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_pricing_model_source,
            commands::settings::set_pricing_model_source,
            commands::misc::get_init_error,
            commands::misc::open_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running cc-switch-lite application");
}

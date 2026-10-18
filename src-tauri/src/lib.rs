pub mod auto_launch;
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
pub mod tray;
pub mod usage_events;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Manager;

use crate::database::Database;
use crate::store::AppState;

/// 本次启动是否「静默」：不显示主窗口，只驻留托盘。
///
/// 在 setup 里按设置定下来，之后被两处显示窗口的路径读取（Windows 的
/// on_page_load、其它平台的 setup）。用静态量而不是闭包捕获，是因为
/// on_page_load 的回调在 builder 阶段就要注册，那时数据库还没打开。
static SILENT_STARTUP: AtomicBool = AtomicBool::new(false);

/// 静默启动时不弹主窗口，只留托盘。
pub fn is_silent_startup() -> bool {
    SILENT_STARTUP.load(Ordering::Relaxed)
}

/// 启动时按设置对账开机自启项。
///
/// 只在「系统里已经登记着自启」时才重写：重写会把启动项指向当前的可执行文件
/// 路径并带上正确的 `--silent`，修掉升级换目录、或改了静默开关但启动项没跟上
/// 的情况。反过来，设置里写着要自启、系统里却没有时**不**重新登记——那多半是
/// 用户自己在任务管理器里禁用了，替他加回来只会打架。
/// 剪枝和会话扫描共用同一把锁，避免一边写入历史行、一边把 30 天前的明细删掉。
async fn prune_usage_logs(db: Arc<Database>) {
    let _guard = services::session_usage::session_sync_mutex().lock().await;
    let _ = tauri::async_runtime::spawn_blocking(move || match db.rollup_and_prune(30) {
        Ok(pruned) if pruned > 0 => log::info!("历史用量日志已聚合剪枝: {pruned} 条"),
        Ok(_) => {}
        Err(error) => log::warn!("历史用量日志聚合与清理失败: {error}"),
    })
    .await;
}

fn reconcile_auto_launch(settings: &settings::AppSettings) {
    if !settings.launch_on_startup {
        return;
    }
    match auto_launch::is_auto_launch_enabled() {
        Ok(true) => {
            if let Err(e) = auto_launch::enable_auto_launch(settings.silent_startup) {
                log::warn!("刷新开机自启项失败: {e}");
            }
        }
        Ok(false) => log::info!("设置要求开机自启，但系统里没有该启动项，交由用户在设置里重新开启"),
        Err(e) => log::warn!("读取开机自启状态失败: {e}"),
    }
}

pub fn run() {
    let startup_page_handled = std::sync::atomic::AtomicBool::new(false);
    let mut builder = tauri::Builder::default()
        // 单实例必须排在其它插件之前：第二个实例要在其余 setup 跑起来之前就被拦下
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("检测到第二个实例，唤回已有窗口");
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // 拦截关闭：按设置决定隐到托盘后台常驻，还是直接退出进程
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let minimize_to_tray = window
                    .try_state::<AppState>()
                    .map(|state| match settings::get_settings_from_db(&state.db) {
                        Ok(settings) => settings.minimize_to_tray_on_close,
                        Err(error) => {
                            log::warn!("读取关窗设置失败，保持在托盘: {error}");
                            true
                        }
                    })
                    .unwrap_or(true);

                if minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                    #[cfg(target_os = "windows")]
                    {
                        // 连同任务栏图标一起收掉，否则窗口隐藏了任务栏还留一个空图标
                        let _ = window.set_skip_taskbar(true);
                    }
                    log::info!("关闭请求已处理：隐藏到系统托盘，进程继续在后台运行");
                } else {
                    api.prevent_close();
                    log::info!("关闭请求已处理：退出应用");
                    window.app_handle().exit(0);
                }
            }
        });

    #[cfg(target_os = "windows")]
    {
        builder = builder.on_page_load(move |webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
                && payload.url().scheme() != "about"
                && !startup_page_handled.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                let window = webview.window();
                let _ = window.set_decorations(false);
                // 静默启动：窗口始终不露出来，只留托盘
                if is_silent_startup() {
                    let _ = window.hide();
                    let _ = window.set_skip_taskbar(true);
                    log::info!("静默启动：主页面已加载，主窗口保持隐藏");
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                    log::info!("主页面加载完成，主窗口已显示");
                }
            }
        });
    }

    builder
        .setup(|app| {
            // 日志先落地：托盘/关窗这些后台行为的诊断全靠它，之前没注册插件，
            // 所有 log::info! 都写进了黑洞。
            {
                use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

                let log_dir = config::get_app_config_dir().join("logs");
                if let Err(e) = std::fs::create_dir_all(&log_dir) {
                    eprintln!("创建日志目录失败: {e}");
                }
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .targets([
                            Target::new(TargetKind::Stdout),
                            Target::new(TargetKind::Folder {
                                path: log_dir,
                                file_name: Some("cc-switch-lite".into()),
                            }),
                        ])
                        .rotation_strategy(RotationStrategy::KeepSome(4))
                        .max_file_size(20 * 1024 * 1024)
                        .timezone_strategy(TimezoneStrategy::UseLocal)
                        .build(),
                )?;
                log::info!("=== CC Switch Lite v{} started ===", env!("CARGO_PKG_VERSION"));
            }

            usage_events::init(app.handle().clone());

            let db = match Database::init() {
                Ok(db) => Arc::new(db),
                Err(error) => {
                    let message = format!("数据库初始化失败: {error}");
                    log::error!("{message}");
                    use tauri_plugin_dialog::DialogExt;
                    let _ = app
                        .dialog()
                        .message(&message)
                        .title("CC Switch Lite")
                        .blocking_show();
                    return Err(message.into());
                }
            };
            app.manage(AppState::new(db.clone()));

            // 静默启动要同时满足两点：设置里开着静默，且本次确实由开机自启拉起来
            // （命令行带 --silent）。这样用户手动双击图标时总能看见窗口，不会因为
            // 开着静默就再也打不开界面。
            let settings = match settings::get_settings_from_db(&db) {
                Ok(settings) => settings,
                Err(error) => {
                    log::error!("读取设置失败，本次使用默认值且不会写回: {error}");
                    settings::AppSettings::default()
                }
            };
            let silent = settings.silent_startup && auto_launch::started_with_silent_arg();
            SILENT_STARTUP.store(silent, Ordering::Relaxed);

            // 开机自启按设置对账：升级后安装路径可能变了，这里把启动项改写到当前
            // 路径；用户已经手动取消了自启（设置里仍为 true 但系统里没了）时不强推
            // 回来，避免和用户在任务管理器里的选择打架。
            reconcile_auto_launch(&settings);

            // 托盘要在 AppState 之后建：菜单文案按数据库里的界面语言取
            if let Err(e) = tray::create_tray(app.handle()) {
                log::error!("创建系统托盘失败，关窗后将无法唤回窗口: {e}");
            }

            #[cfg(not(target_os = "windows"))]
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = main_window.set_decorations(false);
                if silent {
                    let _ = main_window.hide();
                    log::info!("静默启动：主窗口保持隐藏");
                } else {
                    let _ = main_window.show();
                    let _ = main_window.set_focus();
                }
            }

            let sync_db = db.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                let auto_sync = settings::get_settings_from_db(&sync_db)
                    .map(|settings| settings.session_auto_sync_enabled)
                    .unwrap_or(false);
                if !auto_sync {
                    log::info!("startup session scan skipped because automatic sync is disabled");
                    return;
                }
                let _guard = services::session_usage::session_sync_mutex().lock().await;
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    services::session_usage::sync_all_unlocked(&sync_db)
                })
                .await;
            });

            // 启动后异步执行一次历史用量日志聚合与清理（30天前明细汇总进 rollups 并删除，抑制数据库无限膨胀）
            let prune_db = db.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                prune_usage_logs(prune_db).await;
            });

            let ticker_db = db.clone();
            tauri::async_runtime::spawn(async move {
                // 错过的 tick 直接丢掉。一次扫描要是超过间隔，补发会连着打多次设置查询。
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(5000));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut last_sync = std::time::Instant::now();
                let mut last_prune = std::time::Instant::now();
                loop {
                    interval.tick().await;

                    // 每日在后台执行一次历史用量聚合剪枝，并和会话扫描共用同一把锁。
                    if last_prune.elapsed().as_secs() >= 86400 {
                        last_prune = std::time::Instant::now();
                        prune_usage_logs(ticker_db.clone()).await;
                    }

                    let settings = match settings::get_settings_from_db(&ticker_db) {
                        Ok(settings) => settings,
                        Err(error) => {
                            log::warn!("读取同步设置失败，跳过本轮: {error}");
                            continue;
                        }
                    };
                    if settings.session_auto_sync_enabled {
                        let interval_ms = settings.usage_dashboard_refresh_interval_ms.max(5000);
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
            commands::settings::set_auto_launch,
            commands::settings::get_auto_launch_status,
            commands::misc::is_silent_startup,
            commands::misc::open_dir,
            commands::misc::open_app_config_dir,
            commands::misc::set_window_theme,
        ])
        .run(tauri::generate_context!())
        .expect("error while running cc-switch-lite application");
}

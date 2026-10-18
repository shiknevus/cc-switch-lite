use crate::error::AppError;
use crate::services::model_pricing::{ModelPricingInfo, ModelsDevSyncConfig, ModelsDevSyncState};
use crate::services::usage_stats::*;
use crate::store::AppState;
use tauri::State;

async fn run_db_query<T, F>(state: &State<'_, AppState>, query: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(&crate::database::Database) -> Result<T, AppError> + Send + 'static,
{
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || query(&db))
        .await
        .map_err(|error| AppError::Message(format!("用量查询任务失败: {error}")))?
}

#[tauri::command]
pub async fn get_usage_summary(
    state: State<'_, AppState>,
    start_date: Option<i64>,
    end_date: Option<i64>,
    app_type: Option<String>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<UsageSummary, AppError> {
    run_db_query(&state, move |db| {
        db.get_usage_summary(
            start_date,
            end_date,
            app_type.as_deref(),
            provider_name.as_deref(),
            model.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_session_usage_summary(
    state: State<'_, AppState>,
    app_type: String,
    session_id: String,
) -> Result<UsageSummary, AppError> {
    run_db_query(&state, move |db| {
        db.get_session_usage_summary(&app_type, &session_id)
    })
    .await
}

#[tauri::command]
pub async fn get_usage_summary_by_app(
    state: State<'_, AppState>,
    start_date: Option<i64>,
    end_date: Option<i64>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<Vec<UsageSummaryByApp>, AppError> {
    run_db_query(&state, move |db| {
        db.get_usage_summary_by_app(
            start_date,
            end_date,
            provider_name.as_deref(),
            model.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_usage_trends(
    state: State<'_, AppState>,
    start_date: Option<i64>,
    end_date: Option<i64>,
    app_type: Option<String>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<Vec<DailyStats>, AppError> {
    run_db_query(&state, move |db| {
        db.get_daily_trends(
            start_date,
            end_date,
            app_type.as_deref(),
            provider_name.as_deref(),
            model.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_usage_first_date(
    state: State<'_, AppState>,
    app_type: Option<String>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<Option<String>, AppError> {
    run_db_query(&state, move |db| {
        db.get_first_usage_date(
            app_type.as_deref(),
            provider_name.as_deref(),
            model.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_provider_stats(
    state: State<'_, AppState>,
    start_date: Option<i64>,
    end_date: Option<i64>,
    app_type: Option<String>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<Vec<ProviderStats>, AppError> {
    run_db_query(&state, move |db| {
        db.get_provider_stats(
            start_date,
            end_date,
            app_type.as_deref(),
            provider_name.as_deref(),
            model.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_model_stats(
    state: State<'_, AppState>,
    start_date: Option<i64>,
    end_date: Option<i64>,
    app_type: Option<String>,
    provider_name: Option<String>,
    model: Option<String>,
) -> Result<Vec<ModelStats>, AppError> {
    run_db_query(&state, move |db| {
        db.get_model_stats(
            start_date,
            end_date,
            app_type.as_deref(),
            provider_name.as_deref(),
            model.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_request_logs(
    state: State<'_, AppState>,
    filters: LogFilters,
    page: u32,
    page_size: u32,
) -> Result<PaginatedLogs, AppError> {
    run_db_query(&state, move |db| {
        db.get_request_logs(&filters, page, page_size)
    })
    .await
}

#[tauri::command]
pub async fn get_request_detail(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<Option<RequestLogDetail>, AppError> {
    run_db_query(&state, move |db| db.get_request_detail(&request_id)).await
}

#[tauri::command]
pub fn get_model_pricing(state: State<'_, AppState>) -> Result<Vec<ModelPricingInfo>, AppError> {
    log::info!("获取模型定价列表");
    state.db.ensure_model_pricing_seeded()?;
    crate::services::model_pricing::sync_local_model_pricing(&state.db)?;

    let db = state.db.clone();
    let conn = crate::database::lock_conn!(db.conn);

    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='model_pricing'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )
        .unwrap_or(false);

    if !table_exists {
        log::error!("model_pricing 表不存在");
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT model_id, display_name, input_cost_per_million, output_cost_per_million,
                cache_read_cost_per_million, cache_creation_cost_per_million
         FROM model_pricing
         ORDER BY display_name",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(ModelPricingInfo {
            model_id: row.get(0)?,
            display_name: row.get(1)?,
            input_cost_per_million: row.get(2)?,
            output_cost_per_million: row.get(3)?,
            cache_read_cost_per_million: row.get(4)?,
            cache_creation_cost_per_million: row.get(5)?,
        })
    })?;

    let mut pricing = Vec::new();
    for row in rows {
        pricing.push(row?);
    }

    log::info!("成功获取 {} 条模型定价数据", pricing.len());
    Ok(pricing)
}

#[tauri::command]
pub fn update_model_pricing(
    state: State<'_, AppState>,
    model_id: String,
    display_name: String,
    input_cost: String,
    output_cost: String,
    cache_read_cost: String,
    cache_creation_cost: String,
) -> Result<(), AppError> {
    crate::services::model_pricing::update_model_pricing(
        &state.db,
        ModelPricingInfo {
            model_id,
            display_name,
            input_cost_per_million: input_cost,
            output_cost_per_million: output_cost,
            cache_read_cost_per_million: cache_read_cost,
            cache_creation_cost_per_million: cache_creation_cost,
        },
    )?;
    Ok(())
}

#[tauri::command]
pub fn update_model_pricing_batch(
    state: State<'_, AppState>,
    entries: Vec<ModelPricingInfo>,
) -> Result<usize, AppError> {
    crate::services::model_pricing::update_model_pricing_batch(&state.db, entries)
}

#[tauri::command]
pub fn get_models_dev_sync_config(
    state: State<'_, AppState>,
) -> Result<ModelsDevSyncState, AppError> {
    crate::services::model_pricing::get_models_dev_sync_state(&state.db)
}

#[tauri::command]
pub fn save_models_dev_sync_config(
    state: State<'_, AppState>,
    config: ModelsDevSyncConfig,
) -> Result<(), AppError> {
    crate::services::model_pricing::save_models_dev_sync_config(&state.db, config)
}

#[tauri::command]
pub fn record_models_dev_sync_result(
    state: State<'_, AppState>,
    synced_at: Option<i64>,
    error: Option<String>,
) -> Result<(), AppError> {
    crate::services::model_pricing::record_models_dev_sync_result(&state.db, synced_at, error)
}

#[tauri::command]
pub fn check_provider_limits(
    state: State<'_, AppState>,
    provider_id: String,
    app_type: String,
) -> Result<crate::services::usage_stats::ProviderLimitStatus, AppError> {
    state.db.check_provider_limits(&provider_id, &app_type)
}

#[tauri::command]
pub fn delete_model_pricing(state: State<'_, AppState>, model_id: String) -> Result<(), AppError> {
    crate::services::model_pricing::delete_model_pricing(&state.db, &model_id)?;
    log::info!("已删除模型定价: {model_id}");
    Ok(())
}

#[tauri::command]
pub async fn sync_session_usage(
    state: State<'_, AppState>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    let db = state.db.clone();
    let _guard = crate::services::session_usage::session_sync_mutex()
        .lock()
        .await;
    // 防御重复重扫：如果距离最近一次完整扫描完成还不足 3 秒（如后端定时任务刚刚扫完），
    // 直接返回空结果，避免前后端定时器在同一刷新周期内连续执行两次全量目录扫描。
    if let Some(last_sync) = crate::services::session_usage::last_sync_completed_at() {
        let elapsed = chrono::Utc::now().timestamp_millis() - last_sync;
        if elapsed >= 0 && elapsed < 3000 {
            log::debug!("跳过重复会话同步：距上次同步完成仅 {elapsed}ms");
            return Ok(crate::services::session_usage::SessionSyncResult::default());
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::session_usage::sync_all_unlocked(&db)
    })
    .await
    .map_err(|error| AppError::Message(format!("会话用量同步任务失败: {error}")))
}

#[tauri::command]
pub fn get_session_usage_last_sync() -> Option<i64> {
    crate::services::session_usage::last_sync_completed_at()
}

fn restore_rebuild_on_error(
    db: &crate::database::Database,
    backup: Option<std::path::PathBuf>,
    result: Result<crate::services::session_usage::SessionSyncResult, AppError>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            if let Some(path) = backup {
                if let Err(restore_error) = db.restore_database_file(&path) {
                    return Err(AppError::Message(format!(
                        "{error}; automatic restore from {} also failed: {restore_error}",
                        path.display()
                    )));
                }
            }
            Err(error)
        }
    }
}

fn finish_codex_rebuild(
    result: Result<crate::services::session_usage::SessionSyncResult, AppError>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    crate::usage_events::notify_log_recorded();
    result
}

#[tauri::command]
pub async fn rebuild_codex_usage(
    state: State<'_, AppState>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    let db = state.db.clone();
    let _guard = crate::services::session_usage::session_sync_mutex()
        .lock()
        .await;
    tauri::async_runtime::spawn_blocking(move || {
        let backup = db.backup_database_file()?;
        db.reset_codex_usage()?;
        let result = crate::services::session_usage_codex::sync_codex_usage(&db);
        finish_codex_rebuild(restore_rebuild_on_error(&db, backup, result))
    })
    .await
    .map_err(|error| AppError::Message(format!("Codex 用量重建任务失败: {error}")))?
}

fn finish_gemini_rebuild(
    result: Result<crate::services::session_usage::SessionSyncResult, AppError>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    crate::usage_events::notify_log_recorded();
    result
}

#[tauri::command]
pub async fn rebuild_gemini_usage(
    state: State<'_, AppState>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    let db = state.db.clone();
    let _guard = crate::services::session_usage::session_sync_mutex()
        .lock()
        .await;
    tauri::async_runtime::spawn_blocking(move || {
        let backup = db.backup_database_file()?;
        db.reset_gemini_usage()?;
        let result = crate::services::session_usage_gemini::sync_gemini_usage(&db);
        finish_gemini_rebuild(restore_rebuild_on_error(&db, backup, result))
    })
    .await
    .map_err(|error| AppError::Message(format!("Antigravity (agy) 用量重建任务失败: {error}")))?
}

#[tauri::command]
pub async fn rebuild_agy_usage(
    state: State<'_, AppState>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    rebuild_gemini_usage(state).await
}

fn finish_dsh_rebuild(
    result: Result<crate::services::session_usage::SessionSyncResult, AppError>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    crate::usage_events::notify_log_recorded();
    result
}

#[tauri::command]
pub async fn rebuild_dsh_usage(
    state: State<'_, AppState>,
) -> Result<crate::services::session_usage::SessionSyncResult, AppError> {
    let db = state.db.clone();
    let _guard = crate::services::session_usage::session_sync_mutex()
        .lock()
        .await;
    tauri::async_runtime::spawn_blocking(move || {
        let backup = db.backup_database_file()?;
        db.reset_dsh_usage()?;
        let result = crate::services::session_usage_dsh::sync_dsh_usage(&db);
        finish_dsh_rebuild(restore_rebuild_on_error(&db, backup, result))
    })
    .await
    .map_err(|error| AppError::Message(format!("Dsh 用量重建任务失败: {error}")))?
}

#[tauri::command]
pub fn get_usage_data_sources(
    state: State<'_, AppState>,
) -> Result<Vec<crate::services::session_usage::DataSourceSummary>, AppError> {
    crate::services::session_usage::get_data_source_breakdown(&state.db)
}


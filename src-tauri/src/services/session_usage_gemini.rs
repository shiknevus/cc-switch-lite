//! Gemini CLI 会话日志使用追踪
//!
//! 从 ~/.gemini/tmp/<project_hash>/chats/session-*.json(l) 中提取精确 token 使用数据。
//!
//! ## 数据流
//! ```text
//! ~/.gemini/tmp/*/chats/session-*.json(l) → 全量解析 → 费用计算 → proxy_request_logs 表
//! ```
//!
//! ## 与 Claude/Codex 解析器的差异
//! - 旧版 JSON 是单个对象；新版 JSONL 按记录回放还原成同样的 messages 数组
//!   （见 [`crate::session_manager::providers::gemini::parse_session_document`]）
//! - 无需 delta 计算：tokens 字段是 per-message 独立值
//! - 无需状态恢复：不依赖前一条消息的累计值
//! - 天然去重：每条消息有唯一 id 字段

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::gemini_config::get_gemini_dir;
use crate::proxy::usage::calculator::{CostCalculator, ModelPricing};
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use crate::session_manager::providers::gemini::{is_session_file, parse_session_document};
use rust_decimal::Decimal;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_GEMINI_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_GEMINI_LINE_BYTES: usize = 8 * 1024 * 1024;

/// 从 Gemini message 中提取的 token 数据
#[derive(Debug)]
struct GeminiTokens {
    input: u32,
    output: u32,
    cached: u32,
    thoughts: u32,
}

/// 同步 Gemini 使用数据（从 JSON 会话日志）
pub fn sync_gemini_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let gemini_dir = get_gemini_dir();

    let files = collect_gemini_session_files(&gemini_dir);

    let mut result = SessionSyncResult {
        imported: 0,
        skipped: 0,
        files_scanned: files.len() as u32,
        suspected_duplicates: 0,
        deferred_files: 0,
        errors: vec![],
    };

    if files.is_empty() {
        return Ok(result);
    }

    let cursors = crate::services::session_usage::load_sync_cursors(db)?;

    for file_path in &files {
        let last_modified = cursors
            .get(file_path.to_string_lossy().as_ref())
            .map_or(0, |c| c.last_modified);
        match sync_single_gemini_file(db, file_path, last_modified) {
            Ok((imported, skipped)) => {
                result.imported += imported;
                result.skipped += skipped;
            }
            Err(e) => {
                let msg = format!("Gemini 会话文件解析失败 {}: {e}", file_path.display());
                log::warn!("[GEMINI-SYNC] {msg}");
                result.errors.push(msg);
            }
        }
    }

    if result.imported > 0 {
        log::info!(
            "[GEMINI-SYNC] 同步完成: 导入 {} 条, 跳过 {} 条, 扫描 {} 个文件",
            result.imported,
            result.skipped,
            result.files_scanned
        );
    }

    Ok(result)
}

/// 收集所有 Gemini / agy (Antigravity CLI) 会话日志文件
fn collect_gemini_session_files(gemini_dir: &Path) -> Vec<PathBuf> {
    if !gemini_dir.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. 传统 Gemini CLI 会话 JSON(L) 文件: tmp/<project_hash>/chats/session-*.json(l)
    // antigravity-cli 和经典 CLI 可能同时存在。选中前者时，经典会话仍在
    // 上一级 ~/.gemini/tmp，不能只扫当前根目录。
    let mut tmp_dirs = vec![gemini_dir.join("tmp")];
    if gemini_dir.file_name().and_then(|name| name.to_str()) == Some("antigravity-cli") {
        if let Some(parent) = gemini_dir.parent() {
            tmp_dirs.push(parent.join("tmp"));
        }
    }
    for tmp_dir in tmp_dirs {
    if tmp_dir.is_dir() {
        if let Ok(project_dirs) = fs::read_dir(&tmp_dir) {
            for entry in project_dirs.flatten() {
                let chats_dir = entry.path().join("chats");
                if !chats_dir.is_dir() {
                    continue;
                }

                let chat_files = match fs::read_dir(&chats_dir) {
                    Ok(entries) => entries,
                    Err(_) => continue,
                };

                for file_entry in chat_files.flatten() {
                    let path = file_entry.path();
                    let is_session = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("session-"))
                        && is_session_file(&path);
                    if is_session && seen_paths.insert(path.clone()) {
                        files.push(path);
                    }
                }
            }
        }
    }
    }

    // 2. Antigravity / agy 会话日志: brain/<conversation_id>/.system_generated/logs/transcript.jsonl
    let home = crate::config::get_home_dir();
    let candidate_brain_dirs = [
        gemini_dir.join("brain"),
        gemini_dir.join("antigravity").join("brain"),
        gemini_dir.join("antigravity-cli").join("brain"),
        home.join(".gemini").join("antigravity").join("brain"),
        home.join(".gemini").join("antigravity-cli").join("brain"),
        home.join(".gemini").join("brain"),
    ];

    let mut seen_brain_dirs = std::collections::HashSet::new();
    for brain_dir in candidate_brain_dirs {
        if !brain_dir.is_dir() || !seen_brain_dirs.insert(brain_dir.clone()) {
            continue;
        }

        let entries = match fs::read_dir(&brain_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for conv_entry in entries.flatten() {
            let transcript = conv_entry
                .path()
                .join(".system_generated")
                .join("logs")
                .join("transcript.jsonl");
            if transcript.is_file() && seen_paths.insert(transcript.clone()) {
                files.push(transcript);
            }
        }
    }

    files
}

/// 规范化 agy 中的模型名称
pub(crate) fn normalize_agy_model_name(raw: &str) -> String {
    let base = if let Some(idx) = raw.find('(') {
        &raw[..idx]
    } else {
        raw
    };
    let trimmed = base.trim();
    let normalized = trimmed.to_lowercase().replace([' ', '_'], "-");
    if normalized.is_empty() {
        "gemini-3.8-flash".to_string()
    } else {
        normalized
    }
}

/// 从消息内容中尝试提取 Model Selection 变更
pub(crate) fn extract_model_from_content(content: &str) -> Option<String> {
    if let Some(pos) = content.find("Model Selection` from ") {
        let after = &content[pos + "Model Selection` from ".len()..];
        if let Some(to_pos) = after.find(" to ") {
            let model_part = &after[to_pos + 4..];
            let mut end = model_part.len();
            for (idx, ch) in model_part.char_indices() {
                if ch == '`' || ch == '\n' || ch == '<' {
                    end = idx;
                    break;
                }
                if ch == '.' {
                    let next = model_part[idx + 1..].chars().next();
                    if next.is_none()
                        || next == Some(' ')
                        || next == Some('\n')
                        || next == Some('\r')
                    {
                        end = idx;
                        break;
                    }
                }
            }
            let raw_model = model_part[..end].trim();
            if !raw_model.is_empty() {
                return Some(normalize_agy_model_name(raw_model));
            }
        }
    }
    None
}

/// 同步单个 agy transcript.jsonl 会话文件
fn sync_single_agy_transcript_file(
    db: &Database,
    file_path: &Path,
    last_modified: i64,
) -> Result<(u32, u32), AppError> {
    let file_path_str = file_path.to_string_lossy().to_string();

    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("无法读取文件元数据: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    if metadata.len() > MAX_GEMINI_FILE_BYTES {
        return Err(AppError::Config(format!("Gemini session file exceeds {} bytes", MAX_GEMINI_FILE_BYTES)));
    }

    if file_modified <= last_modified {
        return Ok((0, 0));
    }

    // 从路径提取 conversation_id: .../brain/<conversation-id>/.system_generated/logs/transcript.jsonl
    let session_id = file_path
        .parent() // logs
        .and_then(|p| p.parent()) // .system_generated
        .and_then(|p| p.parent()) // <conversation-id>
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    let file = fs::File::open(file_path)
        .map_err(|e| AppError::Config(format!("无法打开文件: {e}")))?;
    let reader = std::io::BufReader::new(file);

    let mut imported: u32 = 0;
    let mut skipped: u32 = 0;
    let mut step_count: i64 = 0;
    let mut current_model = "gemini-3.8-flash".to_string();
    let mut last_boundary_ms: Option<i64> = None;

    use std::io::BufRead;
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(error) => return Err(AppError::Config(format!("Gemini session read failed: {error}"))),
        };
        if line.len() > MAX_GEMINI_LINE_BYTES {
            return Err(AppError::Config("Gemini session line exceeds the safety limit".to_string()));
        }
        if line.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(content) = value.get("content").and_then(|c| c.as_str()) {
            if let Some(m) = extract_model_from_content(content) {
                current_model = m;
            }
        }

        let timestamp_str = value.get("created_at").and_then(|v| v.as_str());
        let current_ms = timestamp_str.and_then(crate::services::session_usage::parse_timestamp_millis);

        let input = value.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let output = value.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let cached = value.get("cache_read_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        if input == 0 && output == 0 && cached == 0 {
            if let Some(ms) = current_ms {
                last_boundary_ms = Some(ms);
            }
            continue;
        }

        step_count += 1;

        let step_index = value
            .get("step_index")
            .and_then(|v| v.as_i64())
            .unwrap_or(step_count);
        let session_id_str = session_id.as_deref().unwrap_or("unknown");
        let request_id = format!("gemini_session:{session_id_str}:step_{step_index}");

        let tokens = GeminiTokens {
            input,
            output,
            cached,
            thoughts: 0,
        };

        // 计算耗时与首字时间（单位毫秒）
        let latency_ms = match (last_boundary_ms, current_ms) {
            (Some(start), Some(curr)) => {
                if curr > start {
                    curr - start
                } else if output >= 200 {
                    1000
                } else if output > 0 {
                    (output as i64 * 10).clamp(200, 800)
                } else {
                    200
                }
            }
            _ => {
                if output >= 200 {
                    1000
                } else {
                    500
                }
            }
        };

        let first_token_ms = if latency_ms >= 1000 {
            Some((latency_ms / 3).clamp(200, 1000))
        } else if latency_ms > 0 {
            Some((latency_ms / 2).max(100))
        } else {
            None
        };

        if let Some(ms) = current_ms {
            last_boundary_ms = Some(ms);
        }

        match insert_gemini_session_entry(
            db,
            &request_id,
            &tokens,
            &current_model,
            session_id.as_deref(),
            timestamp_str,
            latency_ms,
            first_token_ms,
        ) {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                log::warn!("[AGY-SYNC] 插入失败 ({}): {e}", request_id);
                skipped += 1;
            }
        }
    }

    update_sync_state(db, &file_path_str, file_modified, step_count)?;
    Ok((imported, skipped))
}

/// 同步单个 Gemini / agy 会话文件，返回 (imported, skipped)。
///
/// `last_modified` 来自调用方批量预取的游标（见 [`crate::services::session_usage::load_sync_cursors`]）。
fn sync_single_gemini_file(
    db: &Database,
    file_path: &Path,
    last_modified: i64,
) -> Result<(u32, u32), AppError> {
    if file_path.file_name().and_then(|n| n.to_str()) == Some("transcript.jsonl") {
        return sync_single_agy_transcript_file(db, file_path, last_modified);
    }

    let file_path_str = file_path.to_string_lossy().to_string();

    // 获取文件元数据
    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("无法读取文件元数据: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    if metadata.len() > MAX_GEMINI_FILE_BYTES {
        return Err(AppError::Config(format!("Gemini session file exceeds {} bytes", MAX_GEMINI_FILE_BYTES)));
    }

    // 文件未变化则跳过
    if file_modified <= last_modified {
        return Ok((0, 0));
    }

    // 读取并解析整个 JSON 文件
    let content = fs::read_to_string(file_path)
        .map_err(|e| AppError::Config(format!("无法读取文件: {e}")))?;
    let value = parse_session_document(&content)
        .ok_or_else(|| AppError::Config("JSON 解析失败".to_string()))?;

    // 提取顶层 sessionId
    let session_id = value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 遍历 messages 数组
    let messages = match value.get("messages").and_then(|v| v.as_array()) {
        Some(msgs) => msgs,
        None => return Ok((0, 0)),
    };

    let mut imported: u32 = 0;
    let mut skipped: u32 = 0;
    let mut gemini_msg_count: i64 = 0;
    let mut last_boundary_ms: Option<i64> = None;

    for msg in messages {
        let msg_type = msg.get("type").and_then(|t| t.as_str());
        let timestamp_str = msg.get("timestamp").and_then(|v| v.as_str());
        let current_ms = timestamp_str.and_then(crate::services::session_usage::parse_timestamp_millis);

        // 只处理 type == "gemini" 的消息，其他消息记录为边界起点
        if msg_type != Some("gemini") {
            if let Some(ms) = current_ms {
                last_boundary_ms = Some(ms);
            }
            continue;
        }

        // 提取 tokens 对象
        let tokens_obj = match msg.get("tokens") {
            Some(t) if t.is_object() => t,
            _ => continue,
        };

        let tokens = parse_gemini_tokens(tokens_obj);
        if tokens.input == 0 && tokens.output == 0 && tokens.thoughts == 0 && tokens.cached == 0 {
            if let Some(ms) = current_ms {
                last_boundary_ms = Some(ms);
            }
            continue; // 跳过全零的空 token 消息
        }

        gemini_msg_count += 1;

        // 提取消息 ID 和模型
        let message_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let model = msg
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let total_output = tokens.output + tokens.thoughts;
        let latency_ms = match (last_boundary_ms, current_ms) {
            (Some(start), Some(curr)) if curr > start => curr - start,
            _ => {
                if total_output >= 200 {
                    1000
                } else if total_output > 0 {
                    (total_output as i64 * 10).clamp(200, 800)
                } else {
                    200
                }
            }
        };

        let first_token_ms = if latency_ms >= 1000 {
            Some((latency_ms / 3).clamp(200, 1000))
        } else if latency_ms > 0 {
            Some((latency_ms / 2).max(100))
        } else {
            None
        };

        if let Some(ms) = current_ms {
            last_boundary_ms = Some(ms);
        }

        // 生成唯一 request_id
        let session_id_str = session_id.as_deref().unwrap_or("unknown");
        let request_id = format!("gemini_session:{session_id_str}:{message_id}");

        match insert_gemini_session_entry(
            db,
            &request_id,
            &tokens,
            model,
            session_id.as_deref(),
            timestamp_str,
            latency_ms,
            first_token_ms,
        ) {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                log::warn!("[GEMINI-SYNC] 插入失败 ({}): {e}", request_id);
                skipped += 1;
            }
        }
    }

    // 更新同步状态
    update_sync_state(db, &file_path_str, file_modified, gemini_msg_count)?;

    Ok((imported, skipped))
}

/// 从 tokens JSON 对象中提取 token 数据
fn parse_gemini_tokens(tokens: &serde_json::Value) -> GeminiTokens {
    GeminiTokens {
        input: tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        output: tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        cached: tokens.get("cached").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        thoughts: tokens.get("thoughts").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    }
}

/// 插入单条 Gemini 会话记录到 proxy_request_logs
fn insert_gemini_session_entry(
    db: &Database,
    request_id: &str,
    tokens: &GeminiTokens,
    model: &str,
    session_id: Option<&str>,
    timestamp: Option<&str>,
    latency_ms: i64,
    first_token_ms: Option<i64>,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);

    let created_at = timestamp
        .and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.timestamp())
        })
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });

    // 合并 thoughts 到 output（思考 token 按输出计费）
    let output_tokens = tokens.output + tokens.thoughts;

    let dedup_key = DedupKey {
        app_type: "gemini",
        model,
        input_tokens: tokens.input,
        output_tokens,
        cache_read_tokens: tokens.cached,
        cache_creation_tokens: 0,
        created_at,
    };
    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    // 计算费用
    let usage = TokenUsage {
        input_tokens: tokens.input,
        output_tokens,
        cache_read_tokens: tokens.cached,
        cache_creation_tokens: 0,
        cache_creation_1h_tokens: 0,
        model: Some(model.to_string()),
        message_id: None,
    };

    let pricing = find_gemini_pricing(&conn, model);
    let multiplier = Decimal::from(1);
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) = match pricing
    {
        Some(p) => {
            let cost = CostCalculator::calculate_for_app("gemini", &usage, &p, multiplier);
            (
                cost.input_cost.to_string(),
                cost.output_cost.to_string(),
                cost.cache_read_cost.to_string(),
                cost.cache_creation_cost.to_string(),
                cost.total_cost.to_string(),
            )
        }
        None => (
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
            "0".to_string(),
        ),
    };

    // 使用 UPSERT：新记录插入，已存在记录更新 token、费用、耗时及首字时间
    conn.execute(
        "INSERT INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
        ON CONFLICT(request_id) DO UPDATE SET
            model = excluded.model,
            input_tokens = excluded.input_tokens,
            output_tokens = excluded.output_tokens,
            cache_read_tokens = excluded.cache_read_tokens,
            input_cost_usd = excluded.input_cost_usd,
            output_cost_usd = excluded.output_cost_usd,
            cache_read_cost_usd = excluded.cache_read_cost_usd,
            cache_creation_cost_usd = excluded.cache_creation_cost_usd,
            total_cost_usd = excluded.total_cost_usd,
            latency_ms = excluded.latency_ms,
            first_token_ms = excluded.first_token_ms
        WHERE input_tokens != excluded.input_tokens
           OR output_tokens != excluded.output_tokens
           OR cache_read_tokens != excluded.cache_read_tokens
           OR model != excluded.model
           OR latency_ms != excluded.latency_ms
           OR (first_token_ms IS NULL AND excluded.first_token_ms IS NOT NULL)",
        rusqlite::params![
            request_id,
            "_gemini_session",   // provider_id
            "gemini",            // app_type
            model,
            model,               // request_model = model
            tokens.input,
            output_tokens,
            tokens.cached,
            0i64,                // cache_creation_tokens
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
            latency_ms,
            first_token_ms,
            200i64,              // status_code
            Option::<String>::None, // error_message
            session_id.map(|s| s.to_string()),
            Some("gemini_session"), // provider_type
            1i64,                // is_streaming
            "1.0",               // cost_multiplier
            created_at,
            "gemini_session",    // data_source
        ],
    )
    .map_err(|e| AppError::Database(format!("插入 Gemini 会话日志失败: {e}")))?;

    // changes() > 0 表示新插入或已更新，== 0 表示值完全相同（无实际变更）
    let changed = conn.changes() > 0;
    Ok(changed)
}

/// 查找 Gemini 模型定价
fn find_gemini_pricing(conn: &rusqlite::Connection, model_id: &str) -> Option<ModelPricing> {
    find_model_pricing(conn, model_id)
}

fn sqlite_table_exists(conn: &rusqlite::Connection, table: &str) -> Result<bool, AppError> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )
        .map_err(|error| AppError::Database(format!("检查表是否存在失败: {error}")))?;
    Ok(exists)
}

fn sqlite_column_exists(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> Result<bool, AppError> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| AppError::Database(format!("读取表信息失败: {error}")))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| AppError::Database(format!("查询列信息失败: {error}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Database(format!("解析列信息失败: {error}")))?;
    Ok(columns.iter().any(|col| col == column))
}

fn is_gemini_cursor_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("transcript.jsonl")
        || lower.contains("session-")
        || lower.contains(".gemini")
        || lower.contains("antigravity")
}

pub(crate) fn reset_gemini_usage_on_conn(conn: &rusqlite::Connection) -> Result<(), AppError> {
    if sqlite_table_exists(conn, "proxy_request_logs")?
        && sqlite_column_exists(conn, "proxy_request_logs", "data_source")?
    {
        conn.execute(
            "DELETE FROM proxy_request_logs WHERE data_source = 'gemini_session'",
            [],
        )
        .map_err(|error| AppError::Database(format!("清理 Gemini/agy 会话明细失败: {error}")))?;
    }
    if sqlite_table_exists(conn, "usage_daily_rollups")?
        && sqlite_column_exists(conn, "usage_daily_rollups", "provider_id")?
    {
        conn.execute(
            "DELETE FROM usage_daily_rollups WHERE provider_id = '_gemini_session'",
            [],
        )
        .map_err(|error| AppError::Database(format!("清理 Gemini/agy 用量汇总失败: {error}")))?;
    }
    if sqlite_table_exists(conn, "session_log_sync")?
        && sqlite_column_exists(conn, "session_log_sync", "file_path")?
    {
        let paths = {
            let mut statement = conn
                .prepare("SELECT file_path FROM session_log_sync")
                .map_err(|error| {
                    AppError::Database(format!("读取会话同步 cursor 失败: {error}"))
                })?;
            let paths = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| AppError::Database(format!("查询会话同步 cursor 失败: {error}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    AppError::Database(format!("解析会话同步 cursor 失败: {error}"))
                })?;
            paths
        };
        for file_path in paths
            .into_iter()
            .filter(|path| is_gemini_cursor_path(path))
        {
            conn.execute(
                "DELETE FROM session_log_sync WHERE file_path = ?1",
                [file_path],
            )
            .map_err(|error| AppError::Database(format!("清理 Gemini/agy 同步 cursor 失败: {error}")))?;
        }
    }
    Ok(())
}

impl Database {
    pub(crate) fn reset_gemini_usage(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute("SAVEPOINT reset_gemini_usage", [])
            .map_err(|error| AppError::Database(format!("开启 Gemini/agy 重建事务失败: {error}")))?;
        let result = reset_gemini_usage_on_conn(&conn);
        match result {
            Ok(()) => {
                conn.execute("RELEASE reset_gemini_usage", [])
                    .map_err(|error| {
                        AppError::Database(format!("提交 Gemini/agy 重建事务失败: {error}"))
                    })?;
                Ok(())
            }
            Err(error) => {
                conn.execute("ROLLBACK TO reset_gemini_usage", []).ok();
                conn.execute("RELEASE reset_gemini_usage", []).ok();
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_gemini_session_files_nonexistent() {
        let files = collect_gemini_session_files(Path::new("/nonexistent/path"));
        assert!(files.is_empty());
    }

    #[test]
    fn collects_classic_chats_beside_antigravity_cli() {
        let root = tempfile::tempdir().expect("temp");
        let chats = root.path().join("tmp").join("proj").join("chats");
        std::fs::create_dir_all(&chats).expect("chats");
        let session = chats.join("session-classic.json");
        std::fs::write(&session, "{}").expect("session");
        let agy = root.path().join("antigravity-cli");
        std::fs::create_dir_all(&agy).expect("agy");

        let files = collect_gemini_session_files(&agy);
        assert!(
            files.iter().any(|path| path.ends_with("session-classic.json")),
            "classic ~/.gemini/tmp chats must still be scanned when antigravity-cli exists"
        );
    }

    #[test]
    fn test_insert_gemini_session_skips_matching_proxy_log() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    total_cost_usd, latency_ms, status_code, created_at, data_source
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    "gemini-proxy",
                    "google",
                    "gemini",
                    "gemini-2.5-pro",
                    "gemini-2.5-pro",
                    10,
                    7,
                    1,
                    0,
                    "0.01",
                    100,
                    200,
                    1000,
                    "proxy"
                ],
            )?;
        }

        let tokens = GeminiTokens {
            input: 10,
            output: 2,
            cached: 1,
            thoughts: 5,
        };
        let inserted = insert_gemini_session_entry(
            &db,
            "gemini-session-dup",
            &tokens,
            "gemini-2.5-pro",
            Some("session-1"),
            Some("1970-01-01T00:16:45Z"),
            1000,
            Some(300),
        )?;
        assert!(!inserted);

        let conn = lock_conn!(db.conn);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[test]
    fn test_parse_gemini_tokens() {
        let json: serde_json::Value = serde_json::json!({
            "input": 8522,
            "output": 29,
            "cached": 3138,
            "thoughts": 405,
            "tool": 0,
            "total": 8956
        });
        let tokens = parse_gemini_tokens(&json);
        assert_eq!(tokens.input, 8522);
        assert_eq!(tokens.output, 29);
        assert_eq!(tokens.cached, 3138);
        assert_eq!(tokens.thoughts, 405);
        // output + thoughts = 29 + 405 = 434（用于计费）
        assert_eq!(tokens.output + tokens.thoughts, 434);
    }

    #[test]
    fn test_parse_gemini_tokens_missing_fields() {
        // 缺少某些字段时应返回 0
        let json: serde_json::Value = serde_json::json!({
            "input": 100,
            "output": 50
        });
        let tokens = parse_gemini_tokens(&json);
        assert_eq!(tokens.input, 100);
        assert_eq!(tokens.output, 50);
        assert_eq!(tokens.cached, 0);
        assert_eq!(tokens.thoughts, 0);
    }

    #[test]
    fn test_parse_gemini_tokens_all_zero() {
        let json: serde_json::Value = serde_json::json!({
            "input": 0,
            "output": 0,
            "cached": 0,
            "thoughts": 0,
            "tool": 0,
            "total": 0
        });
        let tokens = parse_gemini_tokens(&json);
        assert_eq!(tokens.input, 0);
        assert_eq!(tokens.output, 0);
        // 全零（包括 cached=0）会被 sync 逻辑跳过
        assert!(
            tokens.input == 0 && tokens.output == 0 && tokens.thoughts == 0 && tokens.cached == 0
        );
    }

    #[test]
    fn test_parse_gemini_tokens_cache_only_not_skipped() {
        // 纯缓存命中消息（input/output/thoughts=0 但 cached>0）不应被跳过
        let json: serde_json::Value = serde_json::json!({
            "input": 0,
            "output": 0,
            "cached": 5000,
            "thoughts": 0
        });
        let tokens = parse_gemini_tokens(&json);
        assert_eq!(tokens.cached, 5000);
        // 跳过条件：所有四个字段都为 0 才跳过
        let should_skip =
            tokens.input == 0 && tokens.output == 0 && tokens.thoughts == 0 && tokens.cached == 0;
        assert!(!should_skip, "纯缓存命中记录不应被跳过");
    }

    #[test]
    fn test_normalize_agy_model_name() {
        assert_eq!(
            normalize_agy_model_name("Gemini 3.8 Flash (Medium)"),
            "gemini-3.8-flash"
        );
        assert_eq!(
            normalize_agy_model_name("Gemini 3.8 Flash (High)"),
            "gemini-3.8-flash"
        );
        assert_eq!(
            normalize_agy_model_name("Gemini 2.5 Pro"),
            "gemini-2.5-pro"
        );
        assert_eq!(
            normalize_agy_model_name("gemini-4-argon"),
            "gemini-4-argon"
        );
        assert_eq!(normalize_agy_model_name(""), "gemini-3.8-flash");
    }

    #[test]
    fn test_extract_model_from_content() {
        let content = "<USER_SETTINGS_CHANGE>\nThe user changed setting `Model Selection` from None to Gemini 3.8 Flash (High). No need to comment on this change\n</USER_SETTINGS_CHANGE>";
        assert_eq!(
            extract_model_from_content(content).as_deref(),
            Some("gemini-3.8-flash")
        );

        let without_model = "Hello world!";
        assert_eq!(extract_model_from_content(without_model), None);
    }

    #[test]
    fn test_reset_gemini_usage() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model, request_model,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    total_cost_usd, latency_ms, status_code, created_at, data_source
                ) VALUES ('gemini-1', '_gemini_session', 'gemini', 'gemini-3.8-flash', 'gemini-3.8-flash',
                    10, 10, 0, 0, '0.001', 0, 200, 1000, 'gemini_session')",
                [],
            )?;
            conn.execute(
                "INSERT INTO session_log_sync (file_path, last_modified, last_line_offset, last_synced_at)
                 VALUES ('/path/to/transcript.jsonl', 100, 10, 1000)",
                [],
            )?;
        }

        db.reset_gemini_usage()?;

        let conn = lock_conn!(db.conn);
        let log_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM proxy_request_logs WHERE data_source = 'gemini_session'",
            [],
            |r| r.get(0),
        )?;
        let sync_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_log_sync WHERE file_path LIKE '%transcript.jsonl%'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(log_count, 0);
        assert_eq!(sync_count, 0);

        Ok(())
    }

    #[test]
    fn test_agy_transcript_timing_latency_and_first_token() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp_dir = tempfile::tempdir().map_err(|e| AppError::Config(e.to_string()))?;
        let conv_dir = temp_dir.path().join("conv-123").join(".system_generated").join("logs");
        fs::create_dir_all(&conv_dir).map_err(|e| AppError::Config(e.to_string()))?;
        let transcript_path = conv_dir.join("transcript.jsonl");

        let content = r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-10-18T04:35:40Z","content":"hello"}
{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-18T04:35:43Z","input_tokens":1000,"cache_read_tokens":0,"output_tokens":250,"tool_calls":[{"name":"test"}]}
{"step_index":2,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-10-18T04:35:45Z","content":"done"}
{"step_index":3,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-10-18T04:35:47Z","input_tokens":1200,"cache_read_tokens":0,"output_tokens":300,"content":"world"}
"#;
        fs::write(&transcript_path, content).map_err(|e| AppError::Config(e.to_string()))?;

        let (imported, skipped) = sync_single_agy_transcript_file(&db, &transcript_path, 0)?;
        assert_eq!(imported, 2);
        assert_eq!(skipped, 0);

        {
            let conn = lock_conn!(db.conn);
            let mut stmt = conn.prepare("SELECT request_id, latency_ms, first_token_ms, output_tokens FROM proxy_request_logs ORDER BY request_id")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?.collect::<Result<Vec<_>, _>>()?;

            assert_eq!(rows.len(), 2);
            // Step 1: 04:35:40 -> 04:35:43 => 3000ms latency, first_token = 3000 / 3 = 1000ms
            assert_eq!(rows[0].0, "gemini_session:conv-123:step_1");
            assert_eq!(rows[0].1, 3000);
            assert_eq!(rows[0].2, Some(1000));
            assert_eq!(rows[0].3, 250);

            // Step 3: 04:35:45 -> 04:35:47 => 2000ms latency, first_token = 2000 / 3 = 666ms
            assert_eq!(rows[1].0, "gemini_session:conv-123:step_3");
            assert_eq!(rows[1].1, 2000);
            assert_eq!(rows[1].2, Some(666));
            assert_eq!(rows[1].3, 300);
        }

        // 验证 ModelStats 能计算出估算速度与首字中位数
        let model_stats = db.get_model_stats(None, None, None, None, None)?;
        let gemini_stat = model_stats.iter().find(|s| s.model == "gemini-3.8-flash").expect("gemini stat");
        assert_eq!(gemini_stat.est_speed_output_tokens, 550);
        assert_eq!(gemini_stat.est_speed_duration_ms, 5000);
        assert!(gemini_stat.est_first_token_median_ms.is_some());

        Ok(())
    }
}


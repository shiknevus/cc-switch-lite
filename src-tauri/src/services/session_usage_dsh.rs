//! DeepSeek Shell (dsh) 会话用量追踪
//!
//! 从 `~/.dsh/sessions/<project>/<session-id>/session.v*.jsonl.zstd` 中
//! 提取 token 使用数据、首字时延（first_token_ms）、生成耗时（duration_ms）
//! 和总延迟（latency_ms），写入 proxy_request_logs，实现官方直连/本地模式下的用量与速度统计。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::proxy::usage::calculator::CostCalculator;
use crate::proxy::usage::parser::TokenUsage;
use crate::services::session_usage::{
    metadata_modified_nanos, update_sync_state, SessionSyncResult,
};
use crate::services::usage_stats::{find_model_pricing, should_skip_session_insert, DedupKey};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// 解压可能由多个独立 zstd frame 串联组成的 zstd 压缩包（dsh 每次追加写入独立 frame）
pub fn decompress_zstd_multiframe(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let mut cursor = std::io::Cursor::new(bytes);
    let mut decompressed = Vec::new();

    while (cursor.position() as usize) < bytes.len() {
        let pos_before = cursor.position();

        // 检查剩余部分是否以 Zstandard Magic Header 开头
        let remaining = &bytes[pos_before as usize..];
        if remaining.len() < 4 || remaining[0..4] != ZSTD_MAGIC {
            // 如果不是 magic header，跳出以防死循环
            break;
        }

        let mut decoder = zstd::stream::Decoder::new(&mut cursor)?;
        let bytes_read = decoder.read_to_end(&mut decompressed)?;
        if bytes_read == 0 && cursor.position() == pos_before {
            break;
        }
    }

    // 若由于尾部对齐或单个流解码，decompressed 仍然为空，尝试直接全量 decode_all 作为后备
    if decompressed.is_empty() {
        if let Ok(all) = zstd::decode_all(std::io::Cursor::new(bytes)) {
            return Ok(all);
        }
    }

    Ok(decompressed)
}

/// dsh 会话顶层事件模型
#[derive(Debug, Deserialize)]
struct DshRawEvent {
    #[serde(rename = "type")]
    event_type: String,
    seq: Option<u64>,
    time: Option<u64>,
    id: Option<String>,
    data: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
struct ParsedDshEntry {
    seq: u64,
    time_ms: u64,
    model: String,
    _provider: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    latency_ms: u64,
    first_token_ms: Option<u64>,
    duration_ms: Option<u64>,
}

/// 获取 dsh 存储根目录，默认 `~/.dsh/sessions`
pub fn get_dsh_sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".dsh").join("sessions"))
}

/// 同步所有 dsh 会话日志
pub fn sync_dsh_usage(db: &Database) -> Result<SessionSyncResult, AppError> {
    let sessions_dir = match get_dsh_sessions_dir() {
        Some(d) if d.exists() => d,
        _ => return Ok(SessionSyncResult::default()),
    };

    let mut result = SessionSyncResult::default();
    let entries = match fs::read_dir(&sessions_dir) {
        Ok(e) => e,
        Err(err) => {
            log::warn!("[DSH-SYNC] 无法读取 sessions 目录 {:?}: {err}", sessions_dir);
            return Ok(result);
        }
    };

    for project_entry in entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let session_entries = match fs::read_dir(&project_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for sess_entry in session_entries.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }

            // 检查 session 目录下的文件
            let log_files = match fs::read_dir(&sess_path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for file_entry in log_files.flatten() {
                let file_path = file_entry.path();
                let file_name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();

                if file_name.starts_with("session.")
                    && (file_name.ends_with(".jsonl.zstd") || file_name.ends_with(".jsonl"))
                {
                    match sync_single_dsh_file(db, &file_path) {
                        Ok(sub) => result.merge(sub),
                        Err(err) => {
                            log::warn!("[DSH-SYNC] 处理文件 {:?} 出错: {err}", file_path);
                            result.errors.push(format!("{file_name}: {err}"));
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

/// 同步单个 dsh 会话文件
pub fn sync_single_dsh_file(db: &Database, file_path: &Path) -> Result<SessionSyncResult, AppError> {
    let mut result = SessionSyncResult::default();
    result.files_scanned = 1;

    let metadata = fs::metadata(file_path)
        .map_err(|e| AppError::Config(format!("无法获取 dsh 文件元数据: {e}")))?;
    let file_modified = metadata_modified_nanos(&metadata);
    let file_size = metadata.len() as i64;
    let file_path_str = file_path.to_string_lossy().to_string();

    let (last_modified, last_size) = crate::services::session_usage::get_sync_state(db, &file_path_str)?;
    if last_modified == file_modified && last_size == file_size {
        return Ok(result);
    }

    let file_bytes = fs::read(file_path)
        .map_err(|e| AppError::Config(format!("读取 dsh 文件失败: {e}")))?;

    let text = if file_path_str.ends_with(".zstd") {
        let decompressed = decompress_zstd_multiframe(&file_bytes)
            .map_err(|e| AppError::Config(format!("解压 dsh zstd 文件失败: {e}")))?;
        String::from_utf8_lossy(&decompressed).to_string()
    } else {
        String::from_utf8_lossy(&file_bytes).to_string()
    };

    let mut session_id = file_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut last_request_time: Option<u64> = None;
    let mut entries_to_insert = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let raw: DshRawEvent = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if raw.event_type == "session" {
            if let Some(id) = raw.id {
                session_id = id;
            }
            continue;
        }

        let event_time = raw.time.unwrap_or(0);

        match raw.event_type.as_str() {
            "user/message" | "tool/result" | "step/start" | "request/header" => {
                if event_time > 0 {
                    last_request_time = Some(event_time);
                }
            }
            "assistant/message" => {
                let seq = raw.seq.unwrap_or(line_idx as u64);
                let data = match raw.data {
                    Some(d) => d,
                    None => continue,
                };

                let usage = data.get("usage");
                let input_tokens = usage
                    .and_then(|u| u.get("inputTokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output_tokens = usage
                    .and_then(|u| u.get("outputTokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_read_tokens = usage
                    .and_then(|u| u.get("cacheReadTokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_creation_tokens = usage
                    .and_then(|u| u.get("cacheWriteTokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                if input_tokens == 0 && output_tokens == 0 {
                    continue;
                }

                let source = data
                    .get("message")
                    .and_then(|m| m.get("source"));
                let model = source
                    .and_then(|s| s.get("model"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("deepseek-chat")
                    .to_string();
                let provider = source
                    .and_then(|s| s.get("provider"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("deepseek-official")
                    .to_string();

                let stream_chunks = data
                    .get("stream")
                    .and_then(|s| s.as_array());

                let first_chunk_time = stream_chunks
                    .and_then(|arr| arr.first())
                    .and_then(|c| c.get("time"))
                    .and_then(|t| t.as_u64());

                let last_chunk_time = stream_chunks
                    .and_then(|arr| arr.last())
                    .and_then(|c| c.get("time"))
                    .and_then(|t| t.as_u64());

                let latency_ms = if let Some(req_t) = last_request_time {
                    event_time.saturating_sub(req_t)
                } else {
                    0
                };

                let first_token_ms = if let (Some(req_t), Some(first_t)) = (last_request_time, first_chunk_time) {
                    if first_t >= req_t {
                        Some(first_t - req_t)
                    } else {
                        Some(0)
                    }
                } else {
                    None
                };

                let duration_ms = if let (Some(first_t), Some(last_t)) = (first_chunk_time, last_chunk_time) {
                    if last_t >= first_t {
                        Some(last_t - first_t)
                    } else {
                        None
                    }
                } else {
                    None
                };

                entries_to_insert.push(ParsedDshEntry {
                    seq,
                    time_ms: event_time,
                    model,
                    _provider: provider,
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_creation_tokens,
                    latency_ms,
                    first_token_ms,
                    duration_ms,
                });
            }
            _ => {}
        }
    }

    for entry in entries_to_insert {
        let request_id = format!("dsh_session:{}:{}", session_id, entry.seq);
        match insert_dsh_session_entry(db, &request_id, &entry, &session_id) {
            Ok(true) => result.imported += 1,
            Ok(false) => result.skipped += 1,
            Err(err) => {
                log::warn!("[DSH-SYNC] 插入用量错误 ({}): {err}", request_id);
                result.skipped += 1;
            }
        }
    }

    update_sync_state(db, &file_path_str, file_modified, file_size)?;
    Ok(result)
}

/// 插入一条 dsh 会话日志记录到 proxy_request_logs
fn insert_dsh_session_entry(
    db: &Database,
    request_id: &str,
    entry: &ParsedDshEntry,
    session_id: &str,
) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);
    let created_at = (entry.time_ms / 1000) as i64;

    let dedup_key = DedupKey {
        app_type: "dsh",
        model: &entry.model,
        input_tokens: entry.input_tokens as u32,
        output_tokens: entry.output_tokens as u32,
        cache_read_tokens: entry.cache_read_tokens as u32,
        cache_creation_tokens: entry.cache_creation_tokens as u32,
        created_at,
    };

    if should_skip_session_insert(&conn, request_id, &dedup_key)? {
        return Ok(false);
    }

    let usage = TokenUsage {
        input_tokens: entry.input_tokens as u32,
        output_tokens: entry.output_tokens as u32,
        cache_read_tokens: entry.cache_read_tokens as u32,
        cache_creation_tokens: entry.cache_creation_tokens as u32,
        cache_creation_1h_tokens: 0,
        model: Some(entry.model.clone()),
        message_id: None,
    };

    let pricing = find_model_pricing(&conn, &entry.model);
    let multiplier = Decimal::from(1);
    let (input_cost, output_cost, cache_read_cost, cache_creation_cost, total_cost) = match pricing {
        Some(p) => {
            let cost = CostCalculator::calculate_for_app("dsh", &usage, &p, multiplier);
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

    let latency_ms = entry.latency_ms as i64;
    let first_token_ms = entry.first_token_ms.map(|ms| ms as i64);
    let duration_ms = entry.duration_ms.map(|ms| ms as i64);

    conn.execute(
        "INSERT INTO proxy_request_logs (
            request_id, provider_id, app_type, model, request_model,
            input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
            input_cost_usd, output_cost_usd, cache_read_cost_usd, cache_creation_cost_usd, total_cost_usd,
            latency_ms, first_token_ms, duration_ms, status_code, error_message, session_id,
            provider_type, is_streaming, cost_multiplier, created_at, data_source
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
        ON CONFLICT(request_id) DO UPDATE SET
            model = excluded.model,
            input_tokens = excluded.input_tokens,
            output_tokens = excluded.output_tokens,
            cache_read_tokens = excluded.cache_read_tokens,
            cache_creation_tokens = excluded.cache_creation_tokens,
            total_cost_usd = excluded.total_cost_usd,
            latency_ms = excluded.latency_ms,
            first_token_ms = excluded.first_token_ms,
            duration_ms = excluded.duration_ms
        WHERE (latency_ms = 0 AND excluded.latency_ms != 0)
           OR latency_ms != excluded.latency_ms
           OR (first_token_ms IS NULL AND excluded.first_token_ms IS NOT NULL)
           OR (duration_ms IS NULL AND excluded.duration_ms IS NOT NULL)",
        rusqlite::params![
            request_id,
            "_dsh_session",
            "dsh",
            entry.model,
            entry.model,
            entry.input_tokens as i64,
            entry.output_tokens as i64,
            entry.cache_read_tokens as i64,
            entry.cache_creation_tokens as i64,
            input_cost.to_string(),
            output_cost.to_string(),
            cache_read_cost.to_string(),
            cache_creation_cost.to_string(),
            total_cost.to_string(),
            latency_ms,
            first_token_ms,
            duration_ms,
            200, // status_code
            None::<String>,
            session_id,
            "dsh",
            1, // is_streaming
            1.0f64,
            created_at,
            "dsh_session"
        ],
    ).map_err(|e| AppError::Database(format!("插入 dsh 会话用量失败: {e}")))?;

    Ok(true)
}

pub(crate) fn reset_dsh_usage_on_conn(conn: &rusqlite::Connection) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM proxy_request_logs WHERE data_source = 'dsh_session' OR provider_id = '_dsh_session' OR app_type = 'dsh'",
        [],
    ).map_err(|e| AppError::Database(format!("重置 dsh 用量失败: {e}")))?;

    conn.execute(
        "DELETE FROM session_sync_state WHERE file_path LIKE '%.dsh%'",
        [],
    ).map_err(|e| AppError::Database(format!("清理 dsh 同步状态失败: {e}")))?;

    Ok(())
}

impl Database {
    pub(crate) fn reset_dsh_usage(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute("SAVEPOINT reset_dsh_usage", [])
            .map_err(|error| AppError::Database(format!("开启 dsh 重建事务失败: {error}")))?;
        let result = reset_dsh_usage_on_conn(&conn);
        match result {
            Ok(()) => {
                conn.execute("RELEASE reset_dsh_usage", [])
                    .map_err(|error| AppError::Database(format!("提交 dsh 重建事务失败: {error}")))?;
                Ok(())
            }
            Err(error) => {
                conn.execute("ROLLBACK TO reset_dsh_usage", []).ok();
                conn.execute("RELEASE reset_dsh_usage", []).ok();
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zstd_multiframe_decompression() {
        let frame1 = zstd::encode_all(std::io::Cursor::new(b"frame 1 content\n"), 3).unwrap();
        let frame2 = zstd::encode_all(std::io::Cursor::new(b"frame 2 content\n"), 3).unwrap();
        let mut combined = Vec::new();
        combined.extend_from_slice(&frame1);
        combined.extend_from_slice(&frame2);

        let decompressed = decompress_zstd_multiframe(&combined).unwrap();
        let text = String::from_utf8(decompressed).unwrap();
        assert_eq!(text, "frame 1 content\nframe 2 content\n");
    }

    #[test]
    fn test_dsh_session_timing_calculation() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("session.v4.jsonl");

        let lines = r#"{"type":"session","version":4,"id":"test-session-123","createdAt":1790403220000}
{"type":"step/start","seq":0,"time":1790403221000}
{"type":"user/message","seq":1,"time":1790403221100,"data":{"content":"hello"}}
{"type":"assistant/message","seq":2,"time":1790403223000,"data":{"usage":{"inputTokens":100,"outputTokens":50,"cacheReadTokens":20},"message":{"source":{"model":"deepseek-chat","provider":"deepseek-official"}},"stream":[{"type":"chunk","time":1790403222000,"chunk":{"type":"block-start"}},{"type":"chunk","time":1790403222800,"chunk":{"type":"finish"}}]}}
"#;

        fs::write(&file_path, lines).unwrap();

        let result = sync_single_dsh_file(&db, &file_path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let (latency, first_token, duration, output): (i64, Option<i64>, Option<i64>, i64) = conn.query_row(
            "SELECT latency_ms, first_token_ms, duration_ms, output_tokens FROM proxy_request_logs WHERE request_id = 'dsh_session:test-session-123:2'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;

        // last_request_time = 1790403221100 (user/message)
        // assistant event time = 1790403223000
        // latency = 1790403223000 - 1790403221100 = 1900
        assert_eq!(latency, 1900);
        // first_chunk_time = 1790403222000
        // first_token_ms = 1790403222000 - 1790403221100 = 900
        assert_eq!(first_token, Some(900));
        // duration_ms = 1790403222800 - 1790403222000 = 800
        assert_eq!(duration, Some(800));
        assert_eq!(output, 50);

        Ok(())
    }

    #[test]
    fn test_dsh_multiframe_zstd_file_sync() -> Result<(), AppError> {
        let db = Database::memory()?;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("session.v4.jsonl.zstd");

        let frame1_data = b"{\"type\":\"session\",\"version\":4,\"id\":\"test-zstd-session\",\"createdAt\":1790403220000}\n";
        let frame2_data = b"{\"type\":\"step/start\",\"seq\":0,\"time\":1790403221000}\n{\"type\":\"user/message\",\"seq\":1,\"time\":1790403221100,\"data\":{\"content\":\"hello\"}}\n";
        let frame3_data = b"{\"type\":\"assistant/message\",\"seq\":2,\"time\":1790403223000,\"data\":{\"usage\":{\"inputTokens\":200,\"outputTokens\":300,\"cacheReadTokens\":50},\"message\":{\"source\":{\"model\":\"deepseek-chat\",\"provider\":\"deepseek-official\"}},\"stream\":[{\"type\":\"chunk\",\"time\":1790403222000,\"chunk\":{\"type\":\"block-start\"}},{\"type\":\"chunk\",\"time\":1790403222900,\"chunk\":{\"type\":\"finish\"}}]}}\n";

        let mut combined = Vec::new();
        combined.extend_from_slice(&zstd::encode_all(std::io::Cursor::new(frame1_data), 3).unwrap());
        combined.extend_from_slice(&zstd::encode_all(std::io::Cursor::new(frame2_data), 3).unwrap());
        combined.extend_from_slice(&zstd::encode_all(std::io::Cursor::new(frame3_data), 3).unwrap());

        fs::write(&file_path, combined).unwrap();

        let result = sync_single_dsh_file(&db, &file_path)?;
        assert_eq!(result.imported, 1);

        let conn = lock_conn!(db.conn);
        let (latency, first_token, duration, output): (i64, Option<i64>, Option<i64>, i64) = conn.query_row(
            "SELECT latency_ms, first_token_ms, duration_ms, output_tokens FROM proxy_request_logs WHERE request_id = 'dsh_session:test-zstd-session:2'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;

        assert_eq!(latency, 1900);
        assert_eq!(first_token, Some(900));
        assert_eq!(duration, Some(900));
        assert_eq!(output, 300);

        Ok(())
    }
}

//! 数据库模块 - SQLite 数据持久化

pub(crate) mod backup;
pub mod dao;
pub(crate) mod schema;
mod sql_fns;

use crate::config::get_app_config_dir;
use crate::error::AppError;
use rusqlite::Connection;
use std::fs;
use std::sync::Mutex;

macro_rules! lock_conn {
    ($mutex:expr) => {
        $mutex.lock().unwrap_or_else(|e| {
            log::error!(
                "Database mutex poisoned: {}, recovering inner connection",
                e
            );
            e.into_inner()
        })
    };
}

pub(crate) use lock_conn;

pub struct Database {
    pub(crate) db_path: Option<std::path::PathBuf>,
    pub(crate) conn: Mutex<Connection>,
    pub(crate) log_count_cache: Mutex<Option<crate::services::usage_stats::LogCountCache>>,
}

impl Database {
    pub fn init() -> Result<Self, AppError> {
        let config_dir = get_app_config_dir();
        fs::create_dir_all(&config_dir).map_err(|e| AppError::io(&config_dir, e))?;

        let db_path = config_dir.join("cc-switch.db");
        log::info!("正在初始化数据库: {}", db_path.display());

        let conn = Connection::open(&db_path)?;

        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode = WAL;", [], |r| r.get(0))
            .map_err(|e| AppError::Database(e.to_string()))?;
        if journal_mode.to_lowercase() != "wal" {
            log::warn!("SQLite WAL journal mode not available ({journal_mode}), falling back to synchronous = FULL");
            conn.execute_batch("PRAGMA synchronous = FULL;")?;
        } else {
            conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        }

        conn.execute_batch(
            "PRAGMA cache_size = -65536;
             PRAGMA mmap_size = 268435456;
             PRAGMA temp_store = MEMORY;
             PRAGMA busy_timeout = 5000;
             PRAGMA optimize;",
        )?;
        sql_fns::register_decimal_functions(&conn)?;

        let db = Self {
            db_path: Some(db_path),
            conn: Mutex::new(conn),
            log_count_cache: Mutex::new(None),
        };

        db.create_tables()?;
        db.ensure_model_pricing_seeded()?;
        Ok(db)
    }

    #[cfg(test)]
    pub fn memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory()?;
        sql_fns::register_decimal_functions(&conn)?;
        let db = Self {
            db_path: None,
            conn: Mutex::new(conn),
            log_count_cache: Mutex::new(None),
        };
        db.create_tables()?;
        db.ensure_model_pricing_seeded()?;
        Ok(db)
    }
}

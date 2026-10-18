//! 数据库模块 - SQLite 数据持久化

pub(crate) mod backup;
pub mod dao;
pub(crate) mod schema;

use crate::config::get_app_config_dir;
use crate::error::AppError;
use rusqlite::Connection;
use std::fs;
use std::sync::Mutex;

macro_rules! lock_conn {
    ($mutex:expr) => {
        $mutex
            .lock()
            .map_err(|e| AppError::Database(format!("Mutex lock failed: {}", e)))?
    };
}

pub(crate) use lock_conn;

pub struct Database {
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

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;

        let db = Self {
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
        let db = Self {
            conn: Mutex::new(conn),
            log_count_cache: Mutex::new(None),
        };
        db.create_tables()?;
        db.ensure_model_pricing_seeded()?;
        Ok(db)
    }
}

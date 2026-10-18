use super::{lock_conn, Database};
use crate::config::get_app_config_dir;
use crate::error::AppError;
use chrono::Local;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

impl Database {
    pub(crate) fn backup_database_file(&self) -> Result<Option<PathBuf>, AppError> {
        let db_dir = get_app_config_dir();
        let db_path = db_dir.join("cc-switch.db");
        if !db_path.exists() {
            return Ok(None);
        }

        let backup_dir = db_dir.join("backups");
        fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let backup_file = backup_dir.join(format!("backup_{}.db", timestamp));

        let conn = lock_conn!(self.conn);
        let mut target_conn = Connection::open(&backup_file)?;
        let backup = rusqlite::backup::Backup::new(&conn, &mut target_conn)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(50), None)?;

        log::info!("数据库备份成功: {}", backup_file.display());
        Ok(Some(backup_file))
    }
}

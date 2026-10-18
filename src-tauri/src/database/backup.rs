use super::{lock_conn, Database};
use crate::config::get_app_config_dir;
use crate::error::AppError;
use chrono::Local;
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_BACKUPS: usize = 10;

fn prune_old_backups(backup_dir: &Path) {
    let mut backups = match fs::read_dir(backup_dir) {
        Ok(entries) => entries
            .flatten()
            .filter(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with("backup_") && name.ends_with(".db")
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            log::warn!("Unable to enumerate database backups: {error}");
            return;
        }
    };
    backups.sort_by_key(|entry| entry.file_name());
    while backups.len() > MAX_BACKUPS {
        if let Some(entry) = backups.first() {
            if let Err(error) = fs::remove_file(entry.path()) {
                log::warn!("Unable to prune database backup {}: {error}", entry.path().display());
                break;
            }
        }
        backups.remove(0);
    }
}

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
        let backup_file = backup_dir.join(format!("backup_{timestamp}_{}.db", Uuid::new_v4()));

        let conn = lock_conn!(self.conn);
        let mut target_conn = Connection::open(&backup_file)?;
        let backup = rusqlite::backup::Backup::new(&conn, &mut target_conn)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(50), None)?;
        drop(backup);
        drop(target_conn);
        prune_old_backups(&backup_dir);

        log::info!("Database backup created: {}", backup_file.display());
        Ok(Some(backup_file))
    }

    pub(crate) fn restore_database_file(&self, backup_file: &Path) -> Result<(), AppError> {
        if !backup_file.is_file() {
            return Err(AppError::io(
                backup_file,
                std::io::Error::new(std::io::ErrorKind::NotFound, "backup file not found"),
            ));
        }
        let source = Connection::open_with_flags(backup_file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut conn = lock_conn!(self.conn);
        let backup = rusqlite::backup::Backup::new(&source, &mut conn)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(50), None)?;
        log::info!("Database restored from backup: {}", backup_file.display());
        Ok(())
    }
}

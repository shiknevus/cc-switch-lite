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
    backups.sort_by_key(|entry| {
        let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        (entry.file_name(), mtime)
    });
    while backups.len() > MAX_BACKUPS {
        if let Some(entry) = backups.first() {
            if let Err(error) = fs::remove_file(entry.path()) {
                log::warn!(
                    "Unable to prune database backup {}: {error}",
                    entry.path().display()
                );
                break;
            }
        }
        backups.remove(0);
    }
}

impl Database {
    pub(crate) fn backup_database_file(&self) -> Result<Option<PathBuf>, AppError> {
        let db_path = match &self.db_path {
            Some(path) => path.clone(),
            None => get_app_config_dir().join("cc-switch.db"),
        };
        if !db_path.exists() {
            return Ok(None);
        }

        let backup_dir = db_path
            .parent()
            .map(|p| p.join("backups"))
            .unwrap_or_else(|| get_app_config_dir().join("backups"));
        fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let temp_backup_file =
            backup_dir.join(format!("backup_{timestamp}_{}.tmp", Uuid::new_v4()));
        let final_backup_file =
            backup_dir.join(format!("backup_{timestamp}_{}.db", Uuid::new_v4()));

        {
            let conn = lock_conn!(self.conn);
            let mut target_conn = Connection::open(&temp_backup_file)?;
            let backup = rusqlite::backup::Backup::new(&conn, &mut target_conn)?;
            backup.run_to_completion(1000, std::time::Duration::from_millis(0), None)?;
            drop(backup);

            // Verify backup integrity before naming it as final
            let check: String = target_conn
                .query_row("PRAGMA quick_check;", [], |r| r.get(0))
                .map_err(|e| AppError::Database(format!("Backup verification failed: {e}")))?;
            if check.to_lowercase() != "ok" {
                drop(target_conn);
                let _ = fs::remove_file(&temp_backup_file);
                return Err(AppError::Database(format!(
                    "Backup quick_check failed: {check}"
                )));
            }
        }

        fs::rename(&temp_backup_file, &final_backup_file)
            .map_err(|e| AppError::io(&final_backup_file, e))?;

        prune_old_backups(&backup_dir);
        log::info!("Database backup created: {}", final_backup_file.display());
        Ok(Some(final_backup_file))
    }

    pub(crate) fn restore_database_file(&self, backup_file: &Path) -> Result<(), AppError> {
        if !backup_file.is_file() {
            return Err(AppError::io(
                backup_file,
                std::io::Error::new(std::io::ErrorKind::NotFound, "backup file not found"),
            ));
        }
        let source = Connection::open_with_flags(backup_file, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let check: String = source
            .query_row("PRAGMA quick_check;", [], |r| r.get(0))
            .map_err(|e| {
                AppError::Database(format!("Backup quick_check failed before restore: {e}"))
            })?;
        if check.to_lowercase() != "ok" {
            return Err(AppError::Database(format!("Backup file corrupt: {check}")));
        }

        {
            let mut conn = lock_conn!(self.conn);
            let backup = rusqlite::backup::Backup::new(&source, &mut conn)?;
            backup.run_to_completion(1000, std::time::Duration::from_millis(0), None)?;
            drop(backup);
            drop(source);

            // Re-run migration and seed to ensure restored database schema matches current code
            Self::create_tables_on_conn(&conn)?;
            Self::ensure_model_pricing_seeded_on_conn(&conn)?;
        }

        // Invalidate cached stats
        *lock_conn!(self.log_count_cache) = None;

        log::info!("Database restored from backup: {}", backup_file.display());
        Ok(())
    }
}

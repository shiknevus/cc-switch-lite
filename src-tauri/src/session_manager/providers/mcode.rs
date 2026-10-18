use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

pub(crate) fn database_path() -> PathBuf {
    crate::config::get_mcode_data_dir().join("v2/sqlite/runtime-state.sqlite")
}

pub(crate) fn open_database() -> rusqlite::Result<Connection> {
    Connection::open_with_flags(database_path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
}

#[cfg(test)]
pub fn scan_sessions() -> Vec<crate::session_manager::SessionMeta> {
    Vec::new()
}

#[cfg(test)]
pub fn load_messages(_path: &str) -> Result<Vec<()>, String> {
    Ok(Vec::new())
}


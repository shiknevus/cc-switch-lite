use std::path::PathBuf;

pub fn session_roots() -> Vec<PathBuf> {
    let config_dir = crate::config::get_home_dir().join(".grok");
    vec![
        config_dir.join("sessions"),
        config_dir.join("archived_sessions"),
    ]
}


use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const MAX_TREE_ENTRIES: usize = 500_000;
pub(crate) const MAX_TREE_ID_BYTES: usize = 256;
pub(crate) const MAX_SESSION_BYTES: u64 = 128 * 1024 * 1024;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionLayout {
    Flat,
    ProjectDirectories,
}

pub(crate) fn session_files() -> Result<Vec<PathBuf>, String> {
    let root = if let Some(raw) = std::env::var_os("PI_CODING_AGENT_SESSION_DIR") {
        if !raw.is_empty() {
            PathBuf::from(raw)
        } else {
            crate::pi_config::get_pi_agent_dir().join("sessions")
        }
    } else {
        crate::pi_config::get_pi_agent_dir().join("sessions")
    };

    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_jsonl_files(&root, SessionLayout::ProjectDirectories, &mut files, false);
    let mut children = Vec::new();
    for session in &files {
        collect_child_session_files(&session.with_extension(""), &mut children);
    }
    files.extend(children);
    files.sort();
    Ok(files)
}

pub(crate) fn is_valid_tree_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_TREE_ID_BYTES
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn collect_jsonl_files(
    root: &Path,
    layout: SessionLayout,
    output: &mut Vec<PathBuf>,
    enforce_size_limit: bool,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        match layout {
            SessionLayout::Flat if file_type.is_file() => {
                push_jsonl_file(&entry, output, enforce_size_limit);
            }
            SessionLayout::ProjectDirectories if file_type.is_dir() => {
                let Ok(project_entries) = fs::read_dir(entry.path()) else {
                    continue;
                };
                for project_entry in project_entries.flatten() {
                    if project_entry
                        .file_type()
                        .is_ok_and(|file_type| file_type.is_file())
                    {
                        push_jsonl_file(&project_entry, output, enforce_size_limit);
                    }
                }
            }
            SessionLayout::Flat | SessionLayout::ProjectDirectories => {}
        }
    }
}

fn push_jsonl_file(entry: &fs::DirEntry, output: &mut Vec<PathBuf>, enforce_size_limit: bool) {
    let path = entry.path();
    if path.extension().and_then(|value| value.to_str()) == Some("jsonl")
        && (!enforce_size_limit
            || entry
                .metadata()
                .is_ok_and(|metadata| metadata.len() <= MAX_SESSION_BYTES))
    {
        output.push(path);
    }
}

fn collect_child_session_files(dir: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if entry.file_name() != "subagent-artifacts" {
                collect_child_session_files(&entry.path(), output);
            }
        } else if file_type.is_file() {
            push_jsonl_file(&entry, output, false);
        }
    }
}

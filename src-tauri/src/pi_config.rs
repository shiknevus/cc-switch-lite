//! Lightweight pi_config shim for agent directory resolution and tests.

use std::path::PathBuf;
#[cfg(test)]
use std::sync::{LazyLock, Mutex};

#[cfg(test)]
pub static TEST_AGENT_DIR: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

pub fn get_pi_agent_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(path) = TEST_AGENT_DIR
        .lock()
        .expect("lock Pi test directory")
        .clone()
    {
        return path;
    }

    if let Some(raw) = std::env::var_os("PI_CODING_AGENT_DIR") {
        if !raw.is_empty() {
            return PathBuf::from(raw);
        }
    }

    crate::config::get_home_dir().join(".pi").join("agent")
}

#[cfg(test)]
pub mod test_support {
    use std::path::{Path, PathBuf};

    pub struct TestAgentDir {
        previous: Option<PathBuf>,
    }

    impl TestAgentDir {
        pub fn at(agent_dir: &Path) -> Self {
            let previous = super::TEST_AGENT_DIR
                .lock()
                .expect("lock Pi test directory")
                .replace(agent_dir.to_path_buf());
            Self { previous }
        }
    }

    impl Drop for TestAgentDir {
        fn drop(&mut self) {
            *super::TEST_AGENT_DIR
                .lock()
                .expect("lock Pi test directory") = self.previous.take();
        }
    }
}

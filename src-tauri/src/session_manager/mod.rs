pub mod providers;

#[derive(Debug, Clone, Default)]
pub struct SessionMeta {
    pub source_path: Option<String>,
}

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRoot {
    pub path: PathBuf,
    pub writable: bool,
}

impl SourceRoot {
    pub fn read_only(path: PathBuf) -> Self {
        Self {
            path,
            writable: false,
        }
    }
}

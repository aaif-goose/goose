use crate::config::paths::Paths;
use once_cell::sync::Lazy;
use std::fs;
use uuid::Uuid;

static INSTANCE_ID: Lazy<String> = Lazy::new(load_or_create);

fn file_path() -> std::path::PathBuf {
    Paths::state_dir().join("instance_id")
}

fn load_or_create() -> String {
    let path = file_path();

    if let Ok(id) = fs::read_to_string(&path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    let id = Uuid::new_v4().to_string();

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, &id);

    id
}

/// Returns a stable, globally unique identifier for this Goose installation.
/// The ID is generated once and persisted to disk, surviving restarts.
pub fn get_instance_id() -> &'static str {
    &INSTANCE_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_id_is_stable() {
        let id1 = get_instance_id();
        let id2 = get_instance_id();
        assert_eq!(id1, id2);
        assert!(!id1.is_empty());
    }
}

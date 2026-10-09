use super::is_safe_extension_id;
use crate::config::paths::Paths;
use anyhow::{bail, Result};
use fs_err as fs;
use std::collections::BTreeMap;
use std::path::PathBuf;

const MAX_KEYS: usize = 200;
const MAX_STORE_BYTES: usize = 512 * 1024;

fn storage_dir() -> PathBuf {
    Paths::in_agents_home_dir("client-extensions-storage")
}

fn storage_path(extension_id: &str) -> Result<PathBuf> {
    if !is_safe_extension_id(extension_id) {
        bail!("invalid extension id '{extension_id}'");
    }
    Ok(storage_dir().join(format!("{extension_id}.json")))
}

fn read_store(extension_id: &str) -> Result<BTreeMap<String, serde_json::Value>> {
    let path = storage_path(extension_id)?;
    match fs::read_to_string(&path) {
        Ok(contents) => Ok(serde_json::from_str(&contents).unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(error.into()),
    }
}

fn write_store(extension_id: &str, store: &BTreeMap<String, serde_json::Value>) -> Result<()> {
    let serialized = serde_json::to_string_pretty(store)?;
    if serialized.len() > MAX_STORE_BYTES {
        bail!("plugin storage is limited to {} KB", MAX_STORE_BYTES / 1024);
    }
    fs::create_dir_all(storage_dir())?;
    fs::write(storage_path(extension_id)?, serialized)?;
    Ok(())
}

pub fn get(extension_id: &str, key: &str) -> Result<Option<serde_json::Value>> {
    Ok(read_store(extension_id)?.get(key).cloned())
}

pub fn set(extension_id: &str, key: &str, value: serde_json::Value) -> Result<()> {
    let mut store = read_store(extension_id)?;
    if !store.contains_key(key) && store.len() >= MAX_KEYS {
        bail!("plugin storage is limited to {MAX_KEYS} keys");
    }
    store.insert(key.to_string(), value);
    write_store(extension_id, &store)
}

pub fn delete(extension_id: &str, key: &str) -> Result<bool> {
    let mut store = read_store(extension_id)?;
    let existed = store.remove(key).is_some();
    write_store(extension_id, &store)?;
    Ok(existed)
}

pub fn keys(extension_id: &str) -> Result<Vec<String>> {
    Ok(read_store(extension_id)?.keys().cloned().collect())
}

pub fn clear(extension_id: &str) -> Result<()> {
    let path = storage_path(extension_id)?;
    if path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    struct PathRootGuard(Option<std::ffi::OsString>);

    impl PathRootGuard {
        fn set(root: &std::path::Path) -> Self {
            let previous = std::env::var_os("GOOSE_PATH_ROOT");
            std::env::set_var("GOOSE_PATH_ROOT", root);
            Self(previous)
        }
    }

    impl Drop for PathRootGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(root) => std::env::set_var("GOOSE_PATH_ROOT", root),
                None => std::env::remove_var("GOOSE_PATH_ROOT"),
            }
        }
    }

    #[test]
    #[serial]
    fn stores_reads_deletes_and_lists_keys() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        assert_eq!(get("demo", "theme").unwrap(), None);

        set("demo", "theme", serde_json::json!("dark")).unwrap();
        set("demo", "count", serde_json::json!(3)).unwrap();

        assert_eq!(
            get("demo", "theme").unwrap(),
            Some(serde_json::json!("dark"))
        );
        assert_eq!(keys("demo").unwrap(), vec!["count", "theme"]);

        assert!(delete("demo", "theme").unwrap());
        assert!(!delete("demo", "theme").unwrap());
        assert_eq!(keys("demo").unwrap(), vec!["count"]);
    }

    #[test]
    #[serial]
    fn keeps_each_extension_in_its_own_store() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        set("a", "k", serde_json::json!("from-a")).unwrap();

        assert_eq!(get("b", "k").unwrap(), None);
        assert_eq!(get("a", "k").unwrap(), Some(serde_json::json!("from-a")));
    }

    #[test]
    #[serial]
    fn rejects_an_unsafe_extension_id() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        assert!(get("../evil", "k").is_err());
        assert!(set("../evil", "k", serde_json::json!(1)).is_err());
    }

    #[test]
    #[serial]
    fn enforces_the_key_limit() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        for i in 0..MAX_KEYS {
            set("demo", &format!("k{i}"), serde_json::json!(i)).unwrap();
        }

        assert!(set("demo", "one-too-many", serde_json::json!(true)).is_err());
        // Overwriting an existing key never counts against the limit.
        assert!(set("demo", "k0", serde_json::json!("ok")).is_ok());
    }

    #[test]
    #[serial]
    fn clear_removes_the_store_and_is_a_no_op_when_absent() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        set("demo", "k", serde_json::json!(1)).unwrap();
        clear("demo").unwrap();
        assert_eq!(keys("demo").unwrap(), Vec::<String>::new());

        clear("demo").unwrap();
        clear("never-had-one").unwrap();
    }
}

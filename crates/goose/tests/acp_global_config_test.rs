use goose::config::base::CONFIG_YAML_NAME;
use goose::config::paths::Paths;
use goose::config::{Config, ConfigHandle};
use std::sync::{Arc, Mutex};

/// Serializes config cache tests to prevent races with other tests in the same
/// binary that also mutate the process-global CONFIG_CACHE.
static CONFIG_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn default_config_dir_returns_global_handle() {
    let _guard = CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Config::clear_config_cache();

    let handle = Config::for_config_dir(Paths::config_dir()).unwrap();

    assert!(matches!(handle, ConfigHandle::Global));
}

#[test]
fn same_custom_config_dir_returns_same_cached_config() {
    let _guard = CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Config::clear_config_cache();

    let dir = tempfile::tempdir().unwrap();

    let first = Config::for_config_dir(dir.path().to_path_buf()).unwrap();
    let second = Config::for_config_dir(dir.path().to_path_buf()).unwrap();

    match (first, second) {
        (ConfigHandle::Cached(first), ConfigHandle::Cached(second)) => {
            assert!(Arc::ptr_eq(&first, &second));
        }
        _ => panic!("expected cached config handles"),
    }
}

#[test]
fn equivalent_missing_custom_config_dirs_share_cached_config() {
    let _guard = CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Config::clear_config_cache();

    let root = tempfile::tempdir().unwrap();
    let direct = root.path().join("missing");
    let with_parent = root.path().join("missing").join("..").join("missing");

    let first = Config::for_config_dir(direct).unwrap();
    let second = Config::for_config_dir(with_parent).unwrap();

    match (first, second) {
        (ConfigHandle::Cached(first), ConfigHandle::Cached(second)) => {
            assert!(Arc::ptr_eq(&first, &second));
        }
        _ => panic!("expected cached config handles"),
    }
}

#[test]
fn custom_config_dirs_can_coexist() {
    let _guard = CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Config::clear_config_cache();

    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();

    let first = Config::for_config_dir(first_dir.path().to_path_buf()).unwrap();
    let second = Config::for_config_dir(second_dir.path().to_path_buf()).unwrap();

    first.set_param("CUSTOM_KEY", "first").unwrap();
    second.set_param("CUSTOM_KEY", "second").unwrap();

    assert_eq!(
        first.get_param::<String>("CUSTOM_KEY").unwrap(),
        "first".to_string()
    );
    assert_eq!(
        second.get_param::<String>("CUSTOM_KEY").unwrap(),
        "second".to_string()
    );
    assert_eq!(
        first.path(),
        first_dir
            .path()
            .join(CONFIG_YAML_NAME)
            .display()
            .to_string()
    );
    assert_eq!(
        second.path(),
        second_dir
            .path()
            .join(CONFIG_YAML_NAME)
            .display()
            .to_string()
    );
}

#[test]
fn cached_handles_for_same_custom_path_share_written_state() {
    let _guard = CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Config::clear_config_cache();

    let dir = tempfile::tempdir().unwrap();

    let first = Config::for_config_dir(dir.path().to_path_buf()).unwrap();
    let second = Config::for_config_dir(dir.path().to_path_buf()).unwrap();

    first.set_param("FIRST_KEY", "first").unwrap();
    second.set_param("SECOND_KEY", "second").unwrap();

    assert_eq!(
        first.get_param::<String>("SECOND_KEY").unwrap(),
        "second".to_string()
    );
    assert_eq!(
        second.get_param::<String>("FIRST_KEY").unwrap(),
        "first".to_string()
    );
}

#[test]
fn concurrent_cached_config_writes_to_same_custom_path_do_not_lose_updates() {
    let _guard = CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Config::clear_config_cache();

    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path().to_path_buf();

    let handles = (0..24)
        .map(|i| {
            let dir_path = dir_path.clone();
            std::thread::spawn(move || {
                let config = Config::for_config_dir(dir_path).unwrap();
                let key = format!("CONCURRENT_KEY_{i}");
                config.set_param(&key, i).unwrap();
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().unwrap();
    }

    let config = Config::for_config_dir(dir.path().to_path_buf()).unwrap();
    for i in 0..24 {
        let key = format!("CONCURRENT_KEY_{i}");
        assert_eq!(config.get_param::<i64>(&key).unwrap(), i);
    }
}

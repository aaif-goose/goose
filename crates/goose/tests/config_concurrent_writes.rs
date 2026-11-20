use anyhow::{Context, Result};
use goose::config::Config;
use serde_yaml::{Mapping, Value};
use std::{env, fs, path::Path, time::Duration};
use tokio::process::Command;
use tokio::time::{sleep, timeout};

const CONFIG_DIR_ENV: &str = "GOOSE_TEST_SHARED_CONFIG_DIR";
const CONFIG_KEY_ENV: &str = "GOOSE_TEST_SHARED_CONFIG_KEY";
const WAIT: Duration = Duration::from_secs(30);

async fn wait_for_file(path: &Path) -> Result<()> {
    timeout(WAIT, async {
        while !path.exists() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .with_context(|| format!("waiting for {}", path.display()))
}

#[tokio::test]
#[ignore = "spawned by concurrent_config_writes_preserve_keys"]
async fn config_writer() -> Result<()> {
    let directory = std::path::PathBuf::from(env::var_os(CONFIG_DIR_ENV).context(CONFIG_DIR_ENV)?);
    let key = env::var(CONFIG_KEY_ENV)?;
    let config = Config::new_with_file_secrets(
        directory.join("config.yaml"),
        directory.join("secrets.yaml"),
    )?;

    fs::write(directory.join(format!("{key}.ready")), "")?;
    wait_for_file(&directory.join("start")).await?;
    config.update_param::<String, _, _>(&key, |_| {
        // Overlap the read-modify-write windows if the file lock is missing.
        std::thread::sleep(Duration::from_millis(100));
        format!("value for {key}")
    })?;
    Ok(())
}

#[tokio::test]
async fn concurrent_config_writes_preserve_keys() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config_path = directory.path().join("config.yaml");
    fs::write(&config_path, "{}\n")?;

    let test_exe = env::current_exe()?;
    let keys = ["first_key", "second_key"];
    let mut writers = Vec::new();
    for key in keys {
        writers.push(
            Command::new(&test_exe)
                .args(["--ignored", "--exact", "config_writer"])
                .env(CONFIG_DIR_ENV, directory.path())
                .env(CONFIG_KEY_ENV, key)
                .kill_on_drop(true)
                .spawn()?,
        );
    }

    for key in keys {
        wait_for_file(&directory.path().join(format!("{key}.ready"))).await?;
    }
    fs::write(directory.path().join("start"), "")?;
    for mut writer in writers {
        assert!(timeout(WAIT, writer.wait()).await??.success());
    }

    let saved: Mapping = serde_yaml::from_str(&fs::read_to_string(config_path)?)?;
    for key in keys {
        let expected = format!("value for {key}");
        assert_eq!(
            saved.get(key).and_then(Value::as_str),
            Some(expected.as_str())
        );
    }
    Ok(())
}

#[test]
fn initialize_and_clear_require_config_lock() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config_path = directory.path().join("config.yaml");
    let config =
        Config::new_with_file_secrets(&config_path, directory.path().join("secrets.yaml"))?;
    fs::create_dir(directory.path().join("config.yaml.lock"))?;

    assert!(config.initialize_if_empty(Mapping::new()).is_err());
    assert!(!config_path.exists());

    let original = "saved_setting: true\n";
    fs::write(&config_path, original)?;
    assert!(config.clear().is_err());
    assert_eq!(fs::read_to_string(config_path)?, original);
    Ok(())
}

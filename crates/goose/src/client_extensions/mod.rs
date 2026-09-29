use crate::config::paths::Paths;
use crate::utils::copy_dir_all;
use anyhow::{anyhow, bail, Context, Result};
use fs_err as fs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MANIFEST_FILENAME: &str = "client-extension.json";
const CONFIG_FILENAME: &str = "config.json";
const DEV_DIR_ENV: &str = "GOOSE_CLIENT_EXTENSIONS_DEV_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientExtensionSource {
    Installed,
    Dev,
}

impl std::fmt::Display for ClientExtensionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientExtensionSource::Installed => write!(f, "installed"),
            ClientExtensionSource::Dev => write!(f, "dev"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientExtensionSummary {
    pub id: String,
    pub version: String,
    pub directory: PathBuf,
    pub source: ClientExtensionSource,
    pub enabled: bool,
    pub manifest: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ClientExtensionInstall {
    pub id: String,
    pub version: String,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClientExtensionsConfig {
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default, rename = "enabledDev")]
    pub enabled_dev: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ClientExtensionManifest {
    id: String,
    version: String,
    main: String,
}

struct LoadedManifest {
    manifest: ClientExtensionManifest,
    raw: serde_json::Value,
}

pub fn client_extensions_dir() -> PathBuf {
    Paths::in_agents_home_dir("client-extensions")
}

fn config_path() -> PathBuf {
    client_extensions_dir().join(CONFIG_FILENAME)
}

fn load_client_extensions_config() -> Option<ClientExtensionsConfig> {
    let path = config_path();
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Some(ClientExtensionsConfig::default());
        }
        Err(error) => {
            tracing::error!("Failed to read client extensions config, disabling all: {error}");
            return None;
        }
    };

    match serde_json::from_str(&contents) {
        Ok(config) => Some(config),
        Err(error) => {
            tracing::error!(
                "Invalid client extensions config at {}, disabling all: {error}",
                path.display()
            );
            None
        }
    }
}

fn save_client_extensions_config(config: &ClientExtensionsConfig) -> Result<()> {
    let dir = client_extensions_dir();
    fs::create_dir_all(&dir)?;
    let mut disabled = config.disabled.clone();
    disabled.sort();
    disabled.dedup();
    let mut enabled_dev = config.enabled_dev.clone();
    enabled_dev.sort();
    enabled_dev.dedup();
    let normalized = ClientExtensionsConfig {
        disabled,
        enabled_dev,
    };
    fs::write(config_path(), serde_json::to_string_pretty(&normalized)?)?;
    Ok(())
}

fn is_client_extension_enabled(
    id: &str,
    source: ClientExtensionSource,
    config: Option<&ClientExtensionsConfig>,
) -> bool {
    let Some(config) = config else {
        return false;
    };
    if config.disabled.iter().any(|entry| entry == id) {
        return false;
    }
    match source {
        ClientExtensionSource::Installed => true,
        ClientExtensionSource::Dev => config.enabled_dev.iter().any(|entry| entry == id),
    }
}

fn set_client_extension_enabled(id: &str, enabled: bool) -> Result<ClientExtensionsConfig> {
    let summaries = list_client_extensions()?;
    let summary = summaries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("client extension '{id}' is not installed"))?;

    let mut config = load_client_extensions_config().unwrap_or_default();
    config.disabled.retain(|entry| entry != id);

    match summary.source {
        ClientExtensionSource::Installed => {
            if !enabled {
                config.disabled.push(id.to_string());
            }
        }
        ClientExtensionSource::Dev => {
            config.enabled_dev.retain(|entry| entry != id);
            if enabled {
                config.enabled_dev.push(id.to_string());
            } else {
                config.disabled.push(id.to_string());
            }
        }
    }

    save_client_extensions_config(&config)?;
    Ok(config)
}

pub fn enable_client_extension(id: &str) -> Result<()> {
    set_client_extension_enabled(id, true)?;
    Ok(())
}

pub fn disable_client_extension(id: &str) -> Result<()> {
    set_client_extension_enabled(id, false)?;
    Ok(())
}

pub fn uninstall_client_extension(id: &str) -> Result<()> {
    let summaries = list_client_extensions()?;
    let summary = summaries
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("client extension '{id}' is not installed"))?;

    if summary.source != ClientExtensionSource::Installed {
        bail!("cannot uninstall dev client extension '{id}' — disable it from Plugins instead");
    }

    if summary.directory.is_dir() {
        fs::remove_dir_all(&summary.directory)?;
    }

    let mut config = load_client_extensions_config().unwrap_or_default();
    config.disabled.retain(|entry| entry != id);
    config.enabled_dev.retain(|entry| entry != id);
    save_client_extensions_config(&config)?;

    Ok(())
}

pub fn install_client_extension(source: &Path) -> Result<ClientExtensionInstall> {
    let source = source
        .canonicalize()
        .with_context(|| format!("client extension source not found: {}", source.display()))?;
    if !source.is_dir() {
        bail!("client extension source must be a directory");
    }

    let manifest = read_manifest(&source)?.manifest;
    validate_manifest_files(&source, &manifest)?;

    let install_root = client_extensions_dir()
        .canonicalize()
        .unwrap_or_else(|_| client_extensions_dir());
    let destination = install_root.join(&manifest.id);
    if !destination.starts_with(&install_root) {
        bail!(
            "extension id '{}' would escape the install directory",
            manifest.id
        );
    }
    if destination.exists() {
        bail!(
            "client extension '{}' is already installed at {}",
            manifest.id,
            destination.display()
        );
    }

    fs::create_dir_all(client_extensions_dir())?;
    copy_dir_all(&source, &destination)?;

    Ok(ClientExtensionInstall {
        id: manifest.id,
        version: manifest.version,
        directory: destination,
    })
}

pub fn list_client_extensions() -> Result<Vec<ClientExtensionSummary>> {
    let config = load_client_extensions_config();
    let mut by_id = std::collections::BTreeMap::new();

    let install_root = client_extensions_dir();
    if install_root.is_dir() {
        collect_extensions_in_root(&install_root, ClientExtensionSource::Installed, &mut by_id)?;
    }

    if let Some(dev_root) = dev_client_extensions_dir() {
        let mut dev_by_id = std::collections::BTreeMap::new();
        collect_extensions_in_root(&dev_root, ClientExtensionSource::Dev, &mut dev_by_id)?;
        for (id, summary) in dev_by_id {
            by_id.entry(id).or_insert(summary);
        }
    }

    Ok(by_id
        .into_values()
        .map(|mut summary| {
            summary.enabled =
                is_client_extension_enabled(&summary.id, summary.source.clone(), config.as_ref());
            summary
        })
        .collect())
}

pub fn read_client_extension_main(id: &str) -> Result<String> {
    let summary = list_client_extensions()?
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| anyhow!("client extension '{id}' is not installed"))?;

    if !summary.enabled {
        bail!("client extension '{id}' is disabled");
    }

    let manifest = read_manifest(&summary.directory)?.manifest;
    let main_path = resolve_main_path(&summary.directory, &manifest.main)?;
    Ok(fs::read_to_string(main_path)?)
}

fn dev_client_extensions_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(DEV_DIR_ENV).map(PathBuf::from) {
        return dir.is_dir().then_some(dir);
    }

    let cwd = std::env::current_dir().ok()?;
    [
        cwd.join("examples").join("client-extensions"),
        cwd.join("..")
            .join("..")
            .join("examples")
            .join("client-extensions"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_dir())
}

fn collect_extensions_in_root(
    root: &Path,
    source: ClientExtensionSource,
    by_id: &mut std::collections::BTreeMap<String, ClientExtensionSummary>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == CONFIG_FILENAME {
            continue;
        }
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let directory = entry.path();
        let loaded = match read_manifest(&directory) {
            Ok(loaded) => loaded,
            Err(_) => continue,
        };
        if entry.file_name() != loaded.manifest.id.as_str() {
            continue;
        }
        if validate_manifest_files(&directory, &loaded.manifest).is_err() {
            continue;
        }
        by_id.insert(
            loaded.manifest.id.clone(),
            ClientExtensionSummary {
                id: loaded.manifest.id,
                version: loaded.manifest.version,
                directory,
                source: source.clone(),
                enabled: false,
                manifest: loaded.raw,
            },
        );
    }
    Ok(())
}

fn read_manifest(root: &Path) -> Result<LoadedManifest> {
    let manifest_path = root.join(MANIFEST_FILENAME);
    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("missing manifest at {}", manifest_path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&contents)
        .with_context(|| format!("invalid manifest at {}", manifest_path.display()))?;
    let manifest = serde_json::from_value(raw.clone())
        .with_context(|| format!("invalid manifest at {}", manifest_path.display()))?;
    Ok(LoadedManifest { manifest, raw })
}

fn is_safe_extension_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn resolve_main_path(root: &Path, main: &str) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("extension directory not found: {}", root.display()))?;
    let candidate = root.join(main);
    let main_path = candidate
        .canonicalize()
        .with_context(|| format!("manifest main entry missing at {}", candidate.display()))?;
    if !main_path.starts_with(&root) {
        bail!("manifest main entry '{main}' escapes the extension directory");
    }
    if !main_path.is_file() {
        bail!("manifest main entry missing at {}", main_path.display());
    }
    Ok(main_path)
}

fn validate_manifest_files(root: &Path, manifest: &ClientExtensionManifest) -> Result<()> {
    if manifest.id.trim().is_empty() {
        bail!("manifest id must not be empty");
    }
    if !is_safe_extension_id(&manifest.id) {
        bail!("manifest id '{}' contains invalid characters", manifest.id);
    }
    if manifest.version.trim().is_empty() {
        bail!("manifest version must not be empty");
    }
    if manifest.main.trim().is_empty() {
        bail!("manifest main must not be empty");
    }
    resolve_main_path(root, &manifest.main)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    struct PathRootGuard(Option<std::ffi::OsString>);

    impl PathRootGuard {
        fn set(root: &Path) -> Self {
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

    fn write_extension(root: &Path, id: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join(MANIFEST_FILENAME),
            format!(r#"{{"id":"{id}","version":"0.1.0","main":"index.html"}}"#),
        )
        .unwrap();
        fs::write(root.join("index.html"), "<html></html>").unwrap();
    }

    fn find(id: &str) -> ClientExtensionSummary {
        list_client_extensions()
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == id)
            .expect("extension listed")
    }

    #[test]
    #[serial]
    fn installs_and_uninstalls_extension_from_directory() {
        let source = TempDir::new().unwrap();
        write_extension(source.path(), "demo-ext");

        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        let install = install_client_extension(source.path()).unwrap();
        assert_eq!(install.id, "demo-ext");
        assert!(install.directory.join("index.html").is_file());
        assert!(client_extensions_dir().join("demo-ext").is_dir());

        disable_client_extension("demo-ext").unwrap();
        assert!(!find("demo-ext").enabled);

        enable_client_extension("demo-ext").unwrap();
        assert!(find("demo-ext").enabled);

        uninstall_client_extension("demo-ext").unwrap();
        assert!(!client_extensions_dir().join("demo-ext").exists());
    }

    #[test]
    fn dev_extensions_require_opt_in() {
        let config = ClientExtensionsConfig::default();
        assert!(!is_client_extension_enabled(
            "hello-page",
            ClientExtensionSource::Dev,
            Some(&config)
        ));
        assert!(is_client_extension_enabled(
            "hello-page",
            ClientExtensionSource::Installed,
            Some(&config)
        ));
    }

    #[test]
    fn unreadable_config_disables_everything() {
        assert!(!is_client_extension_enabled(
            "demo-ext",
            ClientExtensionSource::Installed,
            None
        ));
    }

    #[test]
    #[serial]
    fn corrupt_config_fails_closed() {
        let source = TempDir::new().unwrap();
        write_extension(source.path(), "demo-ext");
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        install_client_extension(source.path()).unwrap();
        assert!(find("demo-ext").enabled);

        fs::write(config_path(), "{ not json").unwrap();
        assert!(load_client_extensions_config().is_none());
        assert!(!find("demo-ext").enabled);
    }

    #[test]
    #[serial]
    fn ignores_extensions_whose_directory_name_differs_from_the_id() {
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        write_extension(&client_extensions_dir().join("first"), "shared-id");
        write_extension(&client_extensions_dir().join("shared-id"), "shared-id");

        let listed: Vec<_> = list_client_extensions()
            .unwrap()
            .into_iter()
            .filter(|entry| entry.id == "shared-id")
            .collect();

        assert_eq!(listed.len(), 1);
        assert!(listed[0].directory.ends_with("shared-id"));
    }

    #[test]
    #[serial]
    fn list_exposes_the_raw_manifest() {
        let source = TempDir::new().unwrap();
        fs::create_dir_all(source.path()).unwrap();
        fs::write(
            source.path().join(MANIFEST_FILENAME),
            r#"{"id":"demo-ext","version":"0.1.0","main":"index.html","permissions":["sessions:read"],"contributes":{"rootLinks":[{"id":"home","label":"Home"}]}}"#,
        )
        .unwrap();
        fs::write(source.path().join("index.html"), "<html></html>").unwrap();
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        install_client_extension(source.path()).unwrap();

        let manifest = find("demo-ext").manifest;
        assert_eq!(manifest["permissions"][0], "sessions:read");
        assert_eq!(manifest["contributes"]["rootLinks"][0]["label"], "Home");
    }

    #[test]
    #[serial]
    fn main_entry_cannot_escape_the_extension_directory() {
        let workspace = TempDir::new().unwrap();
        fs::write(workspace.path().join("secret.html"), "secret").unwrap();
        let source = workspace.path().join("evil-ext");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join(MANIFEST_FILENAME),
            r#"{"id":"evil-ext","version":"0.1.0","main":"../secret.html"}"#,
        )
        .unwrap();
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());

        let error = install_client_extension(&source).unwrap_err().to_string();
        assert!(error.contains("escapes the extension directory"), "{error}");
    }

    #[test]
    #[serial]
    fn reads_main_only_for_enabled_extensions() {
        let source = TempDir::new().unwrap();
        write_extension(source.path(), "demo-ext");
        let root = TempDir::new().unwrap();
        let _root = PathRootGuard::set(root.path());
        install_client_extension(source.path()).unwrap();

        assert_eq!(
            read_client_extension_main("demo-ext").unwrap(),
            "<html></html>"
        );

        disable_client_extension("demo-ext").unwrap();
        let error = read_client_extension_main("demo-ext")
            .unwrap_err()
            .to_string();
        assert!(error.contains("disabled"), "{error}");

        let error = read_client_extension_main("missing")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not installed"), "{error}");
    }
}

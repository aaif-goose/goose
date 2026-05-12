use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const DISTRO_DIR_NAME: &str = "distro";
const DISTRO_JSON_NAME: &str = "distro.json";
const DISTRO_CONFIG_NAME: &str = "config.yaml";
const DISTRO_BIN_DIR_NAME: &str = "bin";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistroManifest {
    pub app_version: Option<String>,
    pub feature_toggles: Option<HashMap<String, bool>>,
    pub extension_allowlist: Option<String>,
    pub provider_allowlist: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistroBundleInfo {
    pub present: bool,
    pub app_version: Option<String>,
    pub feature_toggles: Option<HashMap<String, bool>>,
    pub extension_allowlist: Option<String>,
    pub provider_allowlist: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DistroBundle {
    pub root_dir: PathBuf,
    pub config_path: Option<PathBuf>,
    pub bin_dir: Option<PathBuf>,
    pub manifest: DistroManifest,
}

pub struct DistroBundleState {
    bundle: Option<DistroBundle>,
}

impl DistroBundleState {
    pub fn new(app_handle: &AppHandle) -> Self {
        let bundle = load_distro_bundle(app_handle)
            .map_err(|error| {
                log::warn!("Failed to load distro bundle: {error}");
                error
            })
            .ok()
            .flatten();

        Self { bundle }
    }

    pub fn info(&self) -> DistroBundleInfo {
        let Some(bundle) = &self.bundle else {
            return DistroBundleInfo {
                present: false,
                app_version: None,
                feature_toggles: None,
                extension_allowlist: None,
                provider_allowlist: None,
            };
        };

        DistroBundleInfo {
            present: true,
            app_version: bundle.manifest.app_version.clone(),
            feature_toggles: bundle.manifest.feature_toggles.clone(),
            extension_allowlist: bundle.manifest.extension_allowlist.clone(),
            provider_allowlist: bundle.manifest.provider_allowlist.clone(),
        }
    }

    pub fn bundle(&self) -> Option<&DistroBundle> {
        self.bundle.as_ref()
    }
}

fn load_distro_bundle(app_handle: &AppHandle) -> Result<Option<DistroBundle>, String> {
    let Some(root_dir) = resolve_distro_root(app_handle)? else {
        return Ok(None);
    };

    let manifest_path = root_dir.join(DISTRO_JSON_NAME);
    if !manifest_path.exists() {
        return Ok(None);
    }

    let manifest = read_manifest(&manifest_path)?;
    let config_path = root_dir.join(DISTRO_CONFIG_NAME);
    let bin_dir = root_dir.join(DISTRO_BIN_DIR_NAME);

    Ok(Some(DistroBundle {
        root_dir,
        config_path: config_path.exists().then_some(config_path),
        bin_dir: bin_dir.is_dir().then_some(bin_dir),
        manifest,
    }))
}

fn resolve_distro_root(app_handle: &AppHandle) -> Result<Option<PathBuf>, String> {
    if let Ok(override_dir) = env::var("GOOSE_DISTRO_DIR") {
        let path = PathBuf::from(override_dir);
        if path.is_dir() {
            return Ok(Some(path));
        }
        return Err(format!(
            "GOOSE_DISTRO_DIR points to a non-directory path: {}",
            path.display()
        ));
    }

    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|error| format!("Failed to resolve Tauri resource dir: {error}"))?;
    let distro_dir = resource_dir.join(DISTRO_DIR_NAME);

    Ok(distro_dir.is_dir().then_some(distro_dir))
}

fn read_manifest(path: &Path) -> Result<DistroManifest, String> {
    let contents = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "Failed to read distro manifest '{}': {error}",
            path.display()
        )
    })?;

    serde_json::from_str::<DistroManifest>(&contents).map_err(|error| {
        format!(
            "Failed to parse distro manifest '{}': {error}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_manifest() {
        let manifest = serde_json::from_str::<DistroManifest>(
            r#"{
                "appVersion": "development",
                "featureToggles": {"foo": true}
            }"#,
        )
        .expect("manifest should parse");

        assert_eq!(manifest.app_version.as_deref(), Some("development"));
        assert_eq!(
            manifest
                .feature_toggles
                .as_ref()
                .and_then(|toggles| toggles.get("foo"))
                .copied(),
            Some(true)
        );
    }
}

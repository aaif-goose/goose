use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;

use goose_agent::extension_manager::host::SpawnServerFn;

static BUILTIN_REGISTRY: Lazy<RwLock<HashMap<&'static str, SpawnServerFn>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Register a builtin extension into the global registry
pub fn register_builtin_extension(name: &'static str, spawn_fn: SpawnServerFn) {
    BUILTIN_REGISTRY.write().unwrap().insert(name, spawn_fn);
}

/// Register multiple builtin extensions from a HashMap
pub fn register_builtin_extensions(extensions: HashMap<&'static str, SpawnServerFn>) {
    let mut registry = BUILTIN_REGISTRY.write().unwrap();
    registry.extend(extensions);
}

/// Get a copy of all registered builtin extensions
pub fn get_builtin_extension(name: &str) -> Option<SpawnServerFn> {
    BUILTIN_REGISTRY.read().unwrap().get(name).cloned()
}

pub fn get_builtin_extension_names() -> Vec<&'static str> {
    BUILTIN_REGISTRY.read().unwrap().keys().copied().collect()
}

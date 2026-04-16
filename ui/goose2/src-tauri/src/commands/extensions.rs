use serde_json::Value;
use tauri::State;

use crate::services::goose_config::GooseConfig;

fn yaml_to_json(yaml: serde_yaml::Value) -> Value {
    match yaml {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        serde_yaml::Value::String(s) => Value::String(s),
        serde_yaml::Value::Sequence(seq) => {
            Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let obj = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        other => serde_yaml::to_string(&other).ok()?.trim().to_string(),
                    };
                    Some((key, yaml_to_json(v)))
                })
                .collect();
            Value::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(tagged.value),
    }
}

fn json_to_yaml(json: Value) -> serde_yaml::Value {
    match json {
        Value::Null => serde_yaml::Value::Null,
        Value::Bool(b) => serde_yaml::Value::Bool(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_yaml::Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                serde_yaml::Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_yaml::Value::Number(f.into())
            } else {
                serde_yaml::Value::Null
            }
        }
        Value::String(s) => serde_yaml::Value::String(s),
        Value::Array(arr) => {
            serde_yaml::Value::Sequence(arr.into_iter().map(json_to_yaml).collect())
        }
        Value::Object(obj) => {
            let mut map = serde_yaml::Mapping::new();
            for (k, v) in obj {
                map.insert(serde_yaml::Value::String(k), json_to_yaml(v));
            }
            serde_yaml::Value::Mapping(map)
        }
    }
}

fn name_to_key(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => result.push(c),
            c if c.is_whitespace() => continue,
            _ => result.push('_'),
        }
    }
    result.to_lowercase()
}

#[tauri::command]
pub fn list_extensions(config: State<'_, GooseConfig>) -> Result<Vec<Value>, String> {
    let raw = config.get_extensions_raw();
    let mut entries = Vec::with_capacity(raw.len());

    for (k, v) in raw {
        let key = match k {
            serde_yaml::Value::String(s) => s,
            _ => continue,
        };

        let mut json = yaml_to_json(v);

        if let Value::Object(ref mut obj) = json {
            if !obj.contains_key("type") {
                continue;
            }
            obj.insert("config_key".to_string(), Value::String(key.clone()));
            obj.entry("name".to_string())
                .or_insert_with(|| Value::String(key));
            obj.entry("enabled".to_string())
                .or_insert(Value::Bool(false));
            entries.push(json);
        }
    }

    Ok(entries)
}

#[tauri::command]
pub fn add_extension(
    name: String,
    extension_config: Value,
    enabled: bool,
    config: State<'_, GooseConfig>,
) -> Result<(), String> {
    let key = name_to_key(&name);
    let mut raw = config.get_extensions_raw();

    let mut entry = match extension_config {
        Value::Object(obj) => obj,
        _ => return Err("extension_config must be a JSON object".to_string()),
    };

    entry.insert("enabled".to_string(), Value::Bool(enabled));
    entry.insert("name".to_string(), Value::String(name));

    let yaml_value = json_to_yaml(Value::Object(entry));
    raw.insert(serde_yaml::Value::String(key), yaml_value);

    config.set_extensions_raw(raw)
}

#[tauri::command]
pub fn remove_extension(config_key: String, config: State<'_, GooseConfig>) -> Result<(), String> {
    let mut raw = config.get_extensions_raw();
    let yaml_key = serde_yaml::Value::String(config_key.clone());
    if raw.remove(&yaml_key).is_none() {
        return Err(format!("Extension '{}' not found", config_key));
    }
    config.set_extensions_raw(raw)
}

#[tauri::command]
pub fn toggle_extension(
    config_key: String,
    enabled: bool,
    config: State<'_, GooseConfig>,
) -> Result<(), String> {
    let mut raw = config.get_extensions_raw();

    let yaml_key = serde_yaml::Value::String(config_key.clone());
    if let Some(entry) = raw.get_mut(&yaml_key) {
        if let serde_yaml::Value::Mapping(ref mut map) = entry {
            map.insert(
                serde_yaml::Value::String("enabled".to_string()),
                serde_yaml::Value::Bool(enabled),
            );
        }
        config.set_extensions_raw(raw)
    } else {
        Err(format!("Extension '{}' not found", config_key))
    }
}

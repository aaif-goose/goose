use goose::config::GooseConfigSchema;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let schema = schemars::schema_for!(GooseConfigSchema);
    let json_str = serde_json::to_string_pretty(&schema).expect("failed to serialize schema");

    let package_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let schema_path = PathBuf::from(&package_dir).join("config.schema.json");

    let check_mode = env::args().any(|arg| arg == "--check");

    if check_mode {
        let existing = fs::read_to_string(&schema_path).unwrap_or_default();
        if existing.trim() != json_str.trim() {
            eprintln!(
                "Config schema is out of date. Run `cargo run -p goose --bin generate-config-schema` to regenerate."
            );
            std::process::exit(1);
        }
        eprintln!("Config schema is up to date.");
    } else {
        fs::write(&schema_path, format!("{json_str}\n")).expect("failed to write schema file");
        eprintln!("Generated config schema at {}", schema_path.display());
    }
}

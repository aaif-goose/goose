const COMMANDS: &[&str] = &["driver_result"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}

mod commands;
mod services;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::acp::get_goose_serve_url,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::folders::list_memory_notes,
            commands::folders::read_note,
            commands::folders::list_projects,
            commands::folders::list_project_notes,
            commands::recipes::list_recipes,
            commands::recipes::load_recipe_prompt,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

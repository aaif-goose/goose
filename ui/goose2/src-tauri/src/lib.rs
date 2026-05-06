mod commands;
mod services;
mod types;

use services::distro_bundle::DistroBundleState;
use services::personas::PersonaStore;
use tauri::Manager;
use tauri_plugin_window_state::StateFlags;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .targets([tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                )])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .manage(PersonaStore::new());

    #[cfg(feature = "app-test-driver")]
    let builder = builder.plugin(tauri_plugin_app_test_driver::init());

    builder
        .setup(|app| {
            app.manage(DistroBundleState::new(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agents::list_personas,
            commands::agents::create_persona,
            commands::agents::update_persona,
            commands::agents::delete_persona,
            commands::agents::refresh_personas,
            commands::agents::export_persona,
            commands::agents::import_personas,
            commands::agents::read_import_persona_file,
            commands::agents::save_persona_avatar,
            commands::agents::save_persona_avatar_bytes,
            commands::agents::get_avatars_dir,
            commands::acp::get_goose_serve_url,
            commands::acp::get_goose_serve_host_info,
            commands::project_icons::scan_project_icons,
            commands::project_icons::read_project_icon,
            commands::doctor::run_doctor,
            commands::doctor::run_doctor_fix,
            commands::git::get_git_state,
            commands::git_changes::get_changed_files,
            commands::git::git_switch_branch,
            commands::git::git_stash,
            commands::git::git_init,
            commands::git::git_fetch,
            commands::git::git_pull,
            commands::git::git_create_branch,
            commands::git::git_create_worktree,
            commands::model_setup::authenticate_model_provider,
            commands::agent_setup::check_agent_installed,
            commands::agent_setup::check_agent_auth,
            commands::agent_setup::install_agent,
            commands::agent_setup::authenticate_agent,
            commands::path_resolver::resolve_path,
            commands::distro::get_distro_bundle,
            commands::system::get_home_dir,
            commands::system::save_exported_session_file,
            commands::system::path_exists,
            commands::system::list_directory_entries,
            commands::system::inspect_attachment_paths,
            commands::system::list_files_for_mentions,
            commands::system::read_image_attachment,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {});
}

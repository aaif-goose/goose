mod commands;
mod services;
mod types;

use std::sync::Arc;

use services::acp::AcpSessionRegistry;
use services::goose_config::GooseConfig;
use services::personas::PersonaStore;
use tauri_plugin_window_state::StateFlags;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let acp_registry = Arc::new(AcpSessionRegistry::new());
    let acp_registry_for_exit = Arc::clone(&acp_registry);

    let builder = tauri::Builder::default()
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
        .manage(PersonaStore::new())
        .manage(GooseConfig::new())
        .manage(acp_registry);

    #[cfg(feature = "app-test-driver")]
    let builder = builder.plugin(tauri_plugin_app_test_driver::init());

    builder
        .invoke_handler(tauri::generate_handler![
            commands::agents::list_personas,
            commands::agents::create_persona,
            commands::agents::update_persona,
            commands::agents::delete_persona,
            commands::agents::refresh_personas,
            commands::agents::export_persona,
            commands::agents::import_personas,
            commands::agents::save_persona_avatar,
            commands::agents::save_persona_avatar_bytes,
            commands::agents::get_avatars_dir,
            commands::acp::discover_acp_providers,
            commands::acp::acp_prepare_session,
            commands::acp::acp_set_model,
            commands::acp::acp_send_message,
            commands::acp::acp_cancel_session,
            commands::acp::acp_list_sessions,
            commands::acp::acp_search_sessions,
            commands::acp::acp_load_session,
            commands::acp::acp_list_running,
            commands::acp::acp_cancel_all,
            commands::acp::acp_export_session,
            commands::acp::acp_import_session,
            commands::acp::acp_duplicate_session,
            commands::skills::create_skill,
            commands::skills::list_skills,
            commands::skills::delete_skill,
            commands::skills::update_skill,
            commands::skills::export_skill,
            commands::skills::import_skills,
            commands::projects::list_projects,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::delete_project,
            commands::projects::get_project,
            commands::projects::list_archived_projects,
            commands::projects::archive_project,
            commands::projects::restore_project,
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
            commands::credentials::get_provider_config,
            commands::credentials::save_provider_field,
            commands::credentials::delete_provider_config,
            commands::credentials::check_all_provider_status,
            commands::credentials::restart_app,
            commands::model_setup::authenticate_model_provider,
            commands::agent_setup::check_agent_installed,
            commands::agent_setup::check_agent_auth,
            commands::agent_setup::install_agent,
            commands::agent_setup::authenticate_agent,
            commands::system::get_home_dir,
            commands::system::save_exported_session_file,
            commands::system::path_exists,
            commands::system::list_directory_entries,
            commands::system::inspect_attachment_paths,
            commands::system::list_files_for_mentions,
            commands::system::read_image_attachment,
        ])
        .setup(|_app| Ok(()))
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app, event| {
            if let tauri::RunEvent::Exit = event {
                acp_registry_for_exit.cancel_all();
            }
        });
}

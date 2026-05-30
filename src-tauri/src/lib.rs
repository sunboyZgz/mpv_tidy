mod commands;
mod crf;
mod domain;
mod error;
mod library;
mod matcher;
mod mpv;
mod organizer;
mod parser;
mod scanner;
mod settings;
mod training;

pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(mpv::MpvController::default())
        .setup(|app| {
            if cfg!(debug_assertions) {
                if let Err(error) = commands::print_settings_storage_paths(app.handle()) {
                    eprintln!("Failed to print storage paths: {error}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_and_match,
            commands::build_organize_plan,
            commands::execute_organize_plan,
            commands::launch_mpv,
            commands::save_project_config,
            commands::reveal_path,
            commands::save_local_library_entry,
            commands::load_local_library,
            commands::remove_local_library_entry,
            commands::repair_library_entry_paths,
            commands::update_library_episode_progress,
            commands::scan_embedded_subtitle_tracks,
            commands::extract_parse_token_features,
            commands::save_parse_training_sample,
            commands::settings_storage_paths,
            commands::load_app_settings,
            commands::save_app_settings,
            commands::reset_app_settings
        ])
        .run(tauri::generate_context!())
}

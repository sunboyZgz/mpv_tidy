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
mod training;

pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::scan_and_match,
            commands::build_organize_plan,
            commands::execute_organize_plan,
            commands::launch_mpv,
            commands::save_project_config,
            commands::reveal_path,
            commands::save_local_library_entry,
            commands::load_local_library,
            commands::extract_parse_token_features,
            commands::save_parse_training_sample
        ])
        .run(tauri::generate_context!())
}

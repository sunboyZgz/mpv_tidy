use crate::domain::{
    BuildOrganizePlanRequest, MpvLaunchRequest, OrganizeExecutionResult, OrganizePlan,
    ProjectConfig, SaveLocalLibraryRequest, ScanAndMatchResult, ScanInput,
};
use crate::error::to_user_error;
use crate::{library, matcher, mpv, organizer, scanner};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
pub fn scan_and_match(input: ScanInput) -> Result<ScanAndMatchResult, String> {
    scanner::scan(&input)
        .map(matcher::match_scan)
        .map_err(to_user_error)
}

#[tauri::command]
pub fn build_organize_plan(request: BuildOrganizePlanRequest) -> Result<OrganizePlan, String> {
    organizer::build_plan(request).map_err(to_user_error)
}

#[tauri::command]
pub fn execute_organize_plan(plan: OrganizePlan) -> Result<OrganizeExecutionResult, String> {
    organizer::execute_plan(plan).map_err(to_user_error)
}

#[tauri::command]
pub fn launch_mpv(request: MpvLaunchRequest) -> Result<crate::domain::MpvLaunchResult, String> {
    mpv::launch(request).map_err(to_user_error)
}

#[tauri::command]
pub fn save_project_config(path: PathBuf, config: ProjectConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reveal_path(path: PathBuf) -> Result<(), String> {
    mpv::reveal(&path).map_err(to_user_error)
}

#[tauri::command]
pub fn save_local_library_entry(
    app: tauri::AppHandle,
    request: SaveLocalLibraryRequest,
) -> Result<crate::domain::LocalAnimeLibraryEntry, String> {
    let library_path = local_library_path(&app).map_err(to_user_error)?;
    library::save_local_library_entry(&library_path, request).map_err(to_user_error)
}

#[tauri::command]
pub fn load_local_library(
    app: tauri::AppHandle,
) -> Result<crate::domain::LocalAnimeLibraryFile, String> {
    let library_path = local_library_path(&app).map_err(to_user_error)?;
    library::load_local_library(&library_path).map_err(to_user_error)
}

fn local_library_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::LibrarySave(error.to_string()))?;
    Ok(data_dir.join("anime-library.json"))
}

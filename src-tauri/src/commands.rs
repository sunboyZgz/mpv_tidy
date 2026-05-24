use crate::domain::{
    BuildOrganizePlanRequest, MpvLaunchRequest, OrganizeExecutionResult, OrganizePlan,
    ParseTrainingSample, ProjectConfig, SaveLocalLibraryRequest, SaveParseTrainingSampleRequest,
    ScanAndMatchResult, ScanInput, TokenFeatures,
};
use crate::error::to_user_error;
use crate::{crf::CrfSlotTagger, library, matcher, mpv, organizer, scanner, training};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
pub fn scan_and_match(
    app: tauri::AppHandle,
    input: ScanInput,
) -> Result<ScanAndMatchResult, String> {
    let crf_model_path = parse_crf_model_path(&app).map_err(to_user_error)?;
    let crf_tagger = CrfSlotTagger::load_optional(&crf_model_path).map_err(to_user_error)?;
    let scan_result = match crf_tagger.as_ref() {
        Some(tagger) => scanner::scan_with_crf(&input, Some(tagger)),
        None => scanner::scan(&input),
    };
    scan_result.map(matcher::match_scan).map_err(to_user_error)
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

#[tauri::command]
pub fn extract_parse_token_features(path: PathBuf) -> Result<Vec<TokenFeatures>, String> {
    Ok(crate::parser::token_features_for_path(&path))
}

#[tauri::command]
pub fn save_parse_training_sample(
    app: tauri::AppHandle,
    request: SaveParseTrainingSampleRequest,
) -> Result<ParseTrainingSample, String> {
    let training_path = parse_training_path(&app).map_err(to_user_error)?;
    training::save_parse_training_sample(&training_path, request).map_err(to_user_error)
}

fn local_library_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::LibrarySave(error.to_string()))?;
    Ok(data_dir.join("anime-library.json"))
}

fn parse_training_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::LibrarySave(error.to_string()))?;
    Ok(data_dir.join("parser-training-samples.jsonl"))
}

fn parse_crf_model_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::LibrarySave(error.to_string()))?;
    Ok(data_dir.join("parser-crf-model.json"))
}

use crate::domain::{
    AppSettings, BuildOrganizePlanRequest, MpvLaunchRequest, OrganizeExecutionResult, OrganizePlan,
    ParseTrainingSample, ProjectConfig, RemoveLocalLibraryEntryRequest,
    RepairLibraryEntryPathsRequest, RepairLibraryEntryPathsResult, SaveLocalLibraryRequest,
    SaveParseTrainingSampleRequest, ScanAndMatchResult, ScanEmbeddedSubtitleTracksRequest,
    ScanEmbeddedSubtitleTracksResult, ScanInput, SettingsStoragePaths, SubtitlePreferenceSnapshot,
    TokenFeatures, UpdateLibraryEpisodeProgressRequest,
};
use crate::error::{to_user_error, AppError};
use crate::{crf::CrfSlotTagger, library, matcher, mpv, organizer, scanner, settings, training};
use std::fs;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

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
pub async fn execute_organize_plan(
    app: tauri::AppHandle,
    plan: OrganizePlan,
) -> Result<OrganizeExecutionResult, String> {
    let handle = tauri::async_runtime::spawn_blocking(move || {
        organizer::execute_plan_with_progress(plan, |event| {
            app.emit("organize-progress", event)
                .map_err(|error| AppError::UiEvent(error.to_string()))
        })
    });

    match handle.await {
        Ok(result) => result.map_err(to_user_error),
        Err(error) => Err(to_user_error(AppError::LibrarySave(error.to_string()))),
    }
}

#[tauri::command]
pub fn launch_mpv(
    controller: tauri::State<'_, mpv::MpvController>,
    request: MpvLaunchRequest,
) -> Result<crate::domain::MpvLaunchResult, String> {
    mpv::launch(&controller, request).map_err(to_user_error)
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
    mut request: SaveLocalLibraryRequest,
) -> Result<crate::domain::LocalAnimeLibraryEntry, String> {
    if request.subtitle_preference_snapshot.is_none() || request.cover_strategy_snapshot.is_none() {
        let data_dir = app_data_dir(&app).map_err(to_user_error)?;
        let settings_path = app_settings_path(&app).map_err(to_user_error)?;
        let app_settings =
            settings::load_app_settings(&settings_path, &data_dir).map_err(to_user_error)?;
        if request.subtitle_preference_snapshot.is_none() {
            request.subtitle_preference_snapshot = Some(SubtitlePreferenceSnapshot {
                primary_language: app_settings.default_primary_subtitle_language,
                secondary_language: Some(app_settings.default_secondary_subtitle_language),
            });
        }
        if request.cover_strategy_snapshot.is_none() {
            request.cover_strategy_snapshot = Some(app_settings.default_cover_strategy);
        }
    }
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
pub fn remove_local_library_entry(
    app: tauri::AppHandle,
    request: RemoveLocalLibraryEntryRequest,
) -> Result<crate::domain::LocalAnimeLibraryFile, String> {
    let library_path = local_library_path(&app).map_err(to_user_error)?;
    library::remove_local_library_entry(&library_path, request).map_err(to_user_error)
}

#[tauri::command]
pub fn repair_library_entry_paths(
    app: tauri::AppHandle,
    request: RepairLibraryEntryPathsRequest,
) -> Result<RepairLibraryEntryPathsResult, String> {
    let library_path = local_library_path(&app).map_err(to_user_error)?;
    library::repair_library_entry_paths(&library_path, request).map_err(to_user_error)
}

#[tauri::command]
pub fn update_library_episode_progress(
    app: tauri::AppHandle,
    request: UpdateLibraryEpisodeProgressRequest,
) -> Result<crate::domain::LocalAnimeLibraryEntry, String> {
    let library_path = local_library_path(&app).map_err(to_user_error)?;
    library::update_episode_progress(&library_path, request).map_err(to_user_error)
}

#[tauri::command]
pub async fn scan_embedded_subtitle_tracks(
    app: tauri::AppHandle,
    request: ScanEmbeddedSubtitleTracksRequest,
) -> Result<ScanEmbeddedSubtitleTracksResult, String> {
    let data_dir = app_data_dir(&app).map_err(to_user_error)?;
    let settings_path = app_settings_path(&app).map_err(to_user_error)?;
    let app_settings =
        settings::load_app_settings(&settings_path, &data_dir).map_err(to_user_error)?;
    let library_path = local_library_path(&app).map_err(to_user_error)?;
    let mpv_path = app_settings.mpv_executable_path;
    let handle = tauri::async_runtime::spawn_blocking(move || {
        library::scan_embedded_subtitle_tracks(&library_path, &mpv_path, request)
    });

    match handle.await {
        Ok(result) => result.map_err(to_user_error),
        Err(error) => Err(to_user_error(AppError::LibrarySave(error.to_string()))),
    }
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

#[tauri::command]
pub fn settings_storage_paths(app: tauri::AppHandle) -> Result<SettingsStoragePaths, String> {
    storage_paths(&app).map_err(to_user_error)
}

pub(crate) fn print_settings_storage_paths(app: &tauri::AppHandle) -> Result<(), AppError> {
    let paths = storage_paths(app)?;
    println!("========== Anime Subtitle Manager storage paths ==========");
    println!("training_data_dir = {}", paths.training_data_dir.display());
    println!(
        "training_sample_file = {}",
        paths.training_sample_file.display()
    );
    println!("crf_model_file = {}", paths.crf_model_file.display());
    println!("app_settings_file = {}", paths.app_settings_file.display());
    println!(
        "local_library_file = {}",
        paths.local_library_file.display()
    );
    println!("==========================================================");
    Ok(())
}

fn storage_paths(app: &tauri::AppHandle) -> Result<SettingsStoragePaths, AppError> {
    let data_dir = app_data_dir(app)?;
    Ok(SettingsStoragePaths {
        training_data_dir: data_dir.clone(),
        training_sample_file: data_dir.join("parser-training-samples.jsonl"),
        crf_model_file: data_dir.join("parser-crf-model.json"),
        app_settings_file: data_dir.join("app-settings.json"),
        local_library_file: data_dir.join("anime-library.json"),
    })
}

#[tauri::command]
pub fn load_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let data_dir = app_data_dir(&app).map_err(to_user_error)?;
    let settings_path = app_settings_path(&app).map_err(to_user_error)?;
    settings::load_app_settings(&settings_path, &data_dir).map_err(to_user_error)
}

#[tauri::command]
pub fn save_app_settings(
    app: tauri::AppHandle,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let settings_path = app_settings_path(&app).map_err(to_user_error)?;
    settings::save_app_settings(&settings_path, settings).map_err(to_user_error)
}

#[tauri::command]
pub fn reset_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let data_dir = app_data_dir(&app).map_err(to_user_error)?;
    let settings_path = app_settings_path(&app).map_err(to_user_error)?;
    settings::reset_app_settings(&settings_path, &data_dir).map_err(to_user_error)
}

fn local_library_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app_data_dir(app)?;
    Ok(data_dir.join("anime-library.json"))
}

fn parse_training_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app_data_dir(app)?;
    Ok(data_dir.join("parser-training-samples.jsonl"))
}

fn parse_crf_model_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app_data_dir(app)?;
    Ok(data_dir.join("parser-crf-model.json"))
}

fn app_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    let data_dir = app_data_dir(app)?;
    Ok(data_dir.join("app-settings.json"))
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, crate::error::AppError> {
    app.path()
        .app_data_dir()
        .map_err(|error| crate::error::AppError::LibrarySave(error.to_string()))
}

use crate::domain::{AppSettings, CoverStrategy, LanguageCode};
use crate::error::{AppError, AppResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: u16 = 1;

pub fn load_app_settings(settings_path: &Path, data_dir: &Path) -> AppResult<AppSettings> {
    if !settings_path.exists() {
        return Ok(default_app_settings(data_dir));
    }

    let text = fs::read_to_string(settings_path)?;
    serde_json::from_str::<AppSettings>(&text)
        .map_err(|error| AppError::LibrarySave(error.to_string()))
}

pub fn save_app_settings(
    settings_path: &Path,
    mut settings: AppSettings,
) -> AppResult<AppSettings> {
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    settings.schema_version = SCHEMA_VERSION;
    settings.updated_at_unix = now_unix();
    let payload = serde_json::to_string_pretty(&settings)?;
    fs::write(settings_path, payload)?;
    Ok(settings)
}

pub fn reset_app_settings(settings_path: &Path, data_dir: &Path) -> AppResult<AppSettings> {
    save_app_settings(settings_path, default_app_settings(data_dir))
}

pub fn default_app_settings(data_dir: &Path) -> AppSettings {
    AppSettings {
        schema_version: SCHEMA_VERSION,
        mpv_executable_path: PathBuf::from("mpv"),
        default_output_dir: PathBuf::from("D:\\整理输出"),
        anime_library_root_dir: PathBuf::from("D:\\AnimeLibrary"),
        temp_dir: data_dir.join("temp"),
        default_primary_subtitle_language: LanguageCode::ZhHans,
        default_secondary_subtitle_language: LanguageCode::Ja,
        remember_playback_progress: true,
        auto_scan_anime_library_on_startup: true,
        auto_save_watch_progress: true,
        default_cover_strategy: CoverStrategy::LocalFirstThenScreenshot,
        updated_at_unix: now_unix(),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{load_app_settings, save_app_settings};
    use std::error::Error;
    use tempfile::tempdir;

    #[test]
    fn returns_defaults_when_settings_file_is_missing() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let loaded = load_app_settings(&temp.path().join("missing.json"), temp.path())?;

        assert_eq!(loaded.mpv_executable_path, std::path::PathBuf::from("mpv"));
        assert!(loaded.auto_scan_anime_library_on_startup);
        Ok(())
    }

    #[test]
    fn saves_and_loads_settings_json() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let settings_path = temp.path().join("app-settings.json");
        let mut settings = load_app_settings(&settings_path, temp.path())?;
        settings.mpv_executable_path = std::path::PathBuf::from("C:\\Tools\\mpv.exe");

        let saved = save_app_settings(&settings_path, settings)?;
        let loaded = load_app_settings(&settings_path, temp.path())?;

        assert_eq!(loaded.mpv_executable_path, saved.mpv_executable_path);
        assert_eq!(loaded.schema_version, 1);
        Ok(())
    }
}

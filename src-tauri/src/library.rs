use crate::domain::{
    LibraryEpisodeRecord, LocalAnimeLibraryEntry, LocalAnimeLibraryFile,
    RemoveLocalLibraryEntryRequest, SaveLocalLibraryRequest, UpdateLibraryEpisodeProgressRequest,
    WatchStatus,
};
use crate::error::{AppError, AppResult};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_VERSION: &str = "0.1.0";

pub fn save_local_library_entry(
    library_path: &Path,
    request: SaveLocalLibraryRequest,
) -> AppResult<LocalAnimeLibraryEntry> {
    if !request.output_dir.exists() {
        fs::create_dir_all(&request.output_dir)?;
    }
    if let Some(parent) = library_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = load_local_library(library_path)?;
    let now = now_unix();
    let id = library_entry_id(&request.project_name, &request.season, &request.output_dir);
    let existing_created_at = file
        .entries
        .iter()
        .find(|existing| {
            existing.id == id
                || (existing.project_name == request.project_name
                    && existing.season == request.season
                    && existing.output_dir == request.output_dir)
        })
        .map(|existing| existing.created_at_unix)
        .filter(|value| *value > 0)
        .unwrap_or(now);

    let entry = LocalAnimeLibraryEntry {
        id,
        project_name: request.project_name,
        season: request.season,
        output_dir: request.output_dir,
        mode: request.mode,
        episode_count: request.episodes.len(),
        subtitle_preference_snapshot: request.subtitle_preference_snapshot,
        cover_strategy_snapshot: request.cover_strategy_snapshot,
        episodes: request
            .episodes
            .into_iter()
            .map(|episode| normalize_episode_record(episode, now))
            .collect(),
        created_at_unix: existing_created_at,
        updated_at_unix: now,
        organized_at_unix: now,
    };

    file.entries.retain(|existing| {
        !(existing.id == entry.id
            || (existing.project_name == entry.project_name
                && existing.season == entry.season
                && existing.output_dir == entry.output_dir))
    });
    file.entries.push(entry);
    normalize_library_file(&mut file);
    let payload = serde_json::to_string_pretty(&file)?;
    fs::write(library_path, payload)?;
    match file.entries.last() {
        Some(saved) => Ok(saved.to_owned()),
        None => Err(AppError::LibrarySave("未能写入本地动漫库条目".to_owned())),
    }
}

pub fn load_local_library(library_path: &Path) -> AppResult<LocalAnimeLibraryFile> {
    if !library_path.exists() {
        return Ok(empty_library_file());
    }
    let text = fs::read_to_string(library_path)?;
    let mut file = serde_json::from_str::<LocalAnimeLibraryFile>(&text)
        .map_err(|error| AppError::LibrarySave(error.to_string()))?;
    normalize_library_file(&mut file);
    Ok(file)
}

pub fn remove_local_library_entry(
    library_path: &Path,
    request: RemoveLocalLibraryEntryRequest,
) -> AppResult<LocalAnimeLibraryFile> {
    let mut file = load_local_library(library_path)?;
    let before_count = file.entries.len();
    file.entries.retain(|entry| entry.id != request.entry_id);
    if file.entries.len() == before_count {
        return Err(AppError::LibrarySave("未找到本地动漫库条目".to_owned()));
    }

    normalize_library_file(&mut file);
    let payload = serde_json::to_string_pretty(&file)?;
    if let Some(parent) = library_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(library_path, payload)?;
    Ok(file)
}

pub fn update_episode_progress(
    library_path: &Path,
    request: UpdateLibraryEpisodeProgressRequest,
) -> AppResult<LocalAnimeLibraryEntry> {
    let mut file = load_local_library(library_path)?;
    let now = now_unix();
    let Some(entry) = file
        .entries
        .iter_mut()
        .find(|entry| entry.id == request.entry_id)
    else {
        return Err(AppError::LibrarySave("未找到本地动漫库条目".to_owned()));
    };
    let Some(episode) = entry
        .episodes
        .iter_mut()
        .find(|episode| episode.episode_key == request.episode_key)
    else {
        return Err(AppError::LibrarySave("未找到本地动漫库剧集".to_owned()));
    };

    episode.watch_status = request.watch_status;
    episode.last_position_sec = request.last_position_sec;
    episode.progress_percent = request.progress_percent;
    episode.updated_at_unix = now;
    entry.updated_at_unix = now;
    let updated = entry.to_owned();

    let payload = serde_json::to_string_pretty(&file)?;
    if let Some(parent) = library_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(library_path, payload)?;
    Ok(updated)
}

fn empty_library_file() -> LocalAnimeLibraryFile {
    LocalAnimeLibraryFile {
        schema_version: 1,
        app_version: APP_VERSION.to_owned(),
        entries: Vec::new(),
    }
}

fn normalize_library_file(file: &mut LocalAnimeLibraryFile) {
    file.schema_version = 1;
    let now = now_unix();
    for entry in &mut file.entries {
        if entry.id.trim().is_empty() {
            entry.id = library_entry_id(&entry.project_name, &entry.season, &entry.output_dir);
        }
        if entry.created_at_unix == 0 {
            entry.created_at_unix = if entry.organized_at_unix > 0 {
                entry.organized_at_unix
            } else {
                now
            };
        }
        if entry.updated_at_unix == 0 {
            entry.updated_at_unix = if entry.organized_at_unix > 0 {
                entry.organized_at_unix
            } else {
                entry.created_at_unix
            };
        }
        if entry.organized_at_unix == 0 {
            entry.organized_at_unix = entry.created_at_unix;
        }
        entry.episode_count = entry.episodes.len();
        entry.episodes = entry
            .episodes
            .drain(..)
            .map(|episode| normalize_episode_record(episode, entry.updated_at_unix))
            .collect();
    }
}

fn normalize_episode_record(
    mut episode: LibraryEpisodeRecord,
    default_updated_at: u64,
) -> LibraryEpisodeRecord {
    if episode.updated_at_unix == 0 {
        episode.updated_at_unix = default_updated_at;
    }
    if episode.progress_percent.is_some_and(|value| value > 100) {
        episode.progress_percent = Some(100);
    }
    if episode.video_path.is_none()
        && episode.primary_subtitle_path.is_none()
        && episode.secondary_subtitle_path.is_none()
    {
        episode.watch_status = WatchStatus::Unwatched;
    }
    episode
}

fn library_entry_id(project_name: &str, season: &str, output_dir: &Path) -> String {
    let raw = format!("{}-{}-{}", project_name, season, output_dir.display());
    let normalized = raw
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let collapsed = normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        format!("library-{}", now_unix())
    } else {
        collapsed
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
    use super::{
        load_local_library, remove_local_library_entry, save_local_library_entry,
        update_episode_progress,
    };
    use crate::domain::{
        LibraryEpisodeRecord, MatchStatus, OrganizeMode, RemoveLocalLibraryEntryRequest,
        SaveLocalLibraryRequest, UpdateLibraryEpisodeProgressRequest, WatchStatus,
    };
    use std::error::Error;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn saves_and_replaces_library_entry() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let library_path = temp.path().join("app-data").join("anime-library.json");
        let first_request = request(temp.path().join("out"));

        let saved_first = save_local_library_entry(&library_path, first_request)?;
        assert_eq!(saved_first.project_name, "Jujutsu Kaisen");

        let content = fs::read_to_string(&library_path)?;
        assert!(content.contains("Jujutsu Kaisen"));
        assert!(content.contains("\"schemaVersion\""));

        let saved_second =
            save_local_library_entry(&library_path, request(temp.path().join("out")))?;
        assert_eq!(saved_second.episode_count, 1);
        assert!(!saved_second.id.is_empty());
        Ok(())
    }

    #[test]
    fn loads_empty_library_when_file_is_missing() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let loaded = load_local_library(&temp.path().join("missing").join("anime-library.json"))?;
        assert!(loaded.entries.is_empty());
        Ok(())
    }

    #[test]
    fn migrates_old_library_entries_with_defaults() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let library_path = temp.path().join("anime-library.json");
        fs::write(
            &library_path,
            r#"{
                "appVersion": "0.1.0",
                "entries": [{
                    "projectName": "Old Show",
                    "season": "S01",
                    "outputDir": "D:/Old Show",
                    "mode": "copy",
                    "episodeCount": 1,
                    "episodes": [{
                        "episodeKey": "S01E01",
                        "videoPath": null,
                        "primarySubtitlePath": null,
                        "secondarySubtitlePath": null,
                        "subtitleCount": 0,
                        "status": "missingSub"
                    }],
                    "organizedAtUnix": 10
                }]
            }"#,
        )?;

        let loaded = load_local_library(&library_path)?;

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.entries[0].created_at_unix, 10);
        assert_eq!(
            loaded.entries[0].episodes[0].watch_status,
            WatchStatus::Unwatched
        );
        Ok(())
    }

    #[test]
    fn updates_episode_progress_in_library_file() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let library_path = temp.path().join("anime-library.json");
        let saved = save_local_library_entry(&library_path, request(temp.path().join("out")))?;

        let updated = update_episode_progress(
            &library_path,
            UpdateLibraryEpisodeProgressRequest {
                entry_id: saved.id,
                episode_key: "S01E01".to_owned(),
                watch_status: WatchStatus::Partial,
                last_position_sec: Some(120),
                progress_percent: Some(12),
            },
        )?;

        assert_eq!(updated.episodes[0].watch_status, WatchStatus::Partial);
        assert_eq!(updated.episodes[0].last_position_sec, Some(120));
        Ok(())
    }

    #[test]
    fn removes_library_entry_without_touching_files() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let library_path = temp.path().join("anime-library.json");
        let output_dir = temp.path().join("out");
        fs::create_dir_all(&output_dir)?;
        let media_file = output_dir.join("S01E01.mkv");
        fs::write(&media_file, "video")?;
        let saved = save_local_library_entry(&library_path, request(output_dir))?;

        let updated = remove_local_library_entry(
            &library_path,
            RemoveLocalLibraryEntryRequest { entry_id: saved.id },
        )?;

        assert!(updated.entries.is_empty());
        assert!(media_file.exists());
        Ok(())
    }

    fn request(output_dir: std::path::PathBuf) -> SaveLocalLibraryRequest {
        SaveLocalLibraryRequest {
            project_name: "Jujutsu Kaisen".to_owned(),
            season: "S01".to_owned(),
            output_dir,
            mode: OrganizeMode::Copy,
            subtitle_preference_snapshot: None,
            cover_strategy_snapshot: None,
            episodes: vec![LibraryEpisodeRecord {
                episode_key: "S01E01".to_owned(),
                video_path: None,
                primary_subtitle_path: None,
                secondary_subtitle_path: None,
                subtitle_count: 2,
                status: MatchStatus::Matched,
                watch_status: WatchStatus::Unwatched,
                last_position_sec: None,
                progress_percent: None,
                updated_at_unix: 0,
            }],
        }
    }
}

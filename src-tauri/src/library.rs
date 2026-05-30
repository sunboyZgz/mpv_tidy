use crate::domain::{
    AnimeSubMap, EmbeddedSubtitleScanFailure, LibraryEpisodeRecord, LocalAnimeLibraryEntry,
    LocalAnimeLibraryFile, RemoveLocalLibraryEntryRequest, RepairLibraryEntryPathsRequest,
    RepairLibraryEntryPathsResult, SaveLocalLibraryRequest, ScanEmbeddedSubtitleTracksRequest,
    ScanEmbeddedSubtitleTracksResult, UpdateLibraryEpisodeProgressRequest, WatchStatus,
};
use crate::error::{AppError, AppResult};
use crate::mpv;
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

pub fn repair_library_entry_paths(
    library_path: &Path,
    request: RepairLibraryEntryPathsRequest,
) -> AppResult<RepairLibraryEntryPathsResult> {
    let mut file = load_local_library(library_path)?;
    let now = now_unix();
    let Some(entry_index) = file
        .entries
        .iter()
        .position(|entry| entry.id == request.entry_id)
    else {
        return Err(AppError::LibrarySave("未找到本地动漫库条目".to_owned()));
    };

    let entry_output_dir = file.entries[entry_index].output_dir.to_path_buf();
    let map_file_path = entry_output_dir.join("anime-sub-map.json");
    if !map_file_path.is_file() {
        return Err(AppError::MissingFile(map_file_path));
    }

    let text = fs::read_to_string(&map_file_path)?;
    let project_map = serde_json::from_str::<AnimeSubMap>(&text)
        .map_err(|error| AppError::LibrarySave(error.to_string()))?;
    let base_dir = if project_map.output_dir.is_dir() {
        project_map.output_dir.to_path_buf()
    } else {
        entry_output_dir
    };

    let mut repaired_episode_count = 0usize;
    let mut missing_episode_count = 0usize;

    {
        let entry = &mut file.entries[entry_index];
        for episode in &mut entry.episodes {
            let Some(map_episode) = project_map
                .episodes
                .iter()
                .find(|candidate| candidate.episode_key == episode.episode_key)
            else {
                missing_episode_count += 1;
                continue;
            };

            let next_video_path = map_episode
                .video
                .as_ref()
                .map(|path| resolve_project_map_path(&base_dir, path));
            let next_primary_subtitle_path = map_episode
                .primary_subtitle
                .as_ref()
                .map(|path| resolve_project_map_path(&base_dir, path));
            let next_secondary_subtitle_path = map_episode
                .secondary_subtitle
                .as_ref()
                .map(|path| resolve_project_map_path(&base_dir, path));

            if episode.video_path != next_video_path
                || episode.primary_subtitle_path != next_primary_subtitle_path
                || episode.secondary_subtitle_path != next_secondary_subtitle_path
            {
                repaired_episode_count += 1;
            }

            episode.video_path = next_video_path;
            episode.primary_subtitle_path = next_primary_subtitle_path;
            episode.secondary_subtitle_path = next_secondary_subtitle_path;
            episode.updated_at_unix = now;
        }

        entry.project_name = project_map.project_name;
        entry.season = project_map.season;
        entry.output_dir = base_dir;
        entry.updated_at_unix = now;
    }

    normalize_library_file(&mut file);
    let updated_entry = file.entries[entry_index].to_owned();
    write_library_file(library_path, &file)?;

    Ok(RepairLibraryEntryPathsResult {
        entry: updated_entry,
        repaired_episode_count,
        missing_episode_count,
        map_file_path,
    })
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

pub fn scan_embedded_subtitle_tracks(
    library_path: &Path,
    mpv_path: &Path,
    request: ScanEmbeddedSubtitleTracksRequest,
) -> AppResult<ScanEmbeddedSubtitleTracksResult> {
    let mut file = load_local_library(library_path)?;
    let now = now_unix();
    let Some(entry_index) = file
        .entries
        .iter()
        .position(|entry| entry.id == request.entry_id)
    else {
        return Err(AppError::LibrarySave("未找到本地动漫库条目".to_owned()));
    };

    let mut scanned_episode_count = 0usize;
    let mut embedded_subtitle_count = 0usize;
    let mut episodes_without_embedded_subtitles = 0usize;
    let mut failed_episodes = Vec::new();

    {
        let entry = &mut file.entries[entry_index];
        let sample = entry.episodes.iter().find_map(|episode| {
            let video_path = episode.video_path.as_ref()?;
            if video_path.is_file() {
                Some((episode.episode_key.to_owned(), video_path.to_path_buf()))
            } else {
                None
            }
        });
        let episodes_with_video = entry
            .episodes
            .iter()
            .filter(|episode| episode.video_path.is_some())
            .count();

        match sample {
            Some((sample_episode_key, sample_video_path)) => {
                scanned_episode_count = 1;
                match scan_episode_embedded_tracks(mpv_path, &sample_video_path) {
                    Ok(tracks) => {
                        if tracks.is_empty() {
                            episodes_without_embedded_subtitles = episodes_with_video;
                        }
                        embedded_subtitle_count = tracks.len();
                        for episode in entry
                            .episodes
                            .iter_mut()
                            .filter(|episode| episode.video_path.is_some())
                        {
                            episode.embedded_subtitle_tracks = tracks.clone();
                            episode.updated_at_unix = now;
                        }
                    }
                    Err(error) => failed_episodes.push(EmbeddedSubtitleScanFailure {
                        episode_key: sample_episode_key,
                        message: error.to_string(),
                    }),
                }
            }
            None => failed_episodes.push(EmbeddedSubtitleScanFailure {
                episode_key: "all".to_owned(),
                message: "没有找到可用于识别内封字幕的视频文件".to_owned(),
            }),
        }
        entry.updated_at_unix = now;
    }

    normalize_library_file(&mut file);
    let updated_entry = file.entries[entry_index].to_owned();
    write_library_file(library_path, &file)?;

    Ok(ScanEmbeddedSubtitleTracksResult {
        entry: updated_entry,
        scanned_episode_count,
        embedded_subtitle_count,
        episodes_without_embedded_subtitles,
        failed_episodes,
    })
}

fn scan_episode_embedded_tracks(
    mpv_path: &Path,
    video_path: &Path,
) -> AppResult<Vec<crate::domain::EmbeddedSubtitleTrack>> {
    if !video_path.is_file() {
        return Err(AppError::MissingFile(video_path.to_path_buf()));
    }
    mpv::scan_embedded_subtitle_tracks(mpv_path, video_path)
}

fn write_library_file(library_path: &Path, file: &LocalAnimeLibraryFile) -> AppResult<()> {
    let payload = serde_json::to_string_pretty(file)?;
    if let Some(parent) = library_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(library_path, payload)?;
    Ok(())
}

fn resolve_project_map_path(base_dir: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
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
        load_local_library, remove_local_library_entry, repair_library_entry_paths,
        save_local_library_entry, update_episode_progress,
    };
    use crate::domain::{
        AnimeSubMap, AnimeSubMapEpisode, EmbeddedSubtitleTrack, LanguageCode, LibraryEpisodeRecord,
        MatchStatus, OrganizeMode, RemoveLocalLibraryEntryRequest, RepairLibraryEntryPathsRequest,
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
        assert!(loaded.entries[0].episodes[0]
            .embedded_subtitle_tracks
            .is_empty());
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

    #[test]
    fn repairs_library_episode_paths_from_project_map() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let library_path = temp.path().join("anime-library.json");
        let output_dir = temp.path().join("Dealing S01");
        let video_dir = output_dir.join("videos");
        let subtitle_dir = output_dir.join("subs").join("ja");
        fs::create_dir_all(&video_dir)?;
        fs::create_dir_all(&subtitle_dir)?;
        fs::write(video_dir.join("Dealing S01E01.mkv"), "video")?;
        fs::write(subtitle_dir.join("Dealing S01E01.ja.srt"), "subtitle")?;

        let mut request = request(output_dir.clone());
        request.episodes[0].video_path = Some(temp.path().join("old").join("Dealing S01E01.mkv"));
        request.episodes[0].secondary_subtitle_path =
            Some(temp.path().join("old").join("Dealing S01E01.ja.srt"));
        let saved = save_local_library_entry(&library_path, request)?;

        let project_map = AnimeSubMap {
            app_version: "0.1.0".to_owned(),
            project_name: "Dealing".to_owned(),
            season: "S01".to_owned(),
            output_dir: output_dir.clone(),
            primary_language: LanguageCode::ZhHans,
            secondary_language: Some(LanguageCode::Ja),
            episodes: vec![AnimeSubMapEpisode {
                episode_key: "S01E01".to_owned(),
                video: Some(std::path::PathBuf::from("videos/Dealing S01E01.mkv")),
                primary_subtitle: None,
                secondary_subtitle: Some(std::path::PathBuf::from("subs/ja/Dealing S01E01.ja.srt")),
                additional_subtitles: Vec::new(),
            }],
        };
        fs::write(
            output_dir.join("anime-sub-map.json"),
            serde_json::to_string_pretty(&project_map)?,
        )?;

        let repaired = repair_library_entry_paths(
            &library_path,
            RepairLibraryEntryPathsRequest { entry_id: saved.id },
        )?;

        assert_eq!(repaired.repaired_episode_count, 1);
        assert_eq!(
            repaired.entry.episodes[0].video_path,
            Some(video_dir.join("Dealing S01E01.mkv"))
        );
        assert_eq!(
            repaired.entry.episodes[0].secondary_subtitle_path,
            Some(subtitle_dir.join("Dealing S01E01.ja.srt"))
        );
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
                embedded_subtitle_tracks: vec![EmbeddedSubtitleTrack {
                    id: 2,
                    language: LanguageCode::Ja,
                    language_tag: Some("ja".to_owned()),
                    title: Some("Japanese".to_owned()),
                    codec: Some("ass".to_owned()),
                }],
            }],
        }
    }
}

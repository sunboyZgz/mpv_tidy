use crate::domain::{LocalAnimeLibraryEntry, LocalAnimeLibraryFile, SaveLocalLibraryRequest};
use crate::error::{AppError, AppResult};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_VERSION: &str = "0.1.0";

pub fn save_local_library_entry(
    request: SaveLocalLibraryRequest,
) -> AppResult<LocalAnimeLibraryEntry> {
    if !request.output_dir.exists() {
        fs::create_dir_all(&request.output_dir)?;
    }

    let library_path = request.output_dir.join("anime-library.json");
    let mut file = if library_path.exists() {
        let text = fs::read_to_string(&library_path)?;
        serde_json::from_str::<LocalAnimeLibraryFile>(&text)
            .map_err(|error| AppError::LibrarySave(error.to_string()))?
    } else {
        LocalAnimeLibraryFile {
            app_version: APP_VERSION.to_owned(),
            entries: Vec::new(),
        }
    };

    let entry = LocalAnimeLibraryEntry {
        project_name: request.project_name,
        season: request.season,
        output_dir: request.output_dir,
        mode: request.mode,
        episode_count: request.episodes.len(),
        episodes: request.episodes,
        organized_at_unix: now_unix(),
    };

    file.entries.retain(|existing| {
        !(existing.project_name == entry.project_name
            && existing.season == entry.season
            && existing.output_dir == entry.output_dir)
    });
    file.entries.push(entry);
    let payload = serde_json::to_string_pretty(&file)?;
    fs::write(&library_path, payload)?;
    match file.entries.last() {
        Some(saved) => Ok(saved.to_owned()),
        None => Err(AppError::LibrarySave("未能写入本地动漫库条目".to_owned())),
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
    use super::save_local_library_entry;
    use crate::domain::{LibraryEpisodeRecord, MatchStatus, OrganizeMode, SaveLocalLibraryRequest};
    use std::error::Error;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn saves_and_replaces_library_entry() -> Result<(), Box<dyn Error>> {
        let temp = tempdir()?;
        let output_dir = temp.path().join("out");
        let library_path = output_dir.join("anime-library.json");
        let first_request = request(temp.path().join("out"));

        let saved_first = save_local_library_entry(first_request)?;
        assert_eq!(saved_first.project_name, "Jujutsu Kaisen");

        let content = fs::read_to_string(library_path)?;
        assert!(content.contains("Jujutsu Kaisen"));

        let saved_second = save_local_library_entry(request(temp.path().join("out")))?;
        assert_eq!(saved_second.episode_count, 1);
        Ok(())
    }

    fn request(output_dir: std::path::PathBuf) -> SaveLocalLibraryRequest {
        SaveLocalLibraryRequest {
            project_name: "Jujutsu Kaisen".to_owned(),
            season: "S01".to_owned(),
            output_dir,
            mode: OrganizeMode::Copy,
            episodes: vec![LibraryEpisodeRecord {
                episode_key: "S01E01".to_owned(),
                video_path: None,
                primary_subtitle_path: None,
                secondary_subtitle_path: None,
                subtitle_count: 2,
                status: MatchStatus::Matched,
            }],
        }
    }
}

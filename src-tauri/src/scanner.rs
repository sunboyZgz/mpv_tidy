use crate::domain::{ScanInput, ScanResult, ScannedSubtitle, ScannedVideo};
use crate::error::{AppError, AppResult};
use crate::parser::{detect_language, parse_episode};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn scan(input: &ScanInput) -> AppResult<ScanResult> {
    let mut videos = Vec::new();
    let mut subtitles = Vec::new();

    for dir in &input.video_dirs {
        ensure_dir(dir)?;
        for path in collect_files(dir)? {
            if is_video_file(&path) {
                videos.push(scan_video(path));
            }
        }
    }

    for dir in &input.subtitle_dirs {
        ensure_dir(dir)?;
        for path in collect_files(dir)? {
            if is_subtitle_file(&path) {
                subtitles.push(scan_subtitle(path));
            }
        }
    }

    videos.sort_by(|left, right| left.path.cmp(&right.path));
    subtitles.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(ScanResult { videos, subtitles })
}

fn ensure_dir(path: &Path) -> AppResult<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(AppError::MissingDirectory(path.to_path_buf()))
    }
}

fn collect_files(dir: &Path) -> AppResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

fn scan_video(path: PathBuf) -> ScannedVideo {
    let parsed = parse_episode(&path);
    let episode = parsed.as_ref().map(|value| value.key);
    let confidence = parsed.as_ref().map_or(0, |value| value.confidence);
    let episode_key = episode.map(|value| value.to_string());
    ScannedVideo {
        file_name: file_name(&path),
        extension: extension(&path),
        file_size_bytes: file_size(&path),
        path,
        episode,
        episode_key,
        confidence,
    }
}

fn scan_subtitle(path: PathBuf) -> ScannedSubtitle {
    let parsed = parse_episode(&path);
    let episode = parsed.as_ref().map(|value| value.key);
    let confidence = parsed.as_ref().map_or(0, |value| value.confidence);
    let episode_key = episode.map(|value| value.to_string());
    let language = detect_language(&path);
    ScannedSubtitle {
        file_name: file_name(&path),
        extension: extension(&path),
        file_size_bytes: file_size(&path),
        path,
        episode,
        episode_key,
        confidence,
        language,
    }
}

fn is_video_file(path: &Path) -> bool {
    matches!(extension(path).as_str(), "mkv" | "mp4")
}

fn is_subtitle_file(path: &Path) -> bool {
    matches!(extension(path).as_str(), "ass" | "ssa" | "srt" | "vtt")
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_default()
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

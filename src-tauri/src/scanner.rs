use crate::crf::CrfSlotTagger;
use crate::domain::{ScanInput, ScanResult, ScannedSubtitle, ScannedVideo};
use crate::error::{AppError, AppResult};
use crate::parser::{
    detect_language, natural_path_cmp, parse_episode_batch, parse_episode_batch_with_crf,
    to_parse_candidates, EpisodeCandidate, ParseDecision,
};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn scan(input: &ScanInput) -> AppResult<ScanResult> {
    scan_with_crf(input, None)
}

pub fn scan_with_crf(
    input: &ScanInput,
    crf_tagger: Option<&CrfSlotTagger>,
) -> AppResult<ScanResult> {
    let mut videos = Vec::new();
    let mut subtitles = Vec::new();

    for dir in &input.video_dirs {
        ensure_dir(dir)?;
        let paths = collect_files(dir)?;
        let parsed_episodes = parse_paths(&paths, crf_tagger);
        for (path, parsed) in paths.into_iter().zip(parsed_episodes) {
            if is_video_file(&path) {
                videos.push(scan_video(path, parsed));
            }
        }
    }

    for dir in &input.subtitle_dirs {
        ensure_dir(dir)?;
        let paths = collect_files(dir)?;
        let parsed_episodes = parse_paths(&paths, crf_tagger);
        for (path, parsed) in paths.into_iter().zip(parsed_episodes) {
            if is_subtitle_file(&path) {
                subtitles.push(scan_subtitle(path, parsed));
            }
        }
    }

    videos.sort_by(|left, right| natural_path_cmp(&left.path, &right.path));
    subtitles.sort_by(|left, right| natural_path_cmp(&left.path, &right.path));

    Ok(ScanResult { videos, subtitles })
}

fn parse_paths(paths: &[PathBuf], crf_tagger: Option<&CrfSlotTagger>) -> Vec<ParseDecision> {
    match crf_tagger {
        Some(tagger) => parse_episode_batch_with_crf(paths, Some(tagger)),
        None => parse_episode_batch(paths),
    }
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

fn scan_video(path: PathBuf, parsed: ParseDecision) -> ScannedVideo {
    let accepted = parsed
        .parsed
        .as_ref()
        .filter(|value| matches!(value.status, crate::domain::ParseStatus::Accepted));
    let episode = accepted.map(|value| value.key);
    let confidence = accepted.map_or(0, |value| value.confidence);
    let episode_key = episode.map(|value| value.to_string());
    let parse_candidates = to_parse_candidates(&visible_parse_candidates(&parsed));
    ScannedVideo {
        file_name: file_name(&path),
        extension: extension(&path),
        file_size_bytes: file_size(&path),
        path,
        episode,
        episode_key,
        confidence,
        parse_status: parsed.status,
        parse_notes: parsed.notes,
        parse_candidates,
    }
}

fn scan_subtitle(path: PathBuf, parsed: ParseDecision) -> ScannedSubtitle {
    let accepted = parsed
        .parsed
        .as_ref()
        .filter(|value| matches!(value.status, crate::domain::ParseStatus::Accepted));
    let episode = accepted.map(|value| value.key);
    let confidence = accepted.map_or(0, |value| value.confidence);
    let episode_key = episode.map(|value| value.to_string());
    let language = detect_language(&path);
    let parse_candidates = to_parse_candidates(&visible_parse_candidates(&parsed));
    ScannedSubtitle {
        file_name: file_name(&path),
        extension: extension(&path),
        file_size_bytes: file_size(&path),
        path,
        episode,
        episode_key,
        confidence,
        parse_status: parsed.status,
        parse_notes: parsed.notes,
        parse_candidates,
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

fn visible_parse_candidates(parsed: &ParseDecision) -> Vec<EpisodeCandidate> {
    match parsed.parsed.as_ref() {
        Some(accepted) => parsed
            .candidates
            .iter()
            .filter(|candidate| candidate.key == accepted.key)
            .cloned()
            .collect(),
        None => parsed.candidates.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{scan, visible_parse_candidates};
    use crate::domain::{EpisodeKey, ParseCandidateSource, ParseStatus, ScanInput};
    use crate::parser::{EpisodeCandidate, ParseDecision, ParsedEpisode};
    use std::error::Error;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_uses_batch_episode_induction_and_natural_sort() -> Result<(), Box<dyn Error>> {
        let dir = tempdir()?;
        let names = [
            "Jujutsu.Kaisen.10.1080p.WEB-DL.mkv",
            "Jujutsu.Kaisen.2.1080p.WEB-DL.mkv",
            "Jujutsu.Kaisen.1.1080p.WEB-DL.mkv",
        ];

        for name in names {
            fs::write(dir.path().join(name), [])?;
        }

        let result = scan(&ScanInput {
            video_dirs: vec![dir.path().to_path_buf()],
            subtitle_dirs: Vec::new(),
        })?;

        assert_eq!(result.videos.len(), 3);
        assert_eq!(result.videos[0].episode, Some(EpisodeKey::new(1, 1)));
        assert_eq!(result.videos[0].parse_status, ParseStatus::Accepted);
        assert!(result.videos[0]
            .parse_notes
            .iter()
            .any(|note| note.contains("模板置信度")));
        assert_eq!(result.videos[1].episode, Some(EpisodeKey::new(1, 2)));
        assert_eq!(result.videos[2].episode, Some(EpisodeKey::new(1, 10)));

        Ok(())
    }

    #[test]
    fn accepted_parse_evidence_only_exposes_matching_episode_candidates() {
        let accepted_key = EpisodeKey::new(1, 11);
        let decision = ParseDecision {
            parsed: Some(ParsedEpisode {
                key: accepted_key,
                confidence: 90,
                candidates: Vec::new(),
                status: ParseStatus::Accepted,
                notes: Vec::new(),
            }),
            status: ParseStatus::Accepted,
            notes: Vec::new(),
            candidates: vec![
                candidate(EpisodeKey::new(1, 1), 90),
                candidate(accepted_key, 90),
                candidate(accepted_key, 76),
            ],
        };

        let visible = visible_parse_candidates(&decision);

        assert_eq!(visible.len(), 2);
        assert!(visible
            .iter()
            .all(|candidate| candidate.key == accepted_key));
    }

    fn candidate(key: EpisodeKey, confidence: u8) -> EpisodeCandidate {
        EpisodeCandidate {
            key,
            confidence,
            source: ParseCandidateSource::Rule,
            note: "test candidate".to_owned(),
        }
    }
}

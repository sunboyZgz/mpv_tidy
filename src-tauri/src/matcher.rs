use crate::domain::{
    EpisodeKey, EpisodeMatch, LanguageCode, MatchStatus, ScanAndMatchResult, ScanResult,
    ScannedSubtitle, ScannedVideo, SubtitleCandidate, SubtitleRole,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn match_scan(scan: ScanResult) -> ScanAndMatchResult {
    let mut video_groups: BTreeMap<EpisodeKey, Vec<ScannedVideo>> = BTreeMap::new();
    let mut subtitle_groups: BTreeMap<EpisodeKey, Vec<ScannedSubtitle>> = BTreeMap::new();
    let mut unprocessed_videos = Vec::new();
    let mut unprocessed_subtitles = Vec::new();

    for video in &scan.videos {
        if let Some(episode) = video.episode {
            video_groups
                .entry(episode)
                .or_default()
                .push(video.to_owned());
        } else {
            unprocessed_videos.push(video.to_owned());
        }
    }

    for subtitle in &scan.subtitles {
        if let Some(episode) = subtitle.episode {
            subtitle_groups
                .entry(episode)
                .or_default()
                .push(subtitle.to_owned());
        } else {
            unprocessed_subtitles.push(subtitle.to_owned());
        }
    }

    let mut keys = BTreeSet::new();
    keys.extend(video_groups.keys().copied());
    keys.extend(subtitle_groups.keys().copied());

    let preferences = LanguageCode::default_preference();
    let mut matches = Vec::new();
    for key in keys {
        let videos = video_groups.remove(&key).unwrap_or_default();
        let subtitles = subtitle_groups.remove(&key).unwrap_or_default();
        matches.push(build_episode_match(key, videos, subtitles, &preferences));
    }

    ScanAndMatchResult {
        scan,
        matches,
        unprocessed_videos,
        unprocessed_subtitles,
    }
}

fn build_episode_match(
    episode: EpisodeKey,
    mut videos: Vec<ScannedVideo>,
    subtitles: Vec<ScannedSubtitle>,
    preferences: &[LanguageCode],
) -> EpisodeMatch {
    videos.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.path.cmp(&right.path))
    });

    let video = videos.first().map(ToOwned::to_owned);
    let mut notes = Vec::new();
    if videos.len() > 1 {
        notes.push("同一集检测到多个视频文件".to_owned());
    }

    let mut candidates = subtitles
        .into_iter()
        .map(|subtitle| SubtitleCandidate {
            path: subtitle.path,
            file_name: subtitle.file_name,
            extension: subtitle.extension,
            language: subtitle.language,
            confidence: subtitle.confidence,
            role: SubtitleRole::Candidate,
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        language_rank(left.language, preferences)
            .cmp(&language_rank(right.language, preferences))
            .then_with(|| {
                subtitle_format_rank(&left.extension).cmp(&subtitle_format_rank(&right.extension))
            })
            .then_with(|| right.confidence.cmp(&left.confidence))
            .then_with(|| left.path.cmp(&right.path))
    });

    let has_duplicate_language = candidates.iter().any(|candidate| {
        candidates
            .iter()
            .filter(|other| other.language == candidate.language)
            .count()
            > 1
    });
    if has_duplicate_language {
        notes.push("同一语言存在多个字幕候选，需要确认".to_owned());
    }

    let primary_language = preferences.first().copied().unwrap_or(LanguageCode::ZhHans);
    let secondary_language = preferences.get(2).copied().unwrap_or(LanguageCode::Ja);
    let primary_subtitle = best_for_language(&candidates, primary_language, SubtitleRole::Primary);
    let secondary_subtitle =
        best_for_language(&candidates, secondary_language, SubtitleRole::Secondary);

    let status = match (
        video.is_some(),
        primary_subtitle.is_some(),
        has_duplicate_language,
        videos.len() > 1,
    ) {
        (false, true, false, false) | (false, true, true, false) => MatchStatus::MissingVideo,
        (true, false, false, false) => MatchStatus::MissingSub,
        (true, _, true, _) | (true, _, _, true) => MatchStatus::Conflict,
        (true, true, false, false) => MatchStatus::Matched,
        (false, false, false, false) => MatchStatus::Unprocessed,
        (false, false, true, _) => MatchStatus::Conflict,
        (false, true, false, true) => MatchStatus::MissingVideo,
        (false, true, true, true) => MatchStatus::Conflict,
        (false, false, false, true) => MatchStatus::MissingVideo,
    };

    EpisodeMatch {
        episode,
        episode_key: episode.to_string(),
        video,
        primary_subtitle,
        secondary_subtitle,
        candidates,
        status,
        notes,
    }
}

fn best_for_language(
    candidates: &[SubtitleCandidate],
    language: LanguageCode,
    role: SubtitleRole,
) -> Option<SubtitleCandidate> {
    let mut matching = candidates
        .iter()
        .filter(|candidate| candidate.language == language)
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        subtitle_format_rank(&left.extension)
            .cmp(&subtitle_format_rank(&right.extension))
            .then_with(|| right.confidence.cmp(&left.confidence))
            .then_with(|| left.path.cmp(&right.path))
    });
    matching.first().map(|candidate| {
        let mut selected = (*candidate).to_owned();
        selected.role = role;
        selected
    })
}

fn language_rank(language: LanguageCode, preferences: &[LanguageCode]) -> usize {
    preferences
        .iter()
        .position(|candidate| *candidate == language)
        .unwrap_or(preferences.len())
}

fn subtitle_format_rank(extension: &str) -> usize {
    match extension {
        "ass" => 0,
        "ssa" => 1,
        "srt" => 2,
        "vtt" => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::match_scan;
    use crate::domain::{
        LanguageCode, MatchStatus, ParseStatus, ScanResult, ScannedSubtitle, ScannedVideo,
    };
    use std::path::PathBuf;

    #[test]
    fn matches_one_video_with_one_subtitle() {
        let result = match_scan(scan_result(
            vec![video("S01E01.mkv")],
            vec![sub("S01E01.zh-Hans.ass", LanguageCode::ZhHans)],
        ));

        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].status, MatchStatus::Matched);
        assert!(result.matches[0].primary_subtitle.is_some());
    }

    #[test]
    fn matches_primary_and_secondary_subtitles() {
        let result = match_scan(scan_result(
            vec![video("S01E01.mkv")],
            vec![
                sub("S01E01.zh-Hans.ass", LanguageCode::ZhHans),
                sub("S01E01.ja.srt", LanguageCode::Ja),
            ],
        ));

        assert_eq!(result.matches[0].status, MatchStatus::Matched);
        assert!(result.matches[0].primary_subtitle.is_some());
        assert!(result.matches[0].secondary_subtitle.is_some());
    }

    #[test]
    fn reports_missing_primary_subtitle() {
        let result = match_scan(scan_result(
            vec![video("S01E01.mkv")],
            vec![sub("S01E01.ja.srt", LanguageCode::Ja)],
        ));

        assert_eq!(result.matches[0].status, MatchStatus::MissingSub);
    }

    #[test]
    fn reports_conflict_for_duplicate_subtitle_candidates() {
        let result = match_scan(scan_result(
            vec![video("S01E01.mkv")],
            vec![
                sub("S01E01.zh-Hans.ass", LanguageCode::ZhHans),
                sub("S01E01.zh-Hans.srt", LanguageCode::ZhHans),
            ],
        ));

        assert_eq!(result.matches[0].status, MatchStatus::Conflict);
    }

    #[test]
    fn keeps_files_without_episode_out_of_match_table() {
        let mut video = video("Movie.1080p.H.264.mkv");
        video.episode = None;
        video.episode_key = None;
        let mut subtitle = sub("Subtitle.10bit.[2D6390A9].ass", LanguageCode::ZhHans);
        subtitle.episode = None;
        subtitle.episode_key = None;

        let result = match_scan(scan_result(vec![video], vec![subtitle]));

        assert!(result.matches.is_empty());
        assert_eq!(result.unprocessed_videos.len(), 1);
        assert_eq!(result.unprocessed_subtitles.len(), 1);
    }

    #[test]
    fn keeps_ambiguous_parser_results_out_of_match_table() {
        let mut video = video("A-01-02-03.mkv");
        video.episode = None;
        video.episode_key = None;
        video.confidence = 0;
        video.parse_status = ParseStatus::Ambiguous;
        video.parse_notes = vec!["存在多个接近的 episode 候选，需要手动确认。".to_owned()];

        let result = match_scan(scan_result(vec![video], Vec::new()));

        assert!(result.matches.is_empty());
        assert_eq!(result.unprocessed_videos.len(), 1);
        assert_eq!(
            result.unprocessed_videos[0].parse_status,
            ParseStatus::Ambiguous
        );
    }

    fn scan_result(videos: Vec<ScannedVideo>, subtitles: Vec<ScannedSubtitle>) -> ScanResult {
        ScanResult { videos, subtitles }
    }

    fn video(name: &str) -> ScannedVideo {
        ScannedVideo {
            path: PathBuf::from(name),
            file_name: name.to_owned(),
            extension: "mkv".to_owned(),
            file_size_bytes: 0,
            episode: Some(crate::domain::EpisodeKey::new(1, 1)),
            episode_key: Some("S01E01".to_owned()),
            confidence: 100,
            parse_status: ParseStatus::Accepted,
            parse_notes: Vec::new(),
            parse_candidates: Vec::new(),
        }
    }

    fn sub(name: &str, language: LanguageCode) -> ScannedSubtitle {
        ScannedSubtitle {
            path: PathBuf::from(name),
            file_name: name.to_owned(),
            extension: name.rsplit('.').next().unwrap_or("ass").to_owned(),
            file_size_bytes: 0,
            episode: Some(crate::domain::EpisodeKey::new(1, 1)),
            episode_key: Some("S01E01".to_owned()),
            confidence: 100,
            parse_status: ParseStatus::Accepted,
            parse_notes: Vec::new(),
            parse_candidates: Vec::new(),
            language,
        }
    }
}

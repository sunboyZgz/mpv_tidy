use crate::domain::{EpisodeKey, LanguageCode};
use regex::Regex;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEpisode {
    pub key: EpisodeKey,
    pub confidence: u8,
    pub candidates: Vec<EpisodeCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeCandidate {
    pub key: EpisodeKey,
    pub confidence: u8,
}

pub fn parse_episode(path: &Path) -> Option<ParsedEpisode> {
    let text = searchable_path_text(path);
    let season = parse_season(&text).unwrap_or(1);
    let mut candidates = Vec::new();

    collect_sxx_exx(&text, &mut candidates);
    collect_chinese_episode(&text, season, &mut candidates);
    collect_prefixed_episode(&text, season, &mut candidates);
    collect_dash_episode(&text, season, &mut candidates);
    collect_bracket_episode(&text, season, &mut candidates);

    candidates.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.key.episode.cmp(&right.key.episode))
    });
    candidates.dedup_by(|left, right| left.key == right.key && left.confidence == right.confidence);

    let best = candidates.first()?;
    Some(ParsedEpisode {
        key: best.key,
        confidence: best.confidence,
        candidates,
    })
}

pub fn detect_language(path: &Path) -> LanguageCode {
    let text = searchable_path_text(path);
    let lower = text.to_lowercase();

    if contains_any(
        &lower,
        &[
            "zh-hans", "zh_cn", "zh-cn", ".chs.", "[chs]", "简中", "简体",
        ],
    ) {
        return LanguageCode::ZhHans;
    }
    if contains_any(
        &lower,
        &[
            "zh-hant", "zh_tw", "zh-tw", ".cht.", "[cht]", "繁中", "繁体",
        ],
    ) {
        return LanguageCode::ZhHant;
    }
    if contains_any(
        &lower,
        &["ja-jp", ".jpn.", "[jpn]", ".ja.", "[ja]", "日文", "日本語"],
    ) {
        return LanguageCode::Ja;
    }
    if contains_any(
        &lower,
        &["english", ".eng.", "[eng]", ".en.", "[en]", "英文"],
    ) {
        return LanguageCode::En;
    }
    if isolated_token(&lower, "sc") {
        return LanguageCode::ZhHans;
    }
    if isolated_token(&lower, "tc") {
        return LanguageCode::ZhHant;
    }
    if isolated_token(&lower, "ja") {
        return LanguageCode::Ja;
    }
    if isolated_token(&lower, "en") {
        return LanguageCode::En;
    }

    LanguageCode::Und
}

fn parse_season(text: &str) -> Option<u16> {
    let patterns = [
        r"(?i)(?:^|[^a-z0-9])s0*(\d{1,2})(?:e\d{1,3}|[^a-z0-9]|$)",
        r"(?i)season\s*0*(\d{1,2})",
        r"第\s*0*(\d{1,2})\s*季",
    ];

    for pattern in patterns {
        let regex = Regex::new(pattern).ok()?;
        if let Some(captures) = regex.captures(text) {
            if let Some(value) = parse_capture_u16(&captures, 1) {
                if (1..=99).contains(&value) {
                    return Some(value);
                }
            }
        }
    }

    None
}

fn collect_sxx_exx(text: &str, candidates: &mut Vec<EpisodeCandidate>) {
    let pattern = r"(?i)(?:^|[^a-z0-9])s0*(\d{1,2})\s*e0*(\d{1,3})(?:v\d+)?(?:[^a-z0-9]|$)";
    if let Ok(regex) = Regex::new(pattern) {
        for captures in regex.captures_iter(text) {
            if let (Some(season), Some(episode)) = (
                parse_capture_u16(&captures, 1),
                parse_capture_u16(&captures, 2),
            ) {
                push_candidate(candidates, season, episode, 100);
            }
        }
    }
}

fn collect_chinese_episode(text: &str, season: u16, candidates: &mut Vec<EpisodeCandidate>) {
    if let Ok(regex) = Regex::new(r"第\s*0*(\d{1,3})\s*[话話]") {
        for captures in regex.captures_iter(text) {
            if let Some(episode) = parse_capture_u16(&captures, 1) {
                push_candidate(candidates, season, episode, 95);
            }
        }
    }
}

fn collect_prefixed_episode(text: &str, season: u16, candidates: &mut Vec<EpisodeCandidate>) {
    let patterns = [
        (
            r"(?i)(?:^|[^a-z])ep\s*0*(\d{1,3})(?:v\d+)?(?:[^a-z0-9]|$)",
            88,
        ),
        (
            r"(?i)(?:^|[^a-z])e\s*0*(\d{1,3})(?:v\d+)?(?:[^a-z0-9]|$)",
            82,
        ),
    ];

    for (pattern, confidence) in patterns {
        if let Ok(regex) = Regex::new(pattern) {
            for captures in regex.captures_iter(text) {
                if let Some(episode) = parse_capture_u16(&captures, 1) {
                    push_candidate(candidates, season, episode, confidence);
                }
            }
        }
    }
}

fn collect_dash_episode(text: &str, season: u16, candidates: &mut Vec<EpisodeCandidate>) {
    if let Ok(regex) = Regex::new(r"(?:^|[\s._])-\s*0*(\d{1,3})(?:v\d+)?(?:[^a-z0-9]|$)") {
        for captures in regex.captures_iter(text) {
            if let Some(episode) = parse_capture_u16(&captures, 1) {
                push_candidate(candidates, season, episode, 78);
            }
        }
    }
}

fn collect_bracket_episode(text: &str, season: u16, candidates: &mut Vec<EpisodeCandidate>) {
    if let Ok(regex) = Regex::new(r"\[\s*0*(\d{1,3})(?:v\d+)?\s*\]") {
        for captures in regex.captures_iter(text) {
            if let Some(episode) = parse_capture_u16(&captures, 1) {
                push_candidate(candidates, season, episode, 70);
            }
        }
    }
}

fn push_candidate(
    candidates: &mut Vec<EpisodeCandidate>,
    season: u16,
    episode: u16,
    confidence: u8,
) {
    if is_plausible_episode(episode) {
        candidates.push(EpisodeCandidate {
            key: EpisodeKey::new(season, episode),
            confidence,
        });
    }
}

fn is_plausible_episode(value: u16) -> bool {
    (1..=200).contains(&value) && !matches!(value, 264 | 265)
}

fn parse_capture_u16(captures: &regex::Captures<'_>, index: usize) -> Option<u16> {
    captures.get(index)?.as_str().parse::<u16>().ok()
}

fn searchable_path_text(path: &Path) -> String {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn isolated_token(text: &str, token: &str) -> bool {
    let pattern = format!(
        r"(?i)(?:^|[^a-z0-9]){}(?:[^a-z0-9]|$)",
        regex::escape(token)
    );
    Regex::new(&pattern)
        .map(|regex| regex.is_match(text))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{detect_language, parse_episode};
    use crate::domain::{EpisodeKey, LanguageCode};
    use std::error::Error;
    use std::path::PathBuf;

    #[test]
    fn parses_common_anime_episode_names() -> Result<(), Box<dyn Error>> {
        let fixtures = [
            (
                "[SubsPlease] Jujutsu Kaisen - 01v2 (1080p) [2D6390A9].mkv",
                EpisodeKey::new(1, 1),
            ),
            (
                "呪術廻戦.第1話.両面宿儺.AMZN.WEB-DL.ja-jp.srt",
                EpisodeKey::new(1, 1),
            ),
            (
                "[DBD-Raws][咒术回战][01][1080P][BDRip][HEVC-10bit][FLAC].SC.ass",
                EpisodeKey::new(1, 1),
            ),
            (
                "Dealing.With.Mikadono.Sisters.Is.A.Breeze.S01E01.Prodigy.and.Mediocrity.1080p.CR.WEB-DL.DUAL.DDP2.0.H.264-Dooky.mkv",
                EpisodeKey::new(1, 1),
            ),
        ];

        for (name, expected) in fixtures {
            let parsed = parse_episode(&PathBuf::from(name)).ok_or("episode should parse")?;
            assert_eq!(parsed.key, expected);
        }

        Ok(())
    }

    #[test]
    fn avoids_resolution_codec_and_hash_false_positives() {
        let fixtures = [
            "Movie.1080p.HEVC-10bit.FLAC.mkv",
            "Archive.[2D6390A9].H.264.AAC.ass",
            "Show.2025.2160p.x265.mkv",
        ];

        for name in fixtures {
            assert!(parse_episode(&PathBuf::from(name)).is_none());
        }
    }

    #[test]
    fn detects_language_from_file_and_parent_directory() {
        let fixtures = [
            ("subs/简中/Jujutsu.Kaisen.S01E01.ass", LanguageCode::ZhHans),
            ("subs/繁体/Jujutsu.Kaisen.S01E01.srt", LanguageCode::ZhHant),
            ("subs/Jujutsu.Kaisen.S01E01.ja-jp.srt", LanguageCode::Ja),
            (
                "subs/English/Jujutsu.Kaisen.S01E01.en.srt",
                LanguageCode::En,
            ),
            ("subs/Jujutsu.Kaisen.S01E01.srt", LanguageCode::Und),
        ];

        for (name, expected) in fixtures {
            assert_eq!(detect_language(&PathBuf::from(name)), expected);
        }
    }
}

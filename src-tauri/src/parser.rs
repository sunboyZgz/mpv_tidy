use crate::domain::{EpisodeKey, LanguageCode, ParseCandidate, ParseCandidateSource, ParseStatus};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEpisode {
    pub key: EpisodeKey,
    pub confidence: u8,
    pub candidates: Vec<EpisodeCandidate>,
    pub status: ParseStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeCandidate {
    pub key: EpisodeKey,
    pub confidence: u8,
    pub source: ParseCandidateSource,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDecision {
    pub parsed: Option<ParsedEpisode>,
    pub status: ParseStatus,
    pub notes: Vec<String>,
    pub candidates: Vec<EpisodeCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumberToken {
    value: u32,
    width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Alpha,
    Number,
    Separator,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    lower: String,
    kind: TokenKind,
    number: Option<NumberToken>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenSequence {
    tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
struct CohortPath {
    sequence: TokenSequence,
    season: u16,
    special: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotLabel {
    Episode,
    Season,
    Version,
    Hash,
    Resolution,
    Codec,
    Source,
    Language,
    Title,
    Noise,
    Special,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParseEvidence {
    source: ParseCandidateSource,
    label: SlotLabel,
    confidence: u8,
    note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplateSlot {
    token_index: usize,
    label: SlotLabel,
    confidence: u8,
    evidence: ParseEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplatePattern {
    slots: Vec<TemplateSlot>,
    confidence: u8,
    notes: Vec<String>,
}

pub fn parse_episode(path: &Path) -> Option<ParsedEpisode> {
    let text = searchable_path_text(path);
    let mut notes = Vec::new();
    let candidates = regex_episode_candidates(&text);
    if candidates.is_empty() {
        notes.push("未命中单文件强规则。".to_owned());
    }
    decide_episode(candidates, notes).parsed
}

pub fn parse_episode_decision(path: &Path) -> ParseDecision {
    if let Some(parsed) = parse_episode(path) {
        return ParseDecision {
            status: parsed.status,
            notes: parsed.notes.clone(),
            candidates: parsed.candidates.clone(),
            parsed: Some(parsed),
        };
    }

    let text = searchable_path_text(path);
    let mut notes = Vec::new();
    let candidates = regex_episode_candidates(&text);
    if candidates.is_empty() {
        notes.push("未命中单文件强规则。".to_owned());
    }
    decide_episode(candidates, notes)
}

pub fn parse_episode_batch(paths: &[PathBuf]) -> Vec<ParseDecision> {
    let mut parsed = paths
        .iter()
        .map(|path| parse_episode_decision(path))
        .collect::<Vec<_>>();

    let mut cohorts: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, path) in paths.iter().enumerate() {
        cohorts.entry(cohort_key(path)).or_default().push(index);
    }

    for indexes in cohorts.values() {
        if indexes.len() < 2 {
            continue;
        }

        let cohort_paths = indexes
            .iter()
            .map(|index| CohortPath {
                sequence: TokenSequence {
                    tokens: tokenize_file_stem(&paths[*index]),
                },
                season: parse_season(&searchable_path_text(&paths[*index])).unwrap_or(1),
                special: is_special_file(&tokenize_file_stem(&paths[*index])),
            })
            .collect::<Vec<_>>();

        let Some(template) = infer_template_pattern(&cohort_paths) else {
            for original_index in indexes {
                if parsed[*original_index].status == ParseStatus::Rejected {
                    parsed[*original_index]
                        .notes
                        .push("同组文件未能归纳出稳定 episode 模板。".to_owned());
                }
            }
            continue;
        };
        let episode_slots = template
            .slots
            .iter()
            .filter(|slot| slot.label == SlotLabel::Episode)
            .collect::<Vec<_>>();
        if episode_slots.is_empty() {
            continue;
        }

        for (cohort_index, original_index) in indexes.iter().enumerate() {
            if cohort_paths[cohort_index].special {
                parsed[*original_index] = decide_episode(
                    parsed[*original_index].candidates.clone(),
                    unique_notes(
                        parsed[*original_index]
                            .notes
                            .iter()
                            .cloned()
                            .chain([
                                "识别为 OVA/SP/NCOP/NCED/PV/Trailer 等特殊内容，需要手动确认。"
                                    .to_owned(),
                            ])
                            .collect(),
                    ),
                );
                continue;
            }

            let mut candidates = parsed[*original_index].candidates.clone();
            for slot in &episode_slots {
                let Some(token) = cohort_paths[cohort_index]
                    .sequence
                    .tokens
                    .get(slot.token_index)
                else {
                    continue;
                };
                let Some(number) = token.number.as_ref() else {
                    continue;
                };
                let Some(episode) = u16::try_from(number.value).ok() else {
                    continue;
                };
                if !is_plausible_episode(episode) {
                    continue;
                }

                candidates.push(EpisodeCandidate {
                    key: EpisodeKey::new(cohort_paths[cohort_index].season, episode),
                    confidence: slot.confidence,
                    source: slot.evidence.source,
                    note: slot.evidence.note.clone(),
                });
            }
            let mut notes = parsed[*original_index].notes.clone();
            notes.extend(template.notes.iter().cloned());
            parsed[*original_index] = decide_episode(candidates, unique_notes(notes));
        }
    }

    parsed
}

pub fn to_parse_candidates(candidates: &[EpisodeCandidate]) -> Vec<ParseCandidate> {
    candidates
        .iter()
        .map(|candidate| ParseCandidate {
            episode: candidate.key,
            episode_key: candidate.key.to_string(),
            confidence: candidate.confidence,
            source: candidate.source,
            note: candidate.note.clone(),
        })
        .collect()
}

pub fn natural_path_cmp(left: &Path, right: &Path) -> Ordering {
    natural_str_cmp(&path_sort_text(left), &path_sort_text(right))
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

fn regex_episode_candidates(text: &str) -> Vec<EpisodeCandidate> {
    let season = parse_season(text).unwrap_or(1);
    let mut candidates = Vec::new();

    collect_sxx_exx(text, &mut candidates);
    collect_chinese_episode(text, season, &mut candidates);
    collect_versioned_episode(text, season, &mut candidates);
    collect_prefixed_episode(text, season, &mut candidates);
    collect_embedded_prefixed_episode(text, season, &mut candidates);
    collect_dash_episode(text, season, &mut candidates);
    collect_bracket_episode(text, season, &mut candidates);

    sort_candidates(&mut candidates);
    candidates
}

fn decide_episode(mut candidates: Vec<EpisodeCandidate>, notes: Vec<String>) -> ParseDecision {
    sort_candidates(&mut candidates);
    let notes = unique_notes(notes);
    let Some(best) = candidates.first().cloned() else {
        return ParseDecision {
            parsed: None,
            status: ParseStatus::Rejected,
            notes,
            candidates,
        };
    };

    let status = if has_close_competing_candidate(&candidates) {
        ParseStatus::Ambiguous
    } else if best.confidence >= 70 {
        ParseStatus::Accepted
    } else {
        ParseStatus::LowConfidence
    };

    let mut decision_notes = notes;
    match status {
        ParseStatus::Accepted => {
            decision_notes.push(format!("已接受 {}，置信度 {}。", best.key, best.confidence));
        }
        ParseStatus::LowConfidence => {
            decision_notes.push(format!(
                "{} 置信度 {} 低于自动匹配阈值，需要手动确认。",
                best.key, best.confidence
            ));
        }
        ParseStatus::Ambiguous => {
            decision_notes.push("存在多个接近的 episode 候选，需要手动确认。".to_owned());
        }
        ParseStatus::Rejected => {}
    }

    let parsed = if status == ParseStatus::Ambiguous {
        None
    } else {
        Some(ParsedEpisode {
            key: best.key,
            confidence: best.confidence,
            candidates: candidates.clone(),
            status,
            notes: unique_notes(decision_notes.clone()),
        })
    };

    ParseDecision {
        parsed,
        status,
        notes: unique_notes(decision_notes),
        candidates,
    }
}

fn sort_candidates(candidates: &mut Vec<EpisodeCandidate>) {
    candidates.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.key.season.cmp(&right.key.season))
            .then_with(|| left.key.episode.cmp(&right.key.episode))
    });
    candidates.dedup_by(|left, right| {
        left.key == right.key && left.confidence == right.confidence && left.source == right.source
    });
}

fn has_close_competing_candidate(candidates: &[EpisodeCandidate]) -> bool {
    let Some(best) = candidates.first() else {
        return false;
    };
    candidates.iter().skip(1).any(|candidate| {
        candidate.key != best.key && best.confidence.saturating_sub(candidate.confidence) < 8
    })
}

fn unique_notes(notes: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for note in notes {
        if seen.insert(note.clone()) {
            unique.push(note);
        }
    }
    unique
}

fn infer_template_pattern(paths: &[CohortPath]) -> Option<TemplatePattern> {
    let mut groups: BTreeMap<String, Vec<(usize, usize, u32)>> = BTreeMap::new();
    for (path_index, path) in paths.iter().enumerate() {
        if path.special {
            continue;
        }
        for (token_index, token) in path.sequence.tokens.iter().enumerate() {
            let Some(number) = token.number.as_ref() else {
                continue;
            };
            let label = classify_number_slot(&path.sequence.tokens, token_index);
            if matches!(
                label,
                SlotLabel::Noise
                    | SlotLabel::Hash
                    | SlotLabel::Resolution
                    | SlotLabel::Codec
                    | SlotLabel::Version
                    | SlotLabel::Language
                    | SlotLabel::Source
            ) {
                continue;
            }
            groups
                .entry(slot_context_key(&path.sequence.tokens, token_index))
                .or_default()
                .push((path_index, token_index, number.value));
        }
    }

    let mut slots = Vec::new();
    for occurrences in groups.values() {
        let covered_paths = occurrences
            .iter()
            .map(|(path_index, _, _)| *path_index)
            .collect::<BTreeSet<_>>();
        let values = occurrences
            .iter()
            .map(|(_, _, value)| *value)
            .collect::<Vec<_>>();
        let unique_values = values.iter().copied().collect::<BTreeSet<_>>();
        if covered_paths.len() < 2
            || unique_values.len() < 2
            || values.iter().any(|value| !is_plausible_episode_u32(*value))
        {
            continue;
        }

        let token_index = occurrences
            .iter()
            .map(|(_, token_index, _)| *token_index)
            .min()
            .unwrap_or(0);
        let sample_tokens = &paths[occurrences[0].0].sequence.tokens;
        let label = classify_number_slot(sample_tokens, token_index);
        let mut confidence = 45u8;
        if is_monotonic_with_gaps(&values) {
            confidence = confidence.saturating_add(15);
        }
        if is_episode_marker_context(sample_tokens, token_index) {
            confidence = confidence.saturating_add(25);
        }
        if has_separator_context(sample_tokens, token_index) {
            confidence = confidence.saturating_add(8);
        }
        if appears_before_known_quality_tail(paths, token_index) {
            confidence = confidence.saturating_add(8);
        }
        if covered_paths.len() < paths.iter().filter(|path| !path.special).count() {
            confidence = confidence.saturating_sub(8);
        }

        let evidence = ParseEvidence {
            source: ParseCandidateSource::Template,
            label,
            confidence: confidence.min(92),
            note: "同组文件模板归纳出变化数字槽。".to_owned(),
        };
        slots.push(TemplateSlot {
            token_index,
            label,
            confidence: evidence.confidence,
            evidence,
        });
    }

    slots.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.token_index.cmp(&right.token_index))
    });

    let best = slots.first()?.clone();
    if slots
        .iter()
        .skip(1)
        .any(|slot| best.confidence.saturating_sub(slot.confidence) < 8)
    {
        return Some(TemplatePattern {
            slots,
            confidence: best.confidence,
            notes: vec!["同组文件存在多个接近的变化数字槽，保持待确认。".to_owned()],
        });
    }

    Some(TemplatePattern {
        slots: vec![best.clone()],
        confidence: best.confidence,
        notes: vec![format!("模板置信度 {}。", best.confidence)],
    })
}

fn number_slot_is_noise(tokens: &[Token], index: usize) -> bool {
    let Some(number) = tokens.get(index).and_then(|token| token.number.as_ref()) else {
        return true;
    };

    if !is_plausible_episode_u32(number.value) {
        return true;
    }

    is_resolution(tokens, index)
        || is_codec_number(tokens, index)
        || is_bit_depth(tokens, index)
        || is_audio_number(tokens, index)
        || is_date_component(tokens, index)
        || is_version_number(tokens, index)
        || is_segment_or_page_number(tokens, index)
}

fn classify_number_slot(tokens: &[Token], index: usize) -> SlotLabel {
    if is_season_marker_context(tokens, index) {
        SlotLabel::Season
    } else if is_hash_number(tokens, index) {
        SlotLabel::Hash
    } else if is_resolution(tokens, index) {
        SlotLabel::Resolution
    } else if is_codec_number(tokens, index) {
        SlotLabel::Codec
    } else if is_version_number(tokens, index) {
        SlotLabel::Version
    } else if is_source_number(tokens, index) {
        SlotLabel::Source
    } else if is_language_number(tokens, index) {
        SlotLabel::Language
    } else if is_special_number(tokens, index) {
        SlotLabel::Special
    } else if is_title_number(tokens, index) {
        SlotLabel::Title
    } else if is_date_component(tokens, index)
        || is_bit_depth(tokens, index)
        || is_audio_number(tokens, index)
        || is_segment_or_page_number(tokens, index)
        || number_slot_is_noise(tokens, index)
    {
        SlotLabel::Noise
    } else if is_episode_marker_context(tokens, index) || has_separator_context(tokens, index) {
        SlotLabel::Episode
    } else {
        SlotLabel::Unknown
    }
}

fn slot_context_key(tokens: &[Token], index: usize) -> String {
    let left = previous_meaningful(tokens, index)
        .map(context_token_key)
        .unwrap_or_else(|| "^".to_owned());
    let right_index = optional_version_suffix_end(tokens, index).unwrap_or(index);
    let right = next_meaningful(tokens, right_index)
        .map(context_token_key)
        .unwrap_or_else(|| "$".to_owned());
    format!("{left}|{right}")
}

fn optional_version_suffix_end(tokens: &[Token], index: usize) -> Option<usize> {
    let version_index = next_meaningful_index(tokens, index)?;
    let version_token = tokens.get(version_index)?;
    if !matches!(version_token.lower.as_str(), "v" | "ver" | "version") {
        return None;
    }
    let number_index = next_meaningful_index(tokens, version_index)?;
    if tokens.get(number_index)?.kind == TokenKind::Number {
        Some(number_index)
    } else {
        None
    }
}

fn context_token_key(token: &Token) -> String {
    if token.kind == TokenKind::Number {
        "#".to_owned()
    } else {
        token.lower.clone()
    }
}

fn is_monotonic_with_gaps(values: &[u32]) -> bool {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
        .windows(2)
        .all(|pair| pair[0] < pair[1] && pair[1].saturating_sub(pair[0]) <= 20)
}

fn is_episode_marker_context(tokens: &[Token], index: usize) -> bool {
    let previous = previous_meaningful(tokens, index).map(|token| token.lower.as_str());
    let next = next_meaningful(tokens, index).map(|token| token.lower.as_str());

    matches!(previous, Some("e" | "ep" | "episode" | "第")) || matches!(next, Some("话" | "話"))
}

fn is_season_marker_context(tokens: &[Token], index: usize) -> bool {
    let previous = previous_meaningful(tokens, index).map(|token| token.lower.as_str());
    let next = next_meaningful(tokens, index).map(|token| token.lower.as_str());
    matches!(previous, Some("s" | "season" | "第")) || matches!(next, Some("季"))
}

fn has_separator_context(tokens: &[Token], index: usize) -> bool {
    let previous = index
        .checked_sub(1)
        .and_then(|previous_index| tokens.get(previous_index));
    let next = tokens.get(index + 1);
    matches!(previous.map(|token| token.kind), Some(TokenKind::Separator))
        || matches!(next.map(|token| token.kind), Some(TokenKind::Separator))
}

fn appears_before_known_quality_tail(paths: &[CohortPath], index: usize) -> bool {
    paths.iter().all(|path| {
        path.sequence
            .tokens
            .iter()
            .enumerate()
            .skip(index + 1)
            .any(|(_, token)| known_quality_or_source(&token.lower))
    })
}

fn previous_meaningful(tokens: &[Token], index: usize) -> Option<&Token> {
    tokens
        .get(..index)?
        .iter()
        .rev()
        .find(|token| token.kind != TokenKind::Separator)
}

fn next_meaningful(tokens: &[Token], index: usize) -> Option<&Token> {
    tokens
        .iter()
        .skip(index + 1)
        .find(|token| token.kind != TokenKind::Separator)
}

fn next_meaningful_index(tokens: &[Token], index: usize) -> Option<usize> {
    tokens
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, token)| token.kind != TokenKind::Separator)
        .map(|(token_index, _)| token_index)
}

fn is_resolution(tokens: &[Token], index: usize) -> bool {
    let Some(value) = tokens.get(index).and_then(|token| token.number.as_ref()) else {
        return false;
    };
    if !matches!(value.value, 480 | 720 | 1080 | 1440 | 2160 | 4320 | 4 | 8) {
        return false;
    }
    matches!(
        next_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("p" | "k")
    )
}

fn is_codec_number(tokens: &[Token], index: usize) -> bool {
    let Some(value) = tokens.get(index).and_then(|token| token.number.as_ref()) else {
        return false;
    };
    if !matches!(value.value, 264..=266) {
        return false;
    }

    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("h" | "x" | "avc" | "hevc")
    ) || matches!(
        next_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("avc" | "hevc")
    )
}

fn is_bit_depth(tokens: &[Token], index: usize) -> bool {
    let Some(value) = tokens.get(index).and_then(|token| token.number.as_ref()) else {
        return false;
    };
    matches!(value.value, 8 | 10 | 12)
        && matches!(
            next_meaningful(tokens, index).map(|token| token.lower.as_str()),
            Some("bit" | "bits")
        )
}

fn is_audio_number(tokens: &[Token], index: usize) -> bool {
    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("aac" | "flac" | "ddp" | "dd" | "atmos" | "dts")
    )
}

fn is_hash_number(tokens: &[Token], index: usize) -> bool {
    let Some(token) = tokens.get(index) else {
        return false;
    };
    token
        .number
        .as_ref()
        .is_some_and(|number| number.width >= 6 && is_bracketed(tokens, index))
}

fn is_source_number(tokens: &[Token], index: usize) -> bool {
    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("web" | "amzn" | "nf" | "cr" | "bdrip" | "bluray")
    )
}

fn is_language_number(tokens: &[Token], index: usize) -> bool {
    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("zh" | "ja" | "jp" | "jpn" | "en" | "eng")
    )
}

fn is_special_number(tokens: &[Token], index: usize) -> bool {
    previous_meaningful(tokens, index).is_some_and(|token| is_special_token(&token.lower))
}

fn is_title_number(tokens: &[Token], index: usize) -> bool {
    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("title" | "track")
    )
}

fn is_bracketed(tokens: &[Token], index: usize) -> bool {
    let previous = index
        .checked_sub(1)
        .and_then(|previous_index| tokens.get(previous_index));
    let next = tokens.get(index + 1);
    previous.is_some_and(|token| token.text.contains('[') || token.text.contains('('))
        && next.is_some_and(|token| token.text.contains(']') || token.text.contains(')'))
}

fn is_date_component(tokens: &[Token], index: usize) -> bool {
    let current = tokens.get(index).and_then(|token| token.number.as_ref());
    let previous = previous_meaningful(tokens, index).and_then(|token| token.number.as_ref());
    let next = next_meaningful(tokens, index).and_then(|token| token.number.as_ref());

    if let (Some(year), Some(month_or_day)) = (previous, current) {
        if is_year(year.value) && (1..=31).contains(&month_or_day.value) {
            return true;
        }
    }
    if let (Some(month_or_day), Some(year)) = (current, previous) {
        if is_year(year.value) && (1..=31).contains(&month_or_day.value) {
            return true;
        }
    }
    if let (Some(value), Some(following)) = (current, next) {
        if is_year(value.value) && (1..=12).contains(&following.value) {
            return true;
        }
    }

    false
}

fn is_year(value: u32) -> bool {
    (1900..=2099).contains(&value)
}

fn is_version_number(tokens: &[Token], index: usize) -> bool {
    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some("v" | "ver" | "version")
    )
}

fn is_segment_or_page_number(tokens: &[Token], index: usize) -> bool {
    matches!(
        previous_meaningful(tokens, index).map(|token| token.lower.as_str()),
        Some(
            "p" | "pv"
                | "part"
                | "page"
                | "chapter"
                | "chap"
                | "segment"
                | "seg"
                | "shard"
                | "step"
                | "backup"
                | "log"
        )
    )
}

fn known_quality_or_source(text: &str) -> bool {
    matches!(
        text,
        "p" | "k"
            | "web"
            | "webdl"
            | "web-dl"
            | "bdrip"
            | "bluray"
            | "amzn"
            | "nf"
            | "cr"
            | "h"
            | "x"
            | "hevc"
            | "avc"
            | "aac"
            | "flac"
            | "ddp"
    )
}

fn cohort_key(path: &Path) -> String {
    let parent = path
        .parent()
        .map(path_sort_text)
        .unwrap_or_default()
        .to_lowercase();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let signature = cohort_signature(path);

    format!("{parent}|{extension}|{signature}")
}

fn cohort_signature(path: &Path) -> String {
    let tokens = tokenize_file_stem(path);
    let stop_index = tokens
        .iter()
        .position(|token| token.kind == TokenKind::Number)
        .unwrap_or(tokens.len());
    let before_number = tokens
        .iter()
        .take(stop_index)
        .filter(|token| token.kind == TokenKind::Alpha)
        .filter(|token| !ignored_signature_token(&token.lower))
        .map(|token| token.lower.clone())
        .collect::<Vec<_>>();
    if !before_number.is_empty() {
        return before_number.join(" ");
    }

    tokens
        .into_iter()
        .filter(|token| token.kind == TokenKind::Alpha)
        .filter(|token| !ignored_signature_token(&token.lower))
        .map(|token| token.lower)
        .collect::<Vec<_>>()
        .join(" ")
}

fn ignored_signature_token(text: &str) -> bool {
    known_quality_or_source(text)
        || known_language_token(text)
        || is_special_token(text)
        || matches!(text, "s" | "e" | "ep" | "v" | "p" | "h" | "x")
}

fn is_special_file(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Alpha)
        .any(|token| is_special_token(&token.lower))
}

fn is_special_token(text: &str) -> bool {
    matches!(
        text,
        "ova" | "sp" | "special" | "ncop" | "nced" | "pv" | "trailer" | "op" | "ed"
    )
}

fn known_language_token(text: &str) -> bool {
    matches!(
        text,
        "zh" | "hans"
            | "hant"
            | "zh-hans"
            | "zh-hant"
            | "chs"
            | "cht"
            | "sc"
            | "tc"
            | "ja"
            | "jpn"
            | "jp"
            | "en"
            | "eng"
            | "english"
    )
}

fn tokenize_file_stem(path: &Path) -> Vec<Token> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(normalize_text)
        .map(|text| tokenize_text(&text))
        .unwrap_or_default()
}

fn tokenize_text(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    let mut current_kind: Option<TokenKind> = None;

    for character in text.chars() {
        let kind = classify_char(character);
        if current_kind.is_some_and(|existing| existing != kind) && !buffer.is_empty() {
            tokens.push(build_token(
                &buffer,
                current_kind.unwrap_or(TokenKind::Other),
            ));
            buffer.clear();
        }
        buffer.push(character);
        current_kind = Some(kind);
    }

    if let Some(kind) = current_kind {
        if !buffer.is_empty() {
            tokens.push(build_token(&buffer, kind));
        }
    }

    tokens
}

fn classify_char(character: char) -> TokenKind {
    if character.is_ascii_digit() {
        TokenKind::Number
    } else if character.is_alphabetic() || matches!(character, '第' | '话' | '話') {
        TokenKind::Alpha
    } else if character.is_whitespace()
        || matches!(
            character,
            '.' | '-' | '_' | '[' | ']' | '(' | ')' | '【' | '】' | '「' | '」'
        )
    {
        TokenKind::Separator
    } else {
        TokenKind::Other
    }
}

fn build_token(text: &str, kind: TokenKind) -> Token {
    let number = if kind == TokenKind::Number {
        text.parse::<u32>().ok().map(|value| NumberToken {
            value,
            width: text.len(),
        })
    } else {
        None
    };

    Token {
        text: text.to_owned(),
        lower: text.to_lowercase(),
        kind,
        number,
    }
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

fn collect_versioned_episode(text: &str, season: u16, candidates: &mut Vec<EpisodeCandidate>) {
    if let Ok(regex) = Regex::new(r"(?i)(?:^|[^a-z0-9])0*(\d{1,3})v\d+(?:[^a-z0-9]|$)") {
        for captures in regex.captures_iter(text) {
            if let Some(episode) = parse_capture_u16(&captures, 1) {
                push_candidate(candidates, season, episode, 90);
            }
        }
    }
}

fn collect_prefixed_episode(text: &str, season: u16, candidates: &mut Vec<EpisodeCandidate>) {
    let patterns = [
        (
            r"(?i)(?:^|[^a-z0-9])ep\s*0*(\d{1,3})(?:v\d+)?(?:[^a-z0-9]|$)",
            88,
        ),
        (
            r"(?i)(?:^|[^a-z0-9])e\s*0*(\d{1,3})(?:v\d+)?(?:[^a-z0-9]|$)",
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

fn collect_embedded_prefixed_episode(
    text: &str,
    season: u16,
    candidates: &mut Vec<EpisodeCandidate>,
) {
    if let Ok(regex) = Regex::new(r"(?i)e0*(\d{1,3})(?:v\d+)?[a-z]") {
        for captures in regex.captures_iter(text) {
            if let Some(episode) = parse_capture_u16(&captures, 1) {
                push_candidate(candidates, season, episode, 82);
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
            source: ParseCandidateSource::Rule,
            note: "命中单文件强规则。".to_owned(),
        });
    }
}

fn is_plausible_episode(value: u16) -> bool {
    is_plausible_episode_u32(u32::from(value))
}

fn is_plausible_episode_u32(value: u32) -> bool {
    (1..=200).contains(&value) && !matches!(value, 264..=266)
}

fn parse_capture_u16(captures: &regex::Captures<'_>, index: usize) -> Option<u16> {
    captures.get(index)?.as_str().parse::<u16>().ok()
}

fn searchable_path_text(path: &Path) -> String {
    normalize_text(
        &path
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn path_sort_text(path: &Path) -> String {
    normalize_text(
        &path
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn normalize_text(text: &str) -> String {
    text.nfkc()
        .map(|character| match character {
            '–' | '—' | '−' | '‐' | '‑' => '-',
            '（' => '(',
            '）' => ')',
            '［' => '[',
            '］' => ']',
            '／' | '\\' => '/',
            other => other,
        })
        .collect()
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum NaturalPart {
    Text(String),
    Number(u64),
}

fn natural_str_cmp(left: &str, right: &str) -> Ordering {
    let left_parts = natural_parts(left);
    let right_parts = natural_parts(right);

    for (left_part, right_part) in left_parts.iter().zip(right_parts.iter()) {
        let ordering = match (left_part, right_part) {
            (NaturalPart::Number(left_number), NaturalPart::Number(right_number)) => {
                left_number.cmp(right_number)
            }
            (NaturalPart::Text(left_text), NaturalPart::Text(right_text)) => {
                left_text.cmp(right_text)
            }
            (NaturalPart::Number(_), NaturalPart::Text(_)) => Ordering::Less,
            (NaturalPart::Text(_), NaturalPart::Number(_)) => Ordering::Greater,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left_parts
        .len()
        .cmp(&right_parts.len())
        .then_with(|| left.cmp(right))
}

fn natural_parts(text: &str) -> Vec<NaturalPart> {
    let mut parts = Vec::new();
    let mut buffer = String::new();
    let mut in_number: Option<bool> = None;

    for character in normalize_text(text).chars() {
        let is_number = character.is_ascii_digit();
        if in_number.is_some_and(|existing| existing != is_number) && !buffer.is_empty() {
            parts.push(build_natural_part(&buffer, in_number.unwrap_or(false)));
            buffer.clear();
        }
        buffer.push(character);
        in_number = Some(is_number);
    }

    if let Some(is_number) = in_number {
        if !buffer.is_empty() {
            parts.push(build_natural_part(&buffer, is_number));
        }
    }

    parts
}

fn build_natural_part(text: &str, is_number: bool) -> NaturalPart {
    if is_number {
        NaturalPart::Number(text.parse::<u64>().unwrap_or(u64::MAX))
    } else {
        NaturalPart::Text(text.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_language, natural_str_cmp, parse_episode, parse_episode_batch};
    use crate::domain::{EpisodeKey, LanguageCode, ParseStatus};
    use std::cmp::Ordering;
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
                "[SubsPlease] Jujutsu Kaisen - 10v2 (1080p) [78B19E01].mkv",
                EpisodeKey::new(1, 10),
            ),
            (
                "Jujutsu.Kaisen.10v2.1080p.WEB-DL.mkv",
                EpisodeKey::new(1, 10),
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
            (
                "JujutsuKaisenE01BD1080p.mkv",
                EpisodeKey::new(1, 1),
            ),
        ];

        for (name, expected) in fixtures {
            let parsed = parse_episode(&PathBuf::from(name))
                .ok_or_else(|| format!("episode should parse: {name}"))?;
            assert_eq!(parsed.key, expected);
        }

        Ok(())
    }

    #[test]
    fn infers_episode_slot_from_cohort_template() -> Result<(), Box<dyn Error>> {
        let paths = vec![
            PathBuf::from("Jujutsu.Kaisen.01.1080p.WEB-DL.mkv"),
            PathBuf::from("Jujutsu.Kaisen.02.1080p.WEB-DL.mkv"),
            PathBuf::from("Jujutsu.Kaisen.03.1080p.WEB-DL.mkv"),
        ];

        let parsed = parse_episode_batch(&paths);

        assert_eq!(
            parsed[0].parsed.as_ref().ok_or("episode 1")?.key,
            EpisodeKey::new(1, 1)
        );
        assert_eq!(
            parsed[1].parsed.as_ref().ok_or("episode 2")?.key,
            EpisodeKey::new(1, 2)
        );
        assert_eq!(
            parsed[2].parsed.as_ref().ok_or("episode 3")?.key,
            EpisodeKey::new(1, 3)
        );
        Ok(())
    }

    #[test]
    fn keeps_mixed_series_in_separate_cohorts() -> Result<(), Box<dyn Error>> {
        let paths = vec![
            PathBuf::from("ShowA - 01.mkv"),
            PathBuf::from("ShowA - 02.mkv"),
            PathBuf::from("ShowB - 01.mkv"),
            PathBuf::from("ShowB - 02.mkv"),
        ];

        let parsed = parse_episode_batch(&paths);

        assert_eq!(
            parsed[0].parsed.as_ref().ok_or("show a episode 1")?.key,
            EpisodeKey::new(1, 1)
        );
        assert_eq!(
            parsed[1].parsed.as_ref().ok_or("show a episode 2")?.key,
            EpisodeKey::new(1, 2)
        );
        assert_eq!(
            parsed[2].parsed.as_ref().ok_or("show b episode 1")?.key,
            EpisodeKey::new(1, 1)
        );
        assert_eq!(
            parsed[3].parsed.as_ref().ok_or("show b episode 2")?.key,
            EpisodeKey::new(1, 2)
        );
        Ok(())
    }

    #[test]
    fn infers_optional_version_slot_without_breaking_episode() -> Result<(), Box<dyn Error>> {
        let paths = vec![
            PathBuf::from("Show - 01v2 [1080p].mkv"),
            PathBuf::from("Show - 02 [1080p].mkv"),
            PathBuf::from("Show - 03v3 [1080p].mkv"),
        ];

        let parsed = parse_episode_batch(&paths);

        assert_eq!(
            parsed[0].parsed.as_ref().ok_or("episode 1")?.key,
            EpisodeKey::new(1, 1)
        );
        assert_eq!(
            parsed[1].parsed.as_ref().ok_or("episode 2")?.key,
            EpisodeKey::new(1, 2)
        );
        assert_eq!(
            parsed[2].parsed.as_ref().ok_or("episode 3")?.key,
            EpisodeKey::new(1, 3)
        );
        Ok(())
    }

    #[test]
    fn keeps_special_outliers_out_of_main_episode_template() -> Result<(), Box<dyn Error>> {
        let paths = vec![
            PathBuf::from("Show - 01 [1080p].mkv"),
            PathBuf::from("Show - OVA [1080p].mkv"),
            PathBuf::from("Show - 02 [1080p].mkv"),
        ];

        let parsed = parse_episode_batch(&paths);

        assert_eq!(
            parsed[0].parsed.as_ref().ok_or("episode 1")?.key,
            EpisodeKey::new(1, 1)
        );
        assert_eq!(parsed[1].status, ParseStatus::Rejected);
        assert!(parsed[1].notes.iter().any(|note| note.contains("特殊内容")));
        assert_eq!(
            parsed[2].parsed.as_ref().ok_or("episode 2")?.key,
            EpisodeKey::new(1, 2)
        );
        Ok(())
    }

    #[test]
    fn rejects_ambiguous_multi_number_slots() {
        let paths = vec![
            PathBuf::from("A-01-02-03.mkv"),
            PathBuf::from("A-02-03-04.mkv"),
            PathBuf::from("A-03-04-05.mkv"),
        ];

        let parsed = parse_episode_batch(&paths);
        assert!(parsed.iter().all(|decision| decision.parsed.is_none()));
        assert!(parsed
            .iter()
            .all(|decision| decision.status == ParseStatus::Ambiguous));
    }

    #[test]
    fn rejects_random_hash_like_names() {
        let paths = vec![
            PathBuf::from("a8f3c9.mkv"),
            PathBuf::from("B12FF0.mkv"),
            PathBuf::from("zz991q.mkv"),
        ];

        assert!(parse_episode_batch(&paths)
            .iter()
            .all(|decision| decision.parsed.is_none()));
    }

    #[test]
    fn avoids_resolution_codec_and_hash_false_positives() {
        let fixtures = [
            "Movie.1080p.HEVC-10bit.FLAC.mkv",
            "Archive.[2D6390A9].H.264.AAC.ass",
            "Show.2025.2160p.x265.mkv",
            "News.Show.2024.07.01.1080p.WEB.mkv",
        ];

        for name in fixtures {
            assert!(parse_episode(&PathBuf::from(name)).is_none());
        }
    }

    #[test]
    fn natural_sort_compares_numbers_by_value() {
        assert_eq!(
            natural_str_cmp("Show - 8.mkv", "Show - 10.mkv"),
            Ordering::Less
        );
        assert_eq!(
            natural_str_cmp("Show - 07.mkv", "Show - 8.mkv"),
            Ordering::Less
        );
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

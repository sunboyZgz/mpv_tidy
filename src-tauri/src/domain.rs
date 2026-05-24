use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeKey {
    pub season: u16,
    pub episode: u16,
}

impl EpisodeKey {
    pub fn new(season: u16, episode: u16) -> Self {
        Self { season, episode }
    }
}

impl fmt::Display for EpisodeKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "S{:02}E{:02}", self.season, self.episode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LanguageCode {
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "zh-Hant")]
    ZhHant,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "und")]
    Und,
}

impl LanguageCode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ZhHans => "zh-Hans",
            Self::ZhHant => "zh-Hant",
            Self::Ja => "ja",
            Self::En => "en",
            Self::Und => "und",
        }
    }

    pub fn default_preference() -> [Self; 4] {
        [Self::ZhHans, Self::ZhHant, Self::Ja, Self::En]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStatus {
    Matched,
    PendingFix,
    Conflict,
    Unprocessed,
    MissingVideo,
    MissingSub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrganizeMode {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleRole {
    Primary,
    Secondary,
    Candidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseStatus {
    Accepted,
    LowConfidence,
    Ambiguous,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseCandidateSource {
    Rule,
    Template,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseCandidate {
    pub episode: EpisodeKey,
    pub episode_key: String,
    pub confidence: u8,
    pub source: ParseCandidateSource,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileOperationKind {
    Video,
    Subtitle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollisionAction {
    Skip,
    Replace,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileOperationStatus {
    Planned,
    Copied,
    Moved,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub project_name: String,
    pub season: String,
    pub video_dirs: Vec<PathBuf>,
    pub subtitle_dirs: Vec<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub primary_language: LanguageCode,
    pub secondary_language: Option<LanguageCode>,
    pub mpv_path: PathBuf,
    pub extra_mpv_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanInput {
    pub video_dirs: Vec<PathBuf>,
    pub subtitle_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedVideo {
    pub path: PathBuf,
    pub file_name: String,
    pub extension: String,
    pub file_size_bytes: u64,
    pub episode: Option<EpisodeKey>,
    pub episode_key: Option<String>,
    pub confidence: u8,
    pub parse_status: ParseStatus,
    pub parse_notes: Vec<String>,
    pub parse_candidates: Vec<ParseCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedSubtitle {
    pub path: PathBuf,
    pub file_name: String,
    pub extension: String,
    pub file_size_bytes: u64,
    pub episode: Option<EpisodeKey>,
    pub episode_key: Option<String>,
    pub confidence: u8,
    pub parse_status: ParseStatus,
    pub parse_notes: Vec<String>,
    pub parse_candidates: Vec<ParseCandidate>,
    pub language: LanguageCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub videos: Vec<ScannedVideo>,
    pub subtitles: Vec<ScannedSubtitle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCandidate {
    pub path: PathBuf,
    pub file_name: String,
    pub extension: String,
    pub language: LanguageCode,
    pub confidence: u8,
    pub role: SubtitleRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeMatch {
    pub episode: EpisodeKey,
    pub episode_key: String,
    pub video: Option<ScannedVideo>,
    pub primary_subtitle: Option<SubtitleCandidate>,
    pub secondary_subtitle: Option<SubtitleCandidate>,
    pub candidates: Vec<SubtitleCandidate>,
    pub status: MatchStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanAndMatchResult {
    pub scan: ScanResult,
    pub matches: Vec<EpisodeMatch>,
    pub unprocessed_videos: Vec<ScannedVideo>,
    pub unprocessed_subtitles: Vec<ScannedSubtitle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildOrganizePlanRequest {
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub matches: Vec<EpisodeMatch>,
    pub mode: OrganizeMode,
    pub primary_language: LanguageCode,
    pub secondary_language: Option<LanguageCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub mode: OrganizeMode,
    pub items: Vec<OrganizePlanItem>,
    pub has_conflicts: bool,
    pub map_file_path: PathBuf,
    pub map_file_exists: bool,
    pub summary: OrganizePlanSummary,
    pub project_map: AnimeSubMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlanSummary {
    pub videos: usize,
    pub subtitles: usize,
    pub conflicts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlanItem {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub kind: FileOperationKind,
    pub episode_key: String,
    pub language: Option<LanguageCode>,
    pub role: Option<SubtitleRole>,
    pub collision: bool,
    pub collision_action: CollisionAction,
    pub status: FileOperationStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeExecutionResult {
    pub items: Vec<OrganizePlanItem>,
    pub map_written: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimeSubMap {
    pub app_version: String,
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub primary_language: LanguageCode,
    pub secondary_language: Option<LanguageCode>,
    pub episodes: Vec<AnimeSubMapEpisode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimeSubMapEpisode {
    pub episode_key: String,
    pub video: Option<PathBuf>,
    pub primary_subtitle: Option<PathBuf>,
    pub secondary_subtitle: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvLaunchRequest {
    pub mpv_path: PathBuf,
    pub video_path: PathBuf,
    pub primary_subtitle: Option<PathBuf>,
    pub secondary_subtitle: Option<PathBuf>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvLaunchResult {
    pub process_id: u32,
    pub argument_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEpisodeRecord {
    pub episode_key: String,
    pub video_path: Option<PathBuf>,
    pub primary_subtitle_path: Option<PathBuf>,
    pub secondary_subtitle_path: Option<PathBuf>,
    pub subtitle_count: usize,
    pub status: MatchStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLocalLibraryRequest {
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub mode: OrganizeMode,
    pub episodes: Vec<LibraryEpisodeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAnimeLibraryEntry {
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub mode: OrganizeMode,
    pub episode_count: usize,
    pub episodes: Vec<LibraryEpisodeRecord>,
    pub organized_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAnimeLibraryFile {
    pub app_version: String,
    pub entries: Vec<LocalAnimeLibraryEntry>,
}

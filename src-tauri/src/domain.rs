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
    Crf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseSlotLabel {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenFeatureKind {
    Alpha,
    Number,
    Separator,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenCompoundKind {
    SxxExx,
    VersionedEpisode,
    Resolution,
    Codec,
    Source,
    Language,
    Hash,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenFeatures {
    pub index: usize,
    pub text: String,
    pub lower: String,
    pub kind: TokenFeatureKind,
    pub compound_kind: Option<TokenCompoundKind>,
    pub number_value: Option<u32>,
    pub number_width: Option<usize>,
    pub previous_token: Option<String>,
    pub next_token: Option<String>,
    pub is_bracketed: bool,
    pub is_episode_marker_context: bool,
    pub is_season_marker_context: bool,
    pub is_quality_or_source: bool,
    pub is_language_token: bool,
    pub is_special_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabeledToken {
    pub features: TokenFeatures,
    pub label: ParseSlotLabel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseTrainingSampleSource {
    UserConfirmation,
    Fixture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseTrainingSample {
    pub schema_version: u16,
    pub source: ParseTrainingSampleSource,
    pub path: PathBuf,
    pub file_name: String,
    pub extension: String,
    pub confirmed_episode: Option<EpisodeKey>,
    pub note: Option<String>,
    pub tokens: Vec<LabeledToken>,
    pub created_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveParseTrainingSampleRequest {
    pub path: PathBuf,
    pub confirmed_episode: Option<EpisodeKey>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsStoragePaths {
    pub training_data_dir: PathBuf,
    pub training_sample_file: PathBuf,
    pub crf_model_file: PathBuf,
    pub app_settings_file: PathBuf,
    pub local_library_file: PathBuf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverStrategy {
    #[serde(rename = "local-first-then-screenshot")]
    #[default]
    LocalFirstThenScreenshot,
    #[serde(rename = "local-only")]
    LocalOnly,
    #[serde(rename = "screenshot-only")]
    ScreenshotOnly,
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WatchStatus {
    Watched,
    Partial,
    #[default]
    Unwatched,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitlePreferenceSnapshot {
    pub primary_language: LanguageCode,
    pub secondary_language: Option<LanguageCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u16,
    pub mpv_executable_path: PathBuf,
    pub default_output_dir: PathBuf,
    pub anime_library_root_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub default_primary_subtitle_language: LanguageCode,
    pub default_secondary_subtitle_language: LanguageCode,
    pub remember_playback_progress: bool,
    pub auto_scan_anime_library_on_startup: bool,
    pub auto_save_watch_progress: bool,
    pub default_cover_strategy: CoverStrategy,
    pub updated_at_unix: u64,
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
pub struct OrganizeProgressEvent {
    pub total: usize,
    pub processed: usize,
    pub current_episode_key: Option<String>,
    pub current_destination: Option<PathBuf>,
    pub status: FileOperationStatus,
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
    #[serde(default)]
    pub additional_subtitles: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvLaunchRequest {
    pub mpv_path: PathBuf,
    pub video_path: PathBuf,
    pub primary_subtitle: Option<PathBuf>,
    pub secondary_subtitle: Option<PathBuf>,
    pub primary_subtitle_delay_seconds: Option<f64>,
    pub secondary_subtitle_delay_seconds: Option<f64>,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvLaunchResult {
    pub process_id: u32,
    pub argument_count: usize,
    pub reused_existing: bool,
    pub switched_video: bool,
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
    #[serde(default)]
    pub watch_status: WatchStatus,
    #[serde(default)]
    pub last_position_sec: Option<u64>,
    #[serde(default)]
    pub progress_percent: Option<u8>,
    #[serde(default)]
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLocalLibraryRequest {
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub mode: OrganizeMode,
    pub episodes: Vec<LibraryEpisodeRecord>,
    pub subtitle_preference_snapshot: Option<SubtitlePreferenceSnapshot>,
    pub cover_strategy_snapshot: Option<CoverStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveLocalLibraryEntryRequest {
    pub entry_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAnimeLibraryEntry {
    #[serde(default)]
    pub id: String,
    pub project_name: String,
    pub season: String,
    pub output_dir: PathBuf,
    pub mode: OrganizeMode,
    pub episode_count: usize,
    #[serde(default)]
    pub subtitle_preference_snapshot: Option<SubtitlePreferenceSnapshot>,
    #[serde(default)]
    pub cover_strategy_snapshot: Option<CoverStrategy>,
    pub episodes: Vec<LibraryEpisodeRecord>,
    #[serde(default)]
    pub created_at_unix: u64,
    #[serde(default)]
    pub updated_at_unix: u64,
    #[serde(default)]
    pub organized_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAnimeLibraryFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub app_version: String,
    pub entries: Vec<LocalAnimeLibraryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLibraryEpisodeProgressRequest {
    pub entry_id: String,
    pub episode_key: String,
    pub watch_status: WatchStatus,
    pub last_position_sec: Option<u64>,
    pub progress_percent: Option<u8>,
}

fn default_schema_version() -> u16 {
    1
}

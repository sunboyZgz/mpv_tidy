export type LanguageCode = "zh-Hans" | "zh-Hant" | "ja" | "en" | "und";
export type MatchStatus =
  | "matched"
  | "pendingFix"
  | "conflict"
  | "unprocessed"
  | "missingVideo"
  | "missingSub";
export type OrganizeMode = "copy" | "move";
export type SubtitleRole = "primary" | "secondary" | "candidate";
export type CollisionAction = "skip" | "replace" | "rename";
export type ParseStatus = "accepted" | "lowConfidence" | "ambiguous" | "rejected";
export type ParseCandidateSource = "rule" | "template" | "crf";
export type CoverStrategy =
  | "local-first-then-screenshot"
  | "local-only"
  | "screenshot-only"
  | "disabled";
export type WatchStatus = "watched" | "partial" | "unwatched";
export type ParseSlotLabel =
  | "episode"
  | "season"
  | "version"
  | "hash"
  | "resolution"
  | "codec"
  | "source"
  | "language"
  | "title"
  | "noise"
  | "special"
  | "unknown";
export type TokenFeatureKind = "alpha" | "number" | "separator" | "other";
export type TokenCompoundKind =
  | "sxxExx"
  | "versionedEpisode"
  | "resolution"
  | "codec"
  | "source"
  | "language"
  | "hash";

export interface ProjectConfig {
  projectName: string;
  season: string;
  videoDirs: string[];
  subtitleDirs: string[];
  outputDir: string | null;
  primaryLanguage: LanguageCode;
  secondaryLanguage: LanguageCode | null;
  mpvPath: string;
  extraMpvArgs: string[];
}

export interface ScanInput {
  videoDirs: string[];
  subtitleDirs: string[];
}

export interface EpisodeKey {
  season: number;
  episode: number;
}

export interface ParseCandidate {
  episode: EpisodeKey;
  episodeKey: string;
  confidence: number;
  source: ParseCandidateSource;
  note: string;
}

export interface TokenFeatures {
  index: number;
  text: string;
  lower: string;
  kind: TokenFeatureKind;
  compoundKind: TokenCompoundKind | null;
  numberValue: number | null;
  numberWidth: number | null;
  previousToken: string | null;
  nextToken: string | null;
  isBracketed: boolean;
  isEpisodeMarkerContext: boolean;
  isSeasonMarkerContext: boolean;
  isQualityOrSource: boolean;
  isLanguageToken: boolean;
  isSpecialToken: boolean;
}

export interface LabeledToken {
  features: TokenFeatures;
  label: ParseSlotLabel;
}

export interface ParseTrainingSample {
  schemaVersion: number;
  source: "userConfirmation" | "fixture";
  path: string;
  fileName: string;
  extension: string;
  confirmedEpisode: EpisodeKey | null;
  note: string | null;
  tokens: LabeledToken[];
  createdAtUnix: number;
}

export interface SaveParseTrainingSampleRequest {
  path: string;
  confirmedEpisode: EpisodeKey | null;
  note: string | null;
}

export interface SettingsStoragePaths {
  trainingDataDir: string;
  trainingSampleFile: string;
  crfModelFile: string;
  appSettingsFile: string;
  localLibraryFile: string;
}

export interface AppSettings {
  schemaVersion: number;
  mpvExecutablePath: string;
  defaultOutputDir: string;
  animeLibraryRootDir: string;
  tempDir: string;
  defaultPrimarySubtitleLanguage: Exclude<LanguageCode, "und">;
  defaultSecondarySubtitleLanguage: Exclude<LanguageCode, "und">;
  rememberPlaybackProgress: boolean;
  autoScanAnimeLibraryOnStartup: boolean;
  autoSaveWatchProgress: boolean;
  defaultCoverStrategy: CoverStrategy;
  updatedAtUnix: number;
}

export interface SubtitlePreferenceSnapshot {
  primaryLanguage: LanguageCode;
  secondaryLanguage: LanguageCode | null;
}

export interface ScannedVideo {
  path: string;
  fileName: string;
  extension: string;
  fileSizeBytes: number;
  episode: EpisodeKey | null;
  episodeKey: string | null;
  confidence: number;
  parseStatus: ParseStatus;
  parseNotes: string[];
  parseCandidates: ParseCandidate[];
}

export interface ScannedSubtitle {
  path: string;
  fileName: string;
  extension: string;
  fileSizeBytes: number;
  episode: EpisodeKey | null;
  episodeKey: string | null;
  confidence: number;
  parseStatus: ParseStatus;
  parseNotes: string[];
  parseCandidates: ParseCandidate[];
  language: LanguageCode;
}

export interface SubtitleCandidate {
  path: string;
  fileName: string;
  extension: string;
  language: LanguageCode;
  confidence: number;
  role: SubtitleRole;
}

export interface EpisodeMatch {
  episode: EpisodeKey;
  episodeKey: string;
  video: ScannedVideo | null;
  primarySubtitle: SubtitleCandidate | null;
  secondarySubtitle: SubtitleCandidate | null;
  candidates: SubtitleCandidate[];
  status: MatchStatus;
  notes: string[];
}

export interface ScanResult {
  videos: ScannedVideo[];
  subtitles: ScannedSubtitle[];
}

export interface ScanAndMatchResult {
  scan: ScanResult;
  matches: EpisodeMatch[];
  unprocessedVideos: ScannedVideo[];
  unprocessedSubtitles: ScannedSubtitle[];
}

export interface BuildOrganizePlanRequest {
  projectName: string;
  season: string;
  outputDir: string;
  matches: EpisodeMatch[];
  mode: OrganizeMode;
  primaryLanguage: LanguageCode;
  secondaryLanguage: LanguageCode | null;
}

export interface OrganizePlanItem {
  source: string;
  destination: string;
  kind: "video" | "subtitle";
  episodeKey: string;
  language: LanguageCode | null;
  role: SubtitleRole | null;
  collision: boolean;
  collisionAction: CollisionAction;
  status: "planned" | "copied" | "moved" | "skipped" | "failed";
  message: string | null;
}

export interface OrganizePlan {
  projectName: string;
  season: string;
  outputDir: string;
  mode: OrganizeMode;
  items: OrganizePlanItem[];
  hasConflicts: boolean;
  mapFilePath: string;
  mapFileExists: boolean;
  summary: {
    videos: number;
    subtitles: number;
    conflicts: number;
  };
  projectMap: unknown;
}

export interface OrganizeExecutionResult {
  items: OrganizePlanItem[];
  mapWritten: boolean;
  message: string;
}

export interface OrganizeProgressEvent {
  total: number;
  processed: number;
  currentEpisodeKey: string | null;
  currentDestination: string | null;
  status: "planned" | "copied" | "moved" | "skipped" | "failed";
  message: string;
}

export interface LibraryEpisodeRecord {
  episodeKey: string;
  videoPath: string | null;
  primarySubtitlePath: string | null;
  secondarySubtitlePath: string | null;
  subtitleCount: number;
  status: MatchStatus;
  watchStatus: WatchStatus;
  lastPositionSec: number | null;
  progressPercent: number | null;
  updatedAtUnix: number;
}

export interface SaveLocalLibraryRequest {
  projectName: string;
  season: string;
  outputDir: string;
  mode: OrganizeMode;
  episodes: LibraryEpisodeRecord[];
  subtitlePreferenceSnapshot: SubtitlePreferenceSnapshot | null;
  coverStrategySnapshot: CoverStrategy | null;
}

export interface LocalAnimeLibraryEntry {
  id: string;
  projectName: string;
  season: string;
  outputDir: string;
  mode: OrganizeMode;
  episodeCount: number;
  subtitlePreferenceSnapshot: SubtitlePreferenceSnapshot | null;
  coverStrategySnapshot: CoverStrategy | null;
  episodes: LibraryEpisodeRecord[];
  createdAtUnix: number;
  updatedAtUnix: number;
  organizedAtUnix: number;
}

export interface LocalAnimeLibraryFile {
  schemaVersion: number;
  appVersion: string;
  entries: LocalAnimeLibraryEntry[];
}

export interface UpdateLibraryEpisodeProgressRequest {
  entryId: string;
  episodeKey: string;
  watchStatus: WatchStatus;
  lastPositionSec: number | null;
  progressPercent: number | null;
}

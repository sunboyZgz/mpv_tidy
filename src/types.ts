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
export type ParseCandidateSource = "rule" | "template";

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

export interface LibraryEpisodeRecord {
  episodeKey: string;
  videoPath: string | null;
  primarySubtitlePath: string | null;
  secondarySubtitlePath: string | null;
  subtitleCount: number;
  status: MatchStatus;
}

export interface SaveLocalLibraryRequest {
  projectName: string;
  season: string;
  outputDir: string;
  mode: OrganizeMode;
  episodes: LibraryEpisodeRecord[];
}

export interface LocalAnimeLibraryEntry {
  projectName: string;
  season: string;
  outputDir: string;
  mode: OrganizeMode;
  episodeCount: number;
  episodes: LibraryEpisodeRecord[];
  organizedAtUnix: number;
}

export interface LocalAnimeLibraryFile {
  appVersion: string;
  entries: LocalAnimeLibraryEntry[];
}

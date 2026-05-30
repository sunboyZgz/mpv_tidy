import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  BuildOrganizePlanRequest,
  AppSettings,
  LocalAnimeLibraryEntry,
  LocalAnimeLibraryFile,
  OrganizeExecutionResult,
  OrganizePlan,
  RemoveLocalLibraryEntryRequest,
  RepairLibraryEntryPathsRequest,
  RepairLibraryEntryPathsResult,
  SaveLocalLibraryRequest,
  SaveParseTrainingSampleRequest,
  ScanEmbeddedSubtitleTracksRequest,
  ScanEmbeddedSubtitleTracksResult,
  SettingsStoragePaths,
  ScanAndMatchResult,
  ScanInput,
  TokenFeatures,
  UpdateLibraryEpisodeProgressRequest,
} from "../types";

export interface MpvLaunchRequest {
  mpvPath: string;
  videoPath: string;
  primarySubtitle: string | null;
  secondarySubtitle: string | null;
  primaryEmbeddedSubtitleTrackId: number | null;
  secondaryEmbeddedSubtitleTrackId: number | null;
  primarySubtitleDelaySeconds: number | null;
  secondarySubtitleDelaySeconds: number | null;
  extraArgs: string[];
}

export interface MpvLaunchResult {
  processId: number;
  argumentCount: number;
  reusedExisting: boolean;
  switchedVideo: boolean;
}

export async function selectDirectory() {
  const selected = await open({ directory: true, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function selectFile() {
  const selected = await open({ directory: false, multiple: false });
  return typeof selected === "string" ? selected : null;
}

export async function selectDirectories() {
  const selected = await open({ directory: true, multiple: true });
  if (!selected) {
    return [];
  }
  return Array.isArray(selected) ? selected : [selected];
}

export function scanAndMatch(input: ScanInput) {
  return invoke<ScanAndMatchResult>("scan_and_match", { input });
}

export function buildOrganizePlan(request: BuildOrganizePlanRequest) {
  return invoke<OrganizePlan>("build_organize_plan", { request });
}

export function executeOrganizePlan(plan: OrganizePlan) {
  return invoke<OrganizeExecutionResult>("execute_organize_plan", { plan });
}

export function saveLocalLibraryEntry(request: SaveLocalLibraryRequest) {
  return invoke<LocalAnimeLibraryEntry>("save_local_library_entry", { request });
}

export function loadLocalLibrary() {
  return invoke<LocalAnimeLibraryFile>("load_local_library");
}

export function removeLocalLibraryEntry(request: RemoveLocalLibraryEntryRequest) {
  return invoke<LocalAnimeLibraryFile>("remove_local_library_entry", { request });
}

export function repairLibraryEntryPaths(request: RepairLibraryEntryPathsRequest) {
  return invoke<RepairLibraryEntryPathsResult>("repair_library_entry_paths", { request });
}

export function updateLibraryEpisodeProgress(request: UpdateLibraryEpisodeProgressRequest) {
  return invoke<LocalAnimeLibraryEntry>("update_library_episode_progress", { request });
}

export function scanEmbeddedSubtitleTracks(request: ScanEmbeddedSubtitleTracksRequest) {
  return invoke<ScanEmbeddedSubtitleTracksResult>("scan_embedded_subtitle_tracks", { request });
}

export function extractParseTokenFeatures(path: string) {
  return invoke<TokenFeatures[]>("extract_parse_token_features", { path });
}

export function saveParseTrainingSample(request: SaveParseTrainingSampleRequest) {
  return invoke("save_parse_training_sample", { request });
}

export function loadSettingsStoragePaths() {
  return invoke<SettingsStoragePaths>("settings_storage_paths");
}

export function loadAppSettings() {
  return invoke<AppSettings>("load_app_settings");
}

export function saveAppSettings(settings: AppSettings) {
  return invoke<AppSettings>("save_app_settings", { settings });
}

export function resetAppSettings() {
  return invoke<AppSettings>("reset_app_settings");
}

export function launchMpv(request: MpvLaunchRequest) {
  return invoke<MpvLaunchResult>("launch_mpv", { request });
}

export function revealPath(path: string) {
  return invoke("reveal_path", { path });
}

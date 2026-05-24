import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  BuildOrganizePlanRequest,
  LocalAnimeLibraryEntry,
  LocalAnimeLibraryFile,
  OrganizeExecutionResult,
  OrganizePlan,
  SaveLocalLibraryRequest,
  ScanAndMatchResult,
  ScanInput,
} from "../types";

export interface MpvLaunchRequest {
  mpvPath: string;
  videoPath: string;
  primarySubtitle: string | null;
  secondarySubtitle: string | null;
  extraArgs: string[];
}

export async function selectDirectory() {
  const selected = await open({ directory: true, multiple: false });
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

export function launchMpv(request: MpvLaunchRequest) {
  return invoke("launch_mpv", { request });
}

export function revealPath(path: string) {
  return invoke("reveal_path", { path });
}

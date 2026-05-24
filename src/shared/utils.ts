import type { LanguageCode } from "../types";

export const browserPreviewMessage = "当前是浏览器预览；真实扫描、整理和保存请在 Tauri 桌面窗口中执行。";

export const languageLabels: Record<LanguageCode, string> = {
  "zh-Hans": "zh-Hans",
  "zh-Hant": "zh-Hant",
  ja: "ja",
  en: "en",
  und: "und",
};

export function asset(path: string) {
  return `/assets/${path}`;
}

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export function unique<T>(values: T[]) {
  return [...new Set(values)];
}

export function chipClass(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-");
}

export function formatBytes(bytes: number) {
  if (bytes <= 0) {
    return "未知";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

export function formatDuration(seconds?: number) {
  if (!seconds || seconds <= 0) {
    return "未知";
  }
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
}

export function fileNameFromPath(path?: string) {
  if (!path) {
    return "";
  }
  return path.split(/[\\/]/).pop() ?? path;
}

export function roundOffset(value: number) {
  return Math.round(value * 10) / 10;
}

export function splitTextList(value: string) {
  return value
    .split(/[、,，/]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

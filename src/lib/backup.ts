/** Typed wrappers over the Rust side, plus a guard for running in a plain
 *  browser (the screenshot script and `vite dev` without Tauri). */

export interface BackupSummary {
  path: string;
  encrypted: boolean;
  deviceName: string | null;
  productType: string | null;
  iosVersion: string | null;
  serialNumber: string | null;
  lastBackup: string | null;
}

export interface CategoryCount {
  category: string;
  files: number;
}

export interface OpenedBackup {
  encrypted: boolean;
  fileCount: number;
  categories: CategoryCount[];
}

/** True when running inside the desktop shell rather than a bare browser. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Ask for a folder. Returns null if the user cancelled. */
export async function chooseFolder(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title: "Choose a backup folder" });
  return typeof picked === "string" ? picked : null;
}

/** Identify a folder. Resolves to null when it is not a backup at all. */
export function inspectBackup(path: string) {
  return invoke<BackupSummary | null>("inspect_backup", { path });
}

/** Open a backup. `password` is the backup password, not the device passcode. */
export function openBackup(path: string, password?: string) {
  return invoke<OpenedBackup>("open_backup", { path, password: password || null });
}

export function closeBackup() {
  return invoke<void>("close_backup");
}

/* ---- Photos ----------------------------------------------------------- */

export interface Photo {
  id: string;
  filename: string;
  /** Capture time as Unix seconds, when the backup records one. */
  created: number | null;
  width: number | null;
  height: number | null;
  ext: string;
  size: number;
}

export function listPhotos() {
  return invoke<Photo[]>("list_photos");
}

/**
 * Base URL for the image streaming scheme.
 *
 * Windows serves custom schemes over http://<scheme>.localhost; every other
 * platform uses <scheme>://localhost. Images go through this rather than IPC:
 * a 12 MP photo is several megabytes, and base64 over IPC would inflate it by
 * a third and hold the whole string in memory on both sides.
 */
const IMAGE_BASE =
  typeof navigator !== "undefined" && navigator.userAgent.includes("Windows")
    ? "http://archive.localhost"
    : "archive://localhost";

/** Grid thumbnail: generated on request, then cached on disk. */
export function thumbUrl(id: string) {
  return `${IMAGE_BASE}/thumb/${id}`;
}

/** Full-size image, transcoded only when the webview cannot render it. */
export function fullUrl(id: string) {
  return `${IMAGE_BASE}/full/${id}`;
}

/* ---- Thumbnail cache -------------------------------------------------- */

export interface CacheInfo {
  bytes: number;
  limit: number;
  location: string;
}

export function cacheInfo() {
  return invoke<CacheInfo>("cache_info");
}

/** Returns the number of bytes freed. */
export function clearCache() {
  return invoke<number>("clear_cache");
}

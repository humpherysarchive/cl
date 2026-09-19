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

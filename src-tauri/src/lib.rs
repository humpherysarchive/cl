//! Application entry point and the commands the interface calls.
//!
//! An opened backup is held in managed state rather than reopened per call:
//! unlocking an encrypted backup costs two PBKDF2 passes over ~10k iterations,
//! and the class keys have to stay in memory to read any file at all.

mod ios;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

use ios::{Backup, BackupSummary, CategoryCount};

#[derive(Default)]
struct AppState {
    backup: Mutex<Option<Backup>>,
}

/// What the interface learns once a backup is open.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedBackup {
    encrypted: bool,
    file_count: usize,
    categories: Vec<CategoryCount>,
}

/// Identify a folder without needing a password.
///
/// Returns `null` when the folder simply is not a backup, so the interface can
/// say "that doesn't look like a backup" instead of showing an error.
#[tauri::command]
async fn inspect_backup(path: String) -> Result<Option<BackupSummary>, String> {
    ios::inspect(&path).map_err(|e| e.to_string())
}

/// Open a backup, unlocking it when encrypted.
///
/// `password` is the backup password set by "Encrypt local backup" — not the
/// device passcode and not an Apple account password.
#[tauri::command]
async fn open_backup(
    path: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<OpenedBackup, String> {
    let backup =
        Backup::open(&PathBuf::from(&path), password.as_deref()).map_err(|e| e.to_string())?;

    let opened = OpenedBackup {
        encrypted: backup.is_encrypted(),
        file_count: backup.files().len(),
        categories: backup.category_counts(),
    };

    *state.backup.lock().unwrap() = Some(backup);
    Ok(opened)
}

/// Write one file out of the backup, decrypting it on the way.
///
/// `domain` and `path` are the location the file had on the phone, as listed
/// in the backup's manifest.
#[tauri::command]
async fn export_file(
    domain: String,
    path: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    let guard = state.backup.lock().unwrap();
    let backup = guard.as_ref().ok_or("no backup is open")?;

    let file = backup
        .find(&domain, &path)
        .ok_or_else(|| format!("{domain}/{path} is not in this backup"))?;
    let bytes = backup.read_file(file).map_err(|e| e.to_string())?;

    let destination = PathBuf::from(destination);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&destination, &bytes).map_err(|e| e.to_string())?;
    Ok(bytes.len() as u64)
}

/// Drop the open backup, and with it the decrypted class keys.
#[tauri::command]
async fn close_backup(state: State<'_, AppState>) -> Result<(), String> {
    *state.backup.lock().unwrap() = None;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            inspect_backup,
            open_backup,
            export_file,
            close_backup
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}

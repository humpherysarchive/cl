//! Application entry point and the commands the interface calls.
//!
//! An opened backup is held in managed state rather than reopened per call:
//! unlocking an encrypted backup costs two PBKDF2 passes over ~10k iterations,
//! and the class keys have to stay in memory to read any file at all. It is
//! held behind an `Arc` so the image protocol can take a reference and release
//! the lock before doing several hundred milliseconds of decoding.

mod ios;
mod protocol;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{Manager, State};

use ios::{thumbs, Backup, BackupSummary, CategoryCount, Photo};

pub struct AppState {
    pub backup: Mutex<Option<Arc<Backup>>>,
    /// Thumbnail cache ceiling in bytes, adjustable from Settings.
    cache_limit: AtomicU64,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            backup: Mutex::new(None),
            cache_limit: AtomicU64::new(thumbs::DEFAULT_CACHE_LIMIT_BYTES),
        }
    }
}

impl AppState {
    pub fn cache_limit(&self) -> u64 {
        self.cache_limit.load(Ordering::Relaxed)
    }

    /// Where this backup's thumbnails live. Under the OS cache directory —
    /// never inside the backup folder.
    pub fn cache_root(&self, app: &tauri::AppHandle, backup: &Backup) -> PathBuf {
        let base = app
            .path()
            .app_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("cl-cache"));
        thumbs::cache_dir(&base, &backup.cache_key())
    }
}

/// What the interface learns once a backup is open.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedBackup {
    encrypted: bool,
    file_count: usize,
    categories: Vec<CategoryCount>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheInfo {
    bytes: u64,
    limit: u64,
    /// Shown in Settings so it is clear what "Clear cache" will remove.
    location: String,
}

/// Identify a folder without needing a password.
#[tauri::command]
async fn inspect_backup(path: String) -> Result<Option<BackupSummary>, String> {
    ios::inspect(&path).map_err(|e| e.to_string())
}

/// Open a backup, unlocking it when encrypted.
///
/// `password` is the backup password set by "Encrypt local backup" — not the
/// device passcode and not an Apple account password. It is used here and
/// never stored.
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

    *state.backup.lock().unwrap() = Some(Arc::new(backup));
    Ok(opened)
}

/// The camera roll, newest first. Images themselves are fetched over the
/// `archive://` scheme rather than returned here.
#[tauri::command]
async fn list_photos(state: State<'_, AppState>) -> Result<Vec<Photo>, String> {
    let backup = {
        let guard = state.backup.lock().unwrap();
        guard.as_ref().map(Arc::clone).ok_or("no backup is open")?
    };
    backup
        .photos()
        .map(<[Photo]>::to_vec)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cache_info(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<CacheInfo, String> {
    let backup = {
        let guard = state.backup.lock().unwrap();
        guard.as_ref().map(Arc::clone)
    };
    let root = match &backup {
        Some(b) => state.cache_root(&app, b),
        None => app
            .path()
            .app_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("cl-cache"))
            .join("thumbnails"),
    };
    Ok(CacheInfo {
        bytes: thumbs::cache_size(&root),
        limit: state.cache_limit(),
        location: root.display().to_string(),
    })
}

/// Delete every cached thumbnail for the open backup. Returns bytes freed.
#[tauri::command]
async fn clear_cache(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let backup = {
        let guard = state.backup.lock().unwrap();
        guard.as_ref().map(Arc::clone)
    };
    let root = match &backup {
        Some(b) => state.cache_root(&app, b),
        None => return Ok(0),
    };
    Ok(thumbs::clear_cache(&root))
}

/// Write one file out of the backup, decrypting it on the way.
#[tauri::command]
async fn export_file(
    domain: String,
    path: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    let backup = {
        let guard = state.backup.lock().unwrap();
        guard.as_ref().map(Arc::clone).ok_or("no backup is open")?
    };

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
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .setup(|app| {
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            inspect_backup,
            open_backup,
            list_photos,
            cache_info,
            clear_cache,
            export_file,
            close_backup
        ])
        .run(tauri::generate_context!())
        .expect("error while running application");
}

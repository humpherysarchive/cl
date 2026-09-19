//! Application entry point.
//!
//! The importer lives behind commands invoked from the UI; for now the shell
//! only hosts the window so the interface can be reviewed on its own.

mod ios;

use ios::BackupSummary;

/// Inspect a folder and report whether it looks like an iOS backup, and what
/// it contains. Returns `Ok(None)` when the path is not a backup at all.
#[tauri::command]
async fn inspect_backup(path: String) -> Result<Option<BackupSummary>, String> {
    ios::inspect(&path).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![inspect_backup])
        .run(tauri::generate_context!())
        .expect("error while running application");
}

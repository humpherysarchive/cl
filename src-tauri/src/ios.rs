//! iOS backup detection.
//!
//! A backup directory is identified by the files Finder/iTunes always writes
//! at its root. Parsing of the individual data classes is not implemented yet;
//! this module currently answers "is this a backup, and is it encrypted?" so
//! the import flow can tell the user what it found before doing any work.

use serde::Serialize;
use std::fmt;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    /// Absolute path the summary describes.
    pub path: String,
    /// Encrypted backups need the user's backup password before anything can
    /// be read, including the file manifest.
    pub encrypted: bool,
    /// Present only for unencrypted backups, where the plist is readable.
    pub device_name: Option<String>,
    pub ios_version: Option<String>,
    pub last_backup: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "could not read the backup folder: {e}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Every iOS backup root carries these three files.
const REQUIRED: [&str; 3] = ["Manifest.plist", "Info.plist", "Status.plist"];

pub fn inspect(path: &str) -> Result<Option<BackupSummary>, Error> {
    let dir = Path::new(path);
    if !dir.is_dir() {
        return Ok(None);
    }
    if !REQUIRED.iter().all(|f| dir.join(f).is_file()) {
        return Ok(None);
    }

    // TODO: read Manifest.plist for the IsEncrypted flag and Info.plist for
    // the device fields. Until the plist reader lands, report the backup as
    // found with unknown details rather than guessing at them.
    Ok(Some(BackupSummary {
        path: dir.display().to_string(),
        encrypted: false,
        device_name: None,
        ios_version: None,
        last_backup: None,
    }))
}

//! Reading the backup's own bookkeeping.
//!
//! `Manifest.plist` sits in the clear and says whether the backup is
//! encrypted, carrying the keybag and — on iOS 10 and later — the wrapped key
//! for `Manifest.db` itself. `Manifest.db` is the SQLite index of every file:
//! its domain, its path on the phone, and a binary plist holding its own
//! wrapped key. Files live on disk at `<first two hex chars>/<fileID>`.

use std::fs;
use std::path::{Path, PathBuf};

use plist::{Dictionary, Value};

use super::crypto::decrypt_cbc;
use super::keybag::{Keybag, ProtectionClass, UnlockedKeybag};
use super::Error;

/// What `Manifest.plist` tells us before any password is supplied.
pub struct ManifestPlist {
    pub encrypted: bool,
    pub keybag: Option<Vec<u8>>,
    /// iOS 10+ only; older encrypted backups leave `Manifest.db` in the clear.
    pub manifest_key: Option<(ProtectionClass, Vec<u8>)>,
}

impl ManifestPlist {
    pub fn read(root: &Path) -> Result<Self, Error> {
        let value: Value = plist::from_file(root.join("Manifest.plist"))
            .map_err(|e| Error::Plist("Manifest.plist", e.to_string()))?;
        let dict = value
            .as_dictionary()
            .ok_or_else(|| Error::Plist("Manifest.plist", "not a dictionary".into()))?;

        let encrypted = dict
            .get("IsEncrypted")
            .and_then(|v| v.as_boolean())
            .unwrap_or(false);

        let keybag = dict
            .get("BackupKeyBag")
            .and_then(as_data)
            .map(<[u8]>::to_vec);

        // The blob is a little-endian class number followed by the wrapped key.
        let manifest_key = dict.get("ManifestKey").and_then(as_data).and_then(|d| {
            if d.len() < 4 {
                return None;
            }
            let class = u32::from_le_bytes(d[..4].try_into().ok()?);
            Some((class, d[4..].to_vec()))
        });

        Ok(ManifestPlist {
            encrypted,
            keybag,
            manifest_key,
        })
    }

    pub fn parse_keybag(&self) -> Result<Keybag, Error> {
        let blob = self
            .keybag
            .as_deref()
            .ok_or(Error::MalformedKeybag("Manifest.plist has no BackupKeyBag"))?;
        Keybag::parse(blob)
    }
}

/// Device details, read from `Info.plist`, which stays readable even when the
/// backup is encrypted. This is what lets the UI name the phone before asking
/// for a password.
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub device_name: Option<String>,
    pub product_type: Option<String>,
    pub ios_version: Option<String>,
    pub serial_number: Option<String>,
    pub last_backup: Option<String>,
}

impl DeviceInfo {
    pub fn read(root: &Path) -> Result<Self, Error> {
        let value: Value = plist::from_file(root.join("Info.plist"))
            .map_err(|e| Error::Plist("Info.plist", e.to_string()))?;
        let Some(dict) = value.as_dictionary() else {
            return Ok(DeviceInfo::default());
        };

        let s = |k: &str| dict.get(k).and_then(|v| v.as_string()).map(str::to_owned);
        Ok(DeviceInfo {
            device_name: s("Device Name").or_else(|| s("Display Name")),
            product_type: s("Product Type"),
            ios_version: s("Product Version"),
            serial_number: s("Serial Number"),
            last_backup: dict
                .get("Last Backup Date")
                .and_then(|v| v.as_date())
                .map(|d| d.to_xml_format()),
        })
    }
}

/// One row of `Manifest.db`.
#[derive(Debug, Clone)]
pub struct BackupFile {
    pub file_id: String,
    pub domain: String,
    pub relative_path: String,
    pub size: u64,
    pub is_directory: bool,
    /// Absent in an unencrypted backup.
    pub encryption: Option<(ProtectionClass, Vec<u8>)>,
}

impl BackupFile {
    /// Where this file's contents sit inside the backup directory.
    pub fn path_in(&self, root: &Path) -> PathBuf {
        root.join(&self.file_id[..2]).join(&self.file_id)
    }
}

/// The decrypted file index.
pub struct Manifest {
    pub files: Vec<BackupFile>,
}

impl Manifest {
    /// Open `Manifest.db`, decrypting it first when the backup requires it.
    ///
    /// SQLite cannot read from memory, so an encrypted manifest is written to
    /// a temporary file that is removed as soon as the rows are read. The
    /// plaintext never lands in the backup directory or anywhere the user
    /// would have to clean up by hand.
    pub fn read(
        root: &Path,
        plist: &ManifestPlist,
        keybag: Option<&UnlockedKeybag>,
    ) -> Result<Self, Error> {
        let db_path = root.join("Manifest.db");

        let files = match (plist.manifest_key.as_ref(), keybag) {
            (Some((class, wrapped)), Some(bag)) => {
                let key = bag.unwrap_key(*class, wrapped)?;
                let ciphertext = fs::read(&db_path)?;
                let plain = decrypt_cbc(&key, &ciphertext, None)?;

                let tmp = tempfile::Builder::new()
                    .prefix("cl-manifest-")
                    .suffix(".db")
                    .tempfile()?;
                fs::write(tmp.path(), &plain)?;
                let rows = read_rows(tmp.path())?;
                drop(tmp); // removes the plaintext
                rows
            }
            // Unencrypted, or an older encrypted backup whose manifest is
            // plaintext even though its file contents are not.
            _ => read_rows(&db_path)?,
        };

        Ok(Manifest { files })
    }
}

fn read_rows(db: &Path) -> Result<Vec<BackupFile>, Error> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let mut stmt = conn.prepare("SELECT fileID, domain, relativePath, flags, file FROM Files")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<Vec<u8>>>(4)?,
        ))
    })?;

    let mut files = Vec::new();
    for row in rows {
        let (file_id, domain, relative_path, flags, blob) = row?;
        // A fileID is a 40-character SHA-1 hex digest; anything else would
        // break the two-character directory split below.
        if file_id.len() < 2 {
            continue;
        }

        let meta = blob
            .as_deref()
            .map(parse_file_blob)
            .transpose()?
            .unwrap_or_default();
        files.push(BackupFile {
            file_id,
            domain,
            relative_path,
            size: meta.size,
            is_directory: flags == 2,
            encryption: meta.encryption,
        });
    }
    Ok(files)
}

#[derive(Default)]
struct FileMeta {
    size: u64,
    encryption: Option<(ProtectionClass, Vec<u8>)>,
}

/// Parse the `file` column: an NSKeyedArchiver plist whose root object is the
/// MBFile record. Object references are `$objects` indices, so the encryption
/// key has to be followed one hop.
fn parse_file_blob(blob: &[u8]) -> Result<FileMeta, Error> {
    let value: Value = plist::from_bytes(blob)
        .map_err(|e| Error::Plist("Manifest.db file record", e.to_string()))?;
    let Some(dict) = value.as_dictionary() else {
        return Ok(FileMeta::default());
    };

    let objects = match dict.get("$objects").and_then(|v| v.as_array()) {
        Some(o) => o,
        None => return Ok(FileMeta::default()),
    };
    let root_idx = dict
        .get("$top")
        .and_then(|v| v.as_dictionary())
        .and_then(|t| t.get("root"))
        .and_then(as_uid)
        .unwrap_or(1) as usize;

    let Some(mb_file) = objects.get(root_idx).and_then(|v| v.as_dictionary()) else {
        return Ok(FileMeta::default());
    };

    let size = mb_file
        .get("Size")
        .and_then(|v| v.as_signed_integer())
        .filter(|n| *n >= 0)
        .map(|n| n as u64)
        .unwrap_or(0);

    let class = mb_file
        .get("ProtectionClass")
        .and_then(|v| v.as_signed_integer())
        .unwrap_or(0) as ProtectionClass;

    let encryption = mb_file
        .get("EncryptionKey")
        .and_then(as_uid)
        .and_then(|idx| objects.get(idx as usize))
        .and_then(|v| v.as_dictionary())
        .and_then(ns_data)
        // The first four bytes repeat the protection class; the key follows.
        .filter(|d| d.len() > 4)
        .map(|d| (class, d[4..].to_vec()));

    Ok(FileMeta { size, encryption })
}

fn as_data(v: &Value) -> Option<&[u8]> {
    v.as_data()
}

fn as_uid(v: &Value) -> Option<u64> {
    v.as_uid().map(|u| u.get())
}

fn ns_data(d: &Dictionary) -> Option<&[u8]> {
    d.get("NS.data").and_then(as_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_blob_without_a_key_is_not_an_error() {
        // An empty plist dictionary: valid, but carries none of the fields.
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &Dictionary::new()).unwrap();

        let meta = parse_file_blob(&buf).unwrap();
        assert_eq!(meta.size, 0);
        assert!(meta.encryption.is_none());
    }

    #[test]
    fn a_corrupt_file_blob_reports_rather_than_panics() {
        assert!(parse_file_blob(b"not a plist at all").is_err());
    }

    #[test]
    fn file_contents_live_under_the_first_two_hex_characters() {
        let f = BackupFile {
            file_id: "3d0d7e5fb2ce288813306e4d4636395e047a3d28".into(),
            domain: "HomeDomain".into(),
            relative_path: "Library/SMS/sms.db".into(),
            size: 0,
            is_directory: false,
            encryption: None,
        };
        assert_eq!(
            f.path_in(Path::new("/b")),
            Path::new("/b/3d/3d0d7e5fb2ce288813306e4d4636395e047a3d28")
        );
    }
}

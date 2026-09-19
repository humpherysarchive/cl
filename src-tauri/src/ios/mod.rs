//! Reading an iOS backup.
//!
//! The flow the UI drives is deliberately two-step. [`inspect`] answers "is
//! this a backup, whose phone is it, and does it need a password?" without
//! asking for anything — an encrypted backup still names its device, because
//! `Info.plist` is left in the clear. [`Backup::open`] then does the work that
//! needs the password.
//!
//! Encrypted backups are the interesting case: Health data, saved passwords,
//! Wi-Fi settings and call history are *only* present when the user ticked
//! "Encrypt local backup", so the encrypted path is the one that matters for
//! actually recovering someone's data, not an optional extra.

mod crypto;
mod keybag;
mod manifest;

use std::path::{Path, PathBuf};

use serde::Serialize;

use keybag::UnlockedKeybag;
pub use manifest::{BackupFile, DeviceInfo};
use manifest::{Manifest, ManifestPlist};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read the backup: {0}")]
    Io(#[from] std::io::Error),

    #[error("the backup's file index could not be read: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("{0} is damaged or not in the expected format: {1}")]
    Plist(&'static str, String),

    #[error("that backup password is not correct")]
    WrongPassword,

    #[error("this backup is encrypted and needs its backup password")]
    PasswordRequired,

    #[error("this folder is not an iPhone backup")]
    NotABackup,

    #[error("the backup's keybag is damaged: {0}")]
    MalformedKeybag(&'static str),

    /// The backup references a protection class its keybag does not carry.
    #[error("this backup is missing the key for protection class {0}")]
    MissingClassKey(u32),

    #[error("{0}")]
    Crypto(&'static str),

    /// Internal: an RFC 3394 integrity check failed. Callers turn this into
    /// [`Error::WrongPassword`] where that is what it means.
    #[error("the key could not be unwrapped")]
    WrongKey,
}

/// Every iOS backup root carries these files.
const REQUIRED: [&str; 2] = ["Manifest.plist", "Info.plist"];

/// What can be learned about a backup before a password is supplied.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub path: String,
    pub encrypted: bool,
    #[serde(flatten)]
    pub device: DeviceInfo,
}

/// Identify a folder. Returns `Ok(None)` when it simply is not a backup, which
/// is a normal answer rather than an error — the user may have picked the
/// wrong directory.
pub fn inspect(path: &str) -> Result<Option<BackupSummary>, Error> {
    let root = Path::new(path);
    if !root.is_dir() || !REQUIRED.iter().all(|f| root.join(f).is_file()) {
        return Ok(None);
    }

    let plist = ManifestPlist::read(root)?;
    Ok(Some(BackupSummary {
        path: root.display().to_string(),
        encrypted: plist.encrypted,
        // Readable even on an encrypted backup, so the user can confirm they
        // picked the right phone before typing a password.
        device: DeviceInfo::read(root).unwrap_or_default(),
    }))
}

/// An opened backup: file index decrypted, class keys held in memory.
pub struct Backup {
    root: PathBuf,
    files: Vec<BackupFile>,
    keybag: Option<UnlockedKeybag>,
}

/// Written by hand rather than derived: this struct holds decrypted class
/// keys, and a derived `Debug` would put them in any log line or panic
/// message that formatted a `Backup`.
impl std::fmt::Debug for Backup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backup")
            .field("root", &self.root)
            .field("files", &self.files.len())
            .field("encrypted", &self.keybag.is_some())
            .finish_non_exhaustive()
    }
}

impl Backup {
    /// Open a backup, unlocking it when encrypted.
    ///
    /// `password` is the user's *backup* password — the one set by "Encrypt
    /// local backup", which is not the device passcode and not their Apple
    /// account password.
    pub fn open(path: &Path, password: Option<&str>) -> Result<Self, Error> {
        if !path.is_dir() || !REQUIRED.iter().all(|f| path.join(f).is_file()) {
            return Err(Error::NotABackup);
        }

        let plist = ManifestPlist::read(path)?;
        let keybag = if plist.encrypted {
            let password = password.ok_or(Error::PasswordRequired)?;
            Some(plist.parse_keybag()?.unlock(password)?)
        } else {
            None
        };

        let manifest = Manifest::read(path, &plist, keybag.as_ref())?;
        Ok(Backup {
            root: path.to_path_buf(),
            files: manifest.files,
            keybag,
        })
    }

    pub fn files(&self) -> &[BackupFile] {
        &self.files
    }

    pub fn is_encrypted(&self) -> bool {
        self.keybag.is_some()
    }

    /// Read and decrypt one file's contents.
    pub fn read_file(&self, file: &BackupFile) -> Result<Vec<u8>, Error> {
        if file.is_directory {
            return Ok(Vec::new());
        }
        let raw = std::fs::read(file.path_in(&self.root))?;

        match (&file.encryption, &self.keybag) {
            (Some((class, wrapped)), Some(bag)) => {
                let key = bag.unwrap_key(*class, wrapped)?;
                crypto::decrypt_cbc(&key, &raw, Some(file.size))
            }
            _ => Ok(raw),
        }
    }

    /// Locate one file by the domain and path it had on the phone.
    pub fn find(&self, domain: &str, relative_path: &str) -> Option<&BackupFile> {
        self.files
            .iter()
            .find(|f| f.domain == domain && f.relative_path == relative_path)
    }

    /// How many files each category has, for the import screen.
    pub fn category_counts(&self) -> Vec<CategoryCount> {
        let mut counts: std::collections::HashMap<&'static str, u64> = Default::default();
        for file in &self.files {
            if file.is_directory {
                continue;
            }
            if let Some(cat) = classify(&file.domain, &file.relative_path) {
                *counts.entry(cat).or_default() += 1;
            }
        }
        let mut out: Vec<_> = counts
            .into_iter()
            .map(|(category, files)| CategoryCount {
                category: category.into(),
                files,
            })
            .collect();
        out.sort_by(|a, b| a.category.cmp(&b.category));
        out
    }
}

#[derive(Debug, Serialize)]
pub struct CategoryCount {
    pub category: String,
    pub files: u64,
}

/// Route a backup entry to the sidebar category it belongs to.
///
/// Ids match `src/lib/categories.ts`. Returning `None` means the file is
/// plumbing the user has no reason to see.
pub fn classify(domain: &str, path: &str) -> Option<&'static str> {
    // Camera roll: split by extension so Photos and Videos are separate the
    // way they are in the sidebar.
    if domain == "CameraRollDomain" && path.starts_with("Media/DCIM/") {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        return Some(match ext.as_str() {
            "mov" | "mp4" | "m4v" | "avi" => "videos",
            _ => "photos",
        });
    }

    if domain == "HealthDomain" {
        return Some("health");
    }

    if domain == "HomeDomain" {
        return Some(match path {
            p if p.starts_with("Library/SMS/") => "messages",
            p if p.starts_with("Library/AddressBook/") => "contacts",
            p if p.starts_with("Library/CallHistoryDB/") => "calls",
            p if p.starts_with("Library/Voicemail/") => "voicemail",
            p if p.starts_with("Library/Mail/") => "mail",
            p if p.starts_with("Library/Calendar/") => "calendar",
            p if p.starts_with("Library/Safari/") => "safari",
            p if p.starts_with("Library/Passes/") => "wallet",
            _ => return None,
        });
    }

    if domain == "MediaDomain" {
        return Some(match path {
            p if p.starts_with("Media/Recordings/") => "voicememos",
            p if p.starts_with("Media/iTunes_Control/") => "music",
            _ => return None,
        });
    }

    if domain == "RootDomain" && path.starts_with("Library/Caches/locationd/") {
        return Some("locations");
    }

    // App group containers hold the modern Notes, Reminders and Files stores.
    if let Some(group) = domain.strip_prefix("AppDomainGroup-group.com.apple.") {
        return Some(match group {
            "notes" => "notes",
            "reminders" => "reminders",
            "VoiceMemos" | "VoiceMemos.shared" => "voicememos",
            _ if group.starts_with("FileProvider") => "files",
            _ => return None,
        });
    }

    // Anything else an installed app wrote.
    if domain.starts_with("AppDomain-") {
        return Some("apps");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_files_to_their_category() {
        let cases = [
            (
                "CameraRollDomain",
                "Media/DCIM/100APPLE/IMG_0001.HEIC",
                Some("photos"),
            ),
            (
                "CameraRollDomain",
                "Media/DCIM/100APPLE/IMG_0002.JPG",
                Some("photos"),
            ),
            (
                "CameraRollDomain",
                "Media/DCIM/100APPLE/IMG_0003.MOV",
                Some("videos"),
            ),
            ("CameraRollDomain", "Media/PhotoData/Photos.sqlite", None),
            ("HomeDomain", "Library/SMS/sms.db", Some("messages")),
            (
                "HomeDomain",
                "Library/SMS/Attachments/00/00/x.jpg",
                Some("messages"),
            ),
            (
                "HomeDomain",
                "Library/AddressBook/AddressBook.sqlitedb",
                Some("contacts"),
            ),
            (
                "HomeDomain",
                "Library/CallHistoryDB/CallHistory.storedata",
                Some("calls"),
            ),
            ("HomeDomain", "Library/Safari/History.db", Some("safari")),
            (
                "HomeDomain",
                "Library/Preferences/com.apple.foo.plist",
                None,
            ),
            (
                "HealthDomain",
                "Health/healthdb_secure.sqlite",
                Some("health"),
            ),
            (
                "MediaDomain",
                "Media/Recordings/memo.m4a",
                Some("voicememos"),
            ),
            (
                "MediaDomain",
                "Media/iTunes_Control/Music/F00/x.m4a",
                Some("music"),
            ),
            (
                "RootDomain",
                "Library/Caches/locationd/cache.db",
                Some("locations"),
            ),
            (
                "AppDomainGroup-group.com.apple.notes",
                "NoteStore.sqlite",
                Some("notes"),
            ),
            (
                "AppDomainGroup-group.com.apple.reminders",
                "Data.sqlite",
                Some("reminders"),
            ),
            ("AppDomain-com.spotify.client", "Documents/x", Some("apps")),
            (
                "SystemPreferencesDomain",
                "SystemConfiguration/x.plist",
                None,
            ),
        ];
        for (domain, path, want) in cases {
            assert_eq!(classify(domain, path), want, "{domain} / {path}");
        }
    }

    #[test]
    fn extension_matching_is_case_insensitive() {
        let base = "Media/DCIM/100APPLE/IMG_0001";
        for ext in ["MOV", "mov", "Mov", "MP4"] {
            assert_eq!(
                classify("CameraRollDomain", &format!("{base}.{ext}")),
                Some("videos")
            );
        }
    }

    // ---- End-to-end: a synthetic encrypted backup ----------------------
    //
    // Builds a backup the way iOS 10.2+ writes one — keybag, wrapped manifest
    // key, AES-CBC encrypted Manifest.db, per-file wrapped keys — and reads a
    // file back out through the public API. This covers the whole chain:
    // password -> keybag key -> class key -> manifest key -> file key -> bytes.
    //
    // It proves the pieces agree with each other. It cannot prove they agree
    // with Apple; only a real backup does that.

    use crate::ios::crypto::{aes_wrap_key, encrypt_cbc};
    use hmac::Hmac;
    use plist::{Dictionary, Uid, Value};
    use sha1::Sha1;
    use sha2::Sha256;
    use std::path::Path;

    const PASSWORD: &str = "correct horse battery staple";
    const CLASS: u32 = 3;
    const SALT: &[u8] = b"0123456789abcdef";
    const DP_SALT: &[u8] = b"fedcba9876543210";
    const ITER: u32 = 100; // real backups use ~10k; kept low so tests stay fast

    fn tlv(tag: &[u8; 4], value: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(value.len() as u32).to_be_bytes());
        out.extend_from_slice(value);
        out
    }

    fn derive(password: &str) -> [u8; 32] {
        let mut first = [0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha256>>(password.as_bytes(), DP_SALT, ITER, &mut first).unwrap();
        let mut out = [0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha1>>(&first, SALT, ITER, &mut out).unwrap();
        out
    }

    fn keybag_blob(class_key: &[u8; 32]) -> Vec<u8> {
        let derived = derive(PASSWORD);
        let mut blob = Vec::new();
        blob.extend(tlv(b"VERS", &3u32.to_be_bytes()));
        blob.extend(tlv(b"TYPE", &1u32.to_be_bytes()));
        blob.extend(tlv(b"UUID", &[0xAA; 16]));
        blob.extend(tlv(b"HMCK", &[0xBB; 40]));
        blob.extend(tlv(b"WRAP", &0u32.to_be_bytes()));
        blob.extend(tlv(b"SALT", SALT));
        blob.extend(tlv(b"ITER", &ITER.to_be_bytes()));
        blob.extend(tlv(b"DPSL", DP_SALT));
        blob.extend(tlv(b"DPIC", &ITER.to_be_bytes()));
        blob.extend(tlv(b"UUID", &[0x03; 16]));
        blob.extend(tlv(b"CLAS", &CLASS.to_be_bytes()));
        blob.extend(tlv(b"WRAP", &2u32.to_be_bytes()));
        blob.extend(tlv(b"KTYP", &0u32.to_be_bytes()));
        blob.extend(tlv(b"WPKY", &aes_wrap_key(&derived, class_key)));
        blob
    }

    /// The NSKeyedArchiver record Manifest.db stores in its `file` column.
    fn file_blob(size: u64, wrapped_key: &[u8]) -> Vec<u8> {
        let mut key_obj = Dictionary::new();
        let mut ns_data = vec![0u8; 4];
        ns_data[..4].copy_from_slice(&CLASS.to_le_bytes());
        ns_data.extend_from_slice(wrapped_key);
        key_obj.insert("NS.data".into(), Value::Data(ns_data));

        let mut mb_file = Dictionary::new();
        mb_file.insert("Size".into(), Value::Integer((size as i64).into()));
        mb_file.insert(
            "ProtectionClass".into(),
            Value::Integer((CLASS as i64).into()),
        );
        mb_file.insert("EncryptionKey".into(), Value::Uid(Uid::new(2)));

        let mut top = Dictionary::new();
        top.insert("root".into(), Value::Uid(Uid::new(1)));

        let mut root = Dictionary::new();
        root.insert("$version".into(), Value::Integer(100_000.into()));
        root.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
        root.insert("$top".into(), Value::Dictionary(top));
        root.insert(
            "$objects".into(),
            Value::Array(vec![
                Value::String("$null".into()),
                Value::Dictionary(mb_file),
                Value::Dictionary(key_obj),
            ]),
        );

        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &Value::Dictionary(root)).unwrap();
        buf
    }

    fn pad16(data: &[u8]) -> Vec<u8> {
        let mut v = data.to_vec();
        v.resize(data.len().div_ceil(16) * 16, 0);
        v
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
        contents: Vec<u8>,
        file_id: String,
    }

    fn build_encrypted_backup() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();

        let class_key = [0x33u8; 32];
        let manifest_key = [0x44u8; 32];
        let file_key = [0x55u8; 32];

        // --- The one file the backup contains -------------------------------
        let file_id = "3d0d7e5fb2ce288813306e4d4636395e047a3d28".to_string();
        let contents = b"SQLite format 3\0 ... pretend this is sms.db".to_vec();
        std::fs::create_dir_all(root.join(&file_id[..2])).unwrap();
        std::fs::write(
            root.join(&file_id[..2]).join(&file_id),
            encrypt_cbc(&file_key, &pad16(&contents)),
        )
        .unwrap();

        // --- Manifest.db ----------------------------------------------------
        let db_path = root.join("plain.db");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE Files (fileID TEXT PRIMARY KEY, domain TEXT, \
                 relativePath TEXT, flags INTEGER, file BLOB)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO Files VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    file_id,
                    "HomeDomain",
                    "Library/SMS/sms.db",
                    1i64,
                    file_blob(contents.len() as u64, &aes_wrap_key(&class_key, &file_key)),
                ],
            )
            .unwrap();
        }
        let plain_db = std::fs::read(&db_path).unwrap();
        std::fs::remove_file(&db_path).unwrap();
        std::fs::write(
            root.join("Manifest.db"),
            encrypt_cbc(&manifest_key, &pad16(&plain_db)),
        )
        .unwrap();

        // --- Manifest.plist -------------------------------------------------
        let mut mk = CLASS.to_le_bytes().to_vec();
        mk.extend_from_slice(&aes_wrap_key(&class_key, &manifest_key));

        let mut manifest = Dictionary::new();
        manifest.insert("IsEncrypted".into(), Value::Boolean(true));
        manifest.insert("BackupKeyBag".into(), Value::Data(keybag_blob(&class_key)));
        manifest.insert("ManifestKey".into(), Value::Data(mk));
        plist::to_file_binary(root.join("Manifest.plist"), &Value::Dictionary(manifest)).unwrap();

        // --- Info.plist (left in the clear, even when encrypted) ------------
        let mut info = Dictionary::new();
        info.insert(
            "Device Name".into(),
            Value::String("Jonathan's iPhone".into()),
        );
        info.insert("Product Version".into(), Value::String("17.4.1".into()));
        info.insert("Product Type".into(), Value::String("iPhone14,2".into()));
        plist::to_file_binary(root.join("Info.plist"), &Value::Dictionary(info)).unwrap();

        Fixture {
            _dir: dir,
            root,
            contents,
            file_id,
        }
    }

    #[test]
    fn inspect_names_the_phone_without_a_password() {
        let fx = build_encrypted_backup();
        let summary = inspect(fx.root.to_str().unwrap()).unwrap().unwrap();

        assert!(summary.encrypted);
        // This is the point of reading Info.plist separately: the user can
        // confirm which phone it is before being asked for anything.
        assert_eq!(
            summary.device.device_name.as_deref(),
            Some("Jonathan's iPhone")
        );
        assert_eq!(summary.device.ios_version.as_deref(), Some("17.4.1"));
    }

    #[test]
    fn opens_an_encrypted_backup_and_decrypts_a_file() {
        let fx = build_encrypted_backup();
        let backup = Backup::open(&fx.root, Some(PASSWORD)).unwrap();

        assert!(backup.is_encrypted());
        assert_eq!(backup.files().len(), 1);

        let file = backup.find("HomeDomain", "Library/SMS/sms.db").unwrap();
        assert_eq!(file.file_id, fx.file_id);
        assert_eq!(file.size, fx.contents.len() as u64);

        // The padding the fixture added must not come back with the contents.
        assert_eq!(backup.read_file(file).unwrap(), fx.contents);
    }

    #[test]
    fn the_wrong_password_is_reported_clearly() {
        let fx = build_encrypted_backup();
        let err = Backup::open(&fx.root, Some("wrong")).unwrap_err();
        assert!(matches!(err, Error::WrongPassword), "got {err:?}");
        assert_eq!(err.to_string(), "that backup password is not correct");
    }

    #[test]
    fn an_encrypted_backup_opened_without_a_password_says_what_it_needs() {
        let fx = build_encrypted_backup();
        assert!(matches!(
            Backup::open(&fx.root, None),
            Err(Error::PasswordRequired)
        ));
    }

    #[test]
    fn category_counts_come_from_the_decrypted_manifest() {
        let fx = build_encrypted_backup();
        let backup = Backup::open(&fx.root, Some(PASSWORD)).unwrap();
        let counts = backup.category_counts();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].category, "messages");
        assert_eq!(counts[0].files, 1);
    }

    #[test]
    fn the_decrypted_manifest_is_not_left_on_disk() {
        let fx = build_encrypted_backup();
        let _backup = Backup::open(&fx.root, Some(PASSWORD)).unwrap();

        // Only the files the fixture wrote should exist; no stray plaintext.
        let mut names: Vec<_> = std::fs::read_dir(&fx.root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["3d", "Info.plist", "Manifest.db", "Manifest.plist"]);

        // And the on-disk manifest is still ciphertext.
        let raw = std::fs::read(fx.root.join("Manifest.db")).unwrap();
        assert_ne!(&raw[..15], b"SQLite format 3");
        let _ = Path::new("");
    }

    #[test]
    fn a_folder_that_is_not_a_backup_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(inspect(dir.path().to_str().unwrap()).unwrap().is_none());
        assert!(inspect("/definitely/not/here").unwrap().is_none());
    }

    #[test]
    fn opening_a_non_backup_says_so() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            Backup::open(dir.path(), None),
            Err(Error::NotABackup)
        ));
    }
}

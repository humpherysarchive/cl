//! Reading the camera roll.
//!
//! `Media/PhotoData/Photos.sqlite` is Core Data's store for the Photos app.
//! It supplies capture dates, pixel dimensions and — importantly — which
//! assets are in the Recently Deleted album, which the file listing alone
//! cannot tell us.
//!
//! The schema moves between iOS releases, so nothing here assumes a column
//! exists. The asset table is read through `PRAGMA table_info`, every optional
//! column degrades to `None`, and if the database is missing or unreadable the
//! caller falls back to listing `Media/DCIM` in filename order. A backup that
//! is one iOS version ahead of us should show fewer details, never fail.
//!
//! Only originals are listed. Edits made in the Photos app live under
//! `Media/PhotoData/Mutations/` and are not shown.

use std::collections::HashMap;

use serde::Serialize;

use super::manifest::BackupFile;
use super::Error;

/// Core Data counts from 2001-01-01; Unix time counts from 1970-01-01.
const COCOA_EPOCH_OFFSET: i64 = 978_307_200;

/// Extensions that belong to the Videos category rather than Photos.
const VIDEO_EXTS: [&str; 6] = ["mov", "mp4", "m4v", "avi", "3gp", "mpg"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    /// The backup's file id, used as the key for streaming and caching.
    pub id: String,
    pub filename: String,
    /// Capture time as Unix seconds, when the database records one.
    pub created: Option<i64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Lower-case extension, which decides whether the bytes need transcoding.
    pub ext: String,
    pub size: u64,
    /// A pre-rendered JPEG derivative inside the backup, when one exists.
    /// Using it skips HEVC decoding entirely.
    #[serde(skip)]
    pub derivative_id: Option<String>,
}

impl Photo {
    /// HEIC and HEIF need transcoding; the webview renders everything else.
    pub fn needs_transcode(&self) -> bool {
        matches!(self.ext.as_str(), "heic" | "heif")
    }

    pub fn mime(&self) -> &'static str {
        match self.ext.as_str() {
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/jpeg",
        }
    }
}

/// One row of the asset table, before it is matched to a file in the backup.
#[derive(Default)]
struct AssetRow {
    created: Option<i64>,
    width: Option<u32>,
    height: Option<u32>,
    trashed: bool,
}

/// Build the photo list for a backup.
///
/// `read_sqlite` is handed the asset database's bytes so the caller controls
/// how it is staged and cleaned up; passing `None` means the database could
/// not be read and the DCIM listing alone is used.
pub fn list(
    files: &[BackupFile],
    assets_db: Option<&std::path::Path>,
) -> Result<Vec<Photo>, Error> {
    // Every still image in the camera roll, keyed by its path on the phone.
    let mut by_path: HashMap<&str, &BackupFile> = HashMap::new();
    for file in files {
        if file.is_directory || file.domain != "CameraRollDomain" {
            continue;
        }
        if file.relative_path.starts_with("Media/DCIM/") {
            by_path.insert(file.relative_path.as_str(), file);
        }
    }

    let derivatives = index_derivatives(files);

    // The asset table, when it can be read. A failure here is not fatal.
    let rows = match assets_db {
        Some(path) => read_assets(path).unwrap_or_default(),
        None => HashMap::new(),
    };

    let mut photos = Vec::new();
    for (path, file) in &by_path {
        let filename = path.rsplit('/').next().unwrap_or(path);
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        // Videos have their own category.
        if VIDEO_EXTS.contains(&ext.as_str()) {
            continue;
        }
        // "Media/DCIM/100APPLE/IMG_0001.HEIC" -> "DCIM/100APPLE"
        let dir = path
            .strip_prefix("Media/")
            .and_then(|p| p.rsplit_once('/'))
            .map(|(d, _)| d)
            .unwrap_or("");

        let row = rows.get(&format!("{dir}/{filename}"));
        if row.is_some_and(|r| r.trashed) {
            continue;
        }

        photos.push(Photo {
            id: file.file_id.clone(),
            filename: filename.to_string(),
            created: row.and_then(|r| r.created),
            width: row.and_then(|r| r.width),
            height: row.and_then(|r| r.height),
            ext,
            size: file.size,
            derivative_id: derivatives.get(&format!("{dir}/{filename}")).cloned(),
        });
    }

    // Newest first where a date is known; undated assets sort to the end by
    // filename, which on a camera roll is close enough to capture order.
    photos.sort_by(|a, b| match (b.created, a.created) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.filename.cmp(&b.filename),
    });

    Ok(photos)
}

/// iOS keeps JPEG derivatives under `Media/PhotoData/Thumbnails/V2/<dir>/<name>/`.
/// They are regenerable caches, so a backup may or may not carry them; when it
/// does, a grid thumbnail costs a file read instead of an HEVC decode.
/// Returns a map from `<dir>/<filename>` to the derivative's file id.
fn index_derivatives(files: &[BackupFile]) -> HashMap<String, String> {
    const PREFIX: &str = "Media/PhotoData/Thumbnails/V2/";
    let mut best: HashMap<String, (u64, String)> = HashMap::new();

    for file in files {
        if file.is_directory || file.domain != "CameraRollDomain" {
            continue;
        }
        let Some(rest) = file.relative_path.strip_prefix(PREFIX) else {
            continue;
        };
        // rest looks like "DCIM/100APPLE/IMG_0001.HEIC/5005.JPG"
        let Some((asset, leaf)) = rest.rsplit_once('/') else {
            continue;
        };
        if !leaf.to_ascii_lowercase().ends_with(".jpg") {
            continue;
        }
        // Several sizes can be present; keep the largest, which is still far
        // smaller than the original.
        let entry = best.entry(asset.to_string()).or_insert((0, String::new()));
        if file.size > entry.0 {
            *entry = (file.size, file.file_id.clone());
        }
    }

    best.into_iter().map(|(k, (_, id))| (k, id)).collect()
}

/// Read the asset table, keyed by `<ZDIRECTORY>/<ZFILENAME>`.
fn read_assets(path: &std::path::Path) -> Result<HashMap<String, AssetRow>, Error> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    // iOS 13 and later call it ZASSET; iOS 10-12 call it ZGENERICASSET.
    let table = ["ZASSET", "ZGENERICASSET"]
        .into_iter()
        .find(|t| table_exists(&conn, t))
        .ok_or(Error::Crypto("no asset table in Photos.sqlite"))?;

    let columns = table_columns(&conn, table)?;
    let has = |c: &str| columns.iter().any(|x| x == c);

    // Without these two there is no way to match a row to a file.
    if !has("ZDIRECTORY") || !has("ZFILENAME") {
        return Ok(HashMap::new());
    }

    // Build the projection from what this schema actually has, so a renamed
    // or dropped column costs that one detail rather than the whole query.
    let optional = ["ZDATECREATED", "ZWIDTH", "ZHEIGHT", "ZTRASHEDSTATE"];
    let mut select = vec!["ZDIRECTORY".to_string(), "ZFILENAME".to_string()];
    let mut present = Vec::new();
    for col in optional {
        if has(col) {
            present.push(col);
            select.push(col.to_string());
        } else {
            // Keep the column positions fixed so indexing below stays simple.
            select.push(format!("NULL AS {col}"));
        }
    }

    let sql = format!("SELECT {} FROM {}", select.join(", "), table);
    let mut stmt = conn.prepare(&sql)?;
    let mut out = HashMap::new();

    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let dir: String = row.get::<_, Option<String>>(0)?.unwrap_or_default();
        let name: String = match row.get::<_, Option<String>>(1)? {
            Some(n) if !n.is_empty() => n,
            _ => continue,
        };

        // Core Data writes the date as floating point seconds, but SQLite
        // stores whatever it was given: a column's declared type is only an
        // affinity. Coerce across storage classes rather than dropping a value
        // we do not recognise — silently discarding ZTRASHEDSTATE would put
        // deleted photos back in the grid.
        let created = number(row, 2).map(|s| s as i64 + COCOA_EPOCH_OFFSET);
        let dim = |i: usize| number(row, i).filter(|v| *v >= 1.0).map(|v| v as u32);

        out.insert(
            format!("{dir}/{name}"),
            AssetRow {
                created,
                width: dim(3),
                height: dim(4),
                trashed: number(row, 5).unwrap_or(0.0) != 0.0,
            },
        );
    }

    Ok(out)
}

/// Read a numeric value whatever storage class it landed in.
fn number(row: &rusqlite::Row<'_>, index: usize) -> Option<f64> {
    use rusqlite::types::ValueRef;
    match row.get_ref(index).ok()? {
        ValueRef::Integer(i) => Some(i as f64),
        ValueRef::Real(f) => Some(f),
        ValueRef::Text(t) => std::str::from_utf8(t).ok()?.trim().parse().ok(),
        _ => None,
    }
}

fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

fn table_columns(conn: &rusqlite::Connection, table: &str) -> Result<Vec<String>, Error> {
    // PRAGMA does not accept a bound parameter for the table name; the value
    // is one of our own two constants, never user input.
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn file(path: &str, id: &str, size: u64) -> BackupFile {
        BackupFile {
            file_id: id.to_string(),
            domain: "CameraRollDomain".into(),
            relative_path: path.to_string(),
            size,
            is_directory: false,
            encryption: None,
        }
    }

    /// Build an asset table with only the columns named, so tests can model an
    /// iOS version that does not have all of them.
    fn assets_db(
        dir: &std::path::Path,
        table: &str,
        columns: &[&str],
        rows: &[Vec<String>],
    ) -> PathBuf {
        let path = dir.join("Photos.sqlite");
        let conn = Connection::open(&path).unwrap();
        // Match the real schema's affinities: text for the path parts, a
        // float for the Core Data date, integers for the rest.
        let defs: Vec<String> = columns
            .iter()
            .map(|c| match *c {
                "ZDIRECTORY" | "ZFILENAME" => format!("{c} VARCHAR"),
                "ZDATECREATED" => format!("{c} REAL"),
                _ => format!("{c} INTEGER"),
            })
            .collect();
        conn.execute(&format!("CREATE TABLE {table} ({})", defs.join(", ")), [])
            .unwrap();
        for row in rows {
            let holes: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
            conn.execute(
                &format!("INSERT INTO {table} VALUES ({})", holes.join(", ")),
                rusqlite::params_from_iter(row.iter()),
            )
            .unwrap();
        }
        path
    }

    const FULL: [&str; 6] = [
        "ZDIRECTORY",
        "ZFILENAME",
        "ZDATECREATED",
        "ZWIDTH",
        "ZHEIGHT",
        "ZTRASHEDSTATE",
    ];

    fn row(dir: &str, name: &str, created: &str, w: &str, h: &str, trashed: &str) -> Vec<String> {
        vec![dir, name, created, w, h, trashed]
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn reads_dates_widths_and_skips_trashed() {
        let tmp = tempfile::tempdir().unwrap();
        // 700000000 Core Data seconds = 2023-03-08T05:20:00Z.
        let db = assets_db(
            tmp.path(),
            "ZASSET",
            &FULL,
            &[
                row(
                    "DCIM/100APPLE",
                    "IMG_0001.HEIC",
                    "700000000",
                    "4032",
                    "3024",
                    "0",
                ),
                row(
                    "DCIM/100APPLE",
                    "IMG_0002.HEIC",
                    "700000060",
                    "4032",
                    "3024",
                    "0",
                ),
                row(
                    "DCIM/100APPLE",
                    "IMG_0003.HEIC",
                    "700000120",
                    "4032",
                    "3024",
                    "1",
                ),
            ],
        );

        let files = vec![
            file("Media/DCIM/100APPLE/IMG_0001.HEIC", &"a".repeat(40), 100),
            file("Media/DCIM/100APPLE/IMG_0002.HEIC", &"b".repeat(40), 200),
            file("Media/DCIM/100APPLE/IMG_0003.HEIC", &"c".repeat(40), 300),
        ];

        let photos = list(&files, Some(&db)).unwrap();
        // The trashed asset is gone.
        assert_eq!(photos.len(), 2);
        // Newest first.
        assert_eq!(photos[0].filename, "IMG_0002.HEIC");
        assert_eq!(photos[0].created, Some(700_000_060 + COCOA_EPOCH_OFFSET));
        assert_eq!(photos[0].width, Some(4032));
        assert_eq!(photos[0].height, Some(3024));
        assert!(photos[0].needs_transcode());
    }

    #[test]
    fn videos_are_left_for_the_videos_category() {
        let tmp = tempfile::tempdir().unwrap();
        let db = assets_db(
            tmp.path(),
            "ZASSET",
            &FULL,
            &[
                row("DCIM/100APPLE", "IMG_0001.HEIC", "700000000", "0", "0", "0"),
                row("DCIM/100APPLE", "IMG_0002.MOV", "700000060", "0", "0", "0"),
                row("DCIM/100APPLE", "IMG_0003.mp4", "700000120", "0", "0", "0"),
            ],
        );
        let files = vec![
            file("Media/DCIM/100APPLE/IMG_0001.HEIC", &"a".repeat(40), 1),
            file("Media/DCIM/100APPLE/IMG_0002.MOV", &"b".repeat(40), 1),
            file("Media/DCIM/100APPLE/IMG_0003.mp4", &"c".repeat(40), 1),
        ];
        let photos = list(&files, Some(&db)).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].filename, "IMG_0001.HEIC");
    }

    #[test]
    fn older_ios_uses_zgenericasset() {
        let tmp = tempfile::tempdir().unwrap();
        let db = assets_db(
            tmp.path(),
            "ZGENERICASSET",
            &FULL,
            &[row(
                "DCIM/100APPLE",
                "IMG_0001.JPG",
                "500000000",
                "3264",
                "2448",
                "0",
            )],
        );
        let files = vec![file("Media/DCIM/100APPLE/IMG_0001.JPG", &"a".repeat(40), 1)];
        let photos = list(&files, Some(&db)).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].created, Some(500_000_000 + COCOA_EPOCH_OFFSET));
        // JPEG goes to the webview untouched.
        assert!(!photos[0].needs_transcode());
    }

    #[test]
    fn a_schema_without_the_optional_columns_still_lists_photos() {
        let tmp = tempfile::tempdir().unwrap();
        // Only the two columns needed to match a row to a file.
        let db = assets_db(
            tmp.path(),
            "ZASSET",
            &["ZDIRECTORY", "ZFILENAME"],
            &[vec!["DCIM/100APPLE".into(), "IMG_0001.HEIC".into()]],
        );
        let files = vec![file(
            "Media/DCIM/100APPLE/IMG_0001.HEIC",
            &"a".repeat(40),
            1,
        )];

        let photos = list(&files, Some(&db)).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].created, None);
        assert_eq!(photos[0].width, None);
    }

    #[test]
    fn an_unusable_database_falls_back_to_the_file_listing() {
        let files = vec![
            file("Media/DCIM/100APPLE/IMG_0002.HEIC", &"b".repeat(40), 1),
            file("Media/DCIM/100APPLE/IMG_0001.HEIC", &"a".repeat(40), 1),
        ];

        // No database at all.
        let photos = list(&files, None).unwrap();
        assert_eq!(photos.len(), 2);
        // Undated assets fall back to filename order.
        assert_eq!(photos[0].filename, "IMG_0001.HEIC");

        // A database that is not SQLite at all.
        let tmp = tempfile::tempdir().unwrap();
        let junk = tmp.path().join("Photos.sqlite");
        std::fs::write(&junk, b"definitely not a database").unwrap();
        assert_eq!(list(&files, Some(&junk)).unwrap().len(), 2);

        // A database with no asset table.
        let empty = tmp.path().join("empty.sqlite");
        Connection::open(&empty).unwrap();
        assert_eq!(list(&files, Some(&empty)).unwrap().len(), 2);
    }

    #[test]
    fn photos_outside_the_asset_table_are_still_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let db = assets_db(
            tmp.path(),
            "ZASSET",
            &FULL,
            &[row(
                "DCIM/100APPLE",
                "IMG_0001.HEIC",
                "700000000",
                "1",
                "1",
                "0",
            )],
        );
        let files = vec![
            file("Media/DCIM/100APPLE/IMG_0001.HEIC", &"a".repeat(40), 1),
            // Present on disk but absent from the database.
            file("Media/DCIM/100APPLE/IMG_0099.HEIC", &"z".repeat(40), 1),
        ];
        let photos = list(&files, Some(&db)).unwrap();
        assert_eq!(photos.len(), 2);
        let orphan = photos
            .iter()
            .find(|p| p.filename == "IMG_0099.HEIC")
            .unwrap();
        assert_eq!(orphan.created, None);
    }

    #[test]
    fn finds_the_largest_pre_rendered_derivative() {
        let mut files = vec![file(
            "Media/DCIM/100APPLE/IMG_0001.HEIC",
            &"a".repeat(40),
            1,
        )];
        files.push(file(
            "Media/PhotoData/Thumbnails/V2/DCIM/100APPLE/IMG_0001.HEIC/5003.JPG",
            &"d".repeat(40),
            4_000,
        ));
        files.push(file(
            "Media/PhotoData/Thumbnails/V2/DCIM/100APPLE/IMG_0001.HEIC/5005.JPG",
            &"e".repeat(40),
            9_000,
        ));
        // A non-JPEG sibling must not be chosen.
        files.push(file(
            "Media/PhotoData/Thumbnails/V2/DCIM/100APPLE/IMG_0001.HEIC/meta.dat",
            &"f".repeat(40),
            99_000,
        ));

        let photos = list(&files, None).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(
            photos[0].derivative_id.as_deref(),
            Some("e".repeat(40).as_str())
        );
    }

    #[test]
    fn a_backup_with_no_derivatives_simply_has_none() {
        let files = vec![file(
            "Media/DCIM/100APPLE/IMG_0001.HEIC",
            &"a".repeat(40),
            1,
        )];
        let photos = list(&files, None).unwrap();
        assert!(photos[0].derivative_id.is_none());
    }

    /// A column's declared type is only an affinity, so the same field can
    /// come back as text on one device and an integer on another. Losing
    /// ZTRASHEDSTATE that way would put deleted photos back in the grid.
    #[test]
    fn values_stored_as_text_are_still_read() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Photos.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE ZASSET (ZDIRECTORY BLOB, ZFILENAME BLOB, ZDATECREATED BLOB, \
             ZWIDTH BLOB, ZHEIGHT BLOB, ZTRASHEDSTATE BLOB)",
            [],
        )
        .unwrap();
        // Every value inserted as text, which BLOB affinity preserves.
        conn.execute(
            "INSERT INTO ZASSET VALUES ('DCIM/100APPLE', 'IMG_0001.HEIC', '700000000', '4032', '3024', '0')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ZASSET VALUES ('DCIM/100APPLE', 'IMG_0002.HEIC', '700000060', '4032', '3024', '1')",
            [],
        )
        .unwrap();
        drop(conn);

        let files = vec![
            file("Media/DCIM/100APPLE/IMG_0001.HEIC", &"a".repeat(40), 1),
            file("Media/DCIM/100APPLE/IMG_0002.HEIC", &"b".repeat(40), 1),
        ];
        let photos = list(&files, Some(&path)).unwrap();

        assert_eq!(photos.len(), 1, "the trashed photo must still be excluded");
        assert_eq!(photos[0].created, Some(700_000_000 + COCOA_EPOCH_OFFSET));
        assert_eq!(photos[0].width, Some(4032));
    }

    #[test]
    fn other_domains_and_directories_are_ignored() {
        let mut files = vec![file(
            "Media/DCIM/100APPLE/IMG_0001.HEIC",
            &"a".repeat(40),
            1,
        )];
        // Right domain, wrong directory.
        files.push(file("Media/PhotoData/Photos.sqlite", &"b".repeat(40), 1));
        // Right path, wrong domain.
        let mut other = file("Media/DCIM/100APPLE/IMG_0002.HEIC", &"c".repeat(40), 1);
        other.domain = "MediaDomain".into();
        files.push(other);

        let photos = list(&files, None).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].filename, "IMG_0001.HEIC");
    }
}

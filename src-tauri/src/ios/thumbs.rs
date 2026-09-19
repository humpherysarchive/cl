//! Thumbnail generation and the on-disk cache.
//!
//! HEIC is HEVC-coded, which WebKitGTK cannot render and which costs real time
//! to decode — around 400 ms for a 12 MP frame on a modern CPU, and several
//! times that on the 2014-era machines this is meant to run on. So thumbnails
//! are produced only for what the grid actually shows, and kept.
//!
//! Three sources, cheapest first:
//!   1. the cache on disk,
//!   2. a JPEG derivative iOS already wrote into the backup, if present,
//!   3. decoding the original.
//!
//! The cache lives in the OS cache directory, never in the backup folder, and
//! is namespaced per backup because a file id is only unique within one
//! device's backup.

use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, RgbImage};

use super::Error;

/// Longest edge of a grid thumbnail, in pixels. Sized for a ~160 pt cell on a
/// 2x display.
pub const THUMB_EDGE: u32 = 320;

const THUMB_QUALITY: u8 = 72;
const FULL_QUALITY: u8 = 88;

/// Default cache ceiling. Deliberately modest: this has to be sane on a live
/// USB image and on machines with small disks.
pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 200 * 1024 * 1024;

/// Where thumbnails for one backup live.
pub fn cache_dir(base: &Path, backup_key: &str) -> PathBuf {
    base.join("thumbnails").join(backup_key)
}

fn cache_path(dir: &Path, id: &str, edge: u32) -> PathBuf {
    dir.join(format!("{id}_{edge}.jpg"))
}

/// Read a cached thumbnail, if one was made earlier.
pub fn cached(dir: &Path, id: &str, edge: u32) -> Option<Vec<u8>> {
    fs::read(cache_path(dir, id, edge)).ok()
}

/// Write a thumbnail to the cache. A failure here is not fatal — the image was
/// produced either way, and the next request simply regenerates it.
pub fn store(dir: &Path, id: &str, edge: u32, jpeg: &[u8]) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = cache_path(dir, id, edge);
    // Write to a temporary name and rename, so a process that dies midway
    // cannot leave a truncated JPEG behind to be served later.
    let tmp = path.with_extension("part");
    if fs::write(&tmp, jpeg).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

/// Decode any format we support into RGB8.
pub fn decode(bytes: &[u8], ext: &str) -> Result<RgbImage, Error> {
    if matches!(ext, "heic" | "heif") {
        let img = heif_oxide::decode_bytes(bytes)
            .map_err(|_| Error::Decode("this HEIC image could not be decoded"))?;
        let (w, h) = (img.width, img.height);
        let rgba = img.to_rgba8();
        let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
        for px in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
        }
        return RgbImage::from_raw(w, h, rgb)
            .ok_or(Error::Decode("decoded HEIC had an unexpected size"));
    }

    let img = image::load_from_memory(bytes)
        .map_err(|_| Error::Decode("this image could not be decoded"))?;
    Ok(img.to_rgb8())
}

/// Scale to fit within `edge` on the longest side and encode as JPEG.
/// Images already smaller than `edge` are encoded without upscaling.
pub fn encode_scaled(img: &RgbImage, edge: u32, quality: u8) -> Result<Vec<u8>, Error> {
    let (w, h) = (img.width(), img.height());
    let scaled = if w.max(h) <= edge {
        None
    } else {
        let (nw, nh) = if w >= h {
            (edge, (h * edge / w).max(1))
        } else {
            ((w * edge / h).max(1), edge)
        };
        // Triangle is a good trade at thumbnail sizes: visibly better than
        // nearest, far cheaper than Lanczos on a slow CPU.
        Some(DynamicImage::ImageRgb8(img.clone()).resize_exact(
            nw,
            nh,
            image::imageops::FilterType::Triangle,
        ))
    };

    let (out, w, h) = match &scaled {
        Some(d) => {
            let rgb = d.to_rgb8();
            let (w, h) = (rgb.width(), rgb.height());
            (rgb.into_raw(), w, h)
        }
        None => (img.as_raw().clone(), w, h),
    };

    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .write_image(&out, w, h, ExtendedColorType::Rgb8)
        .map_err(|_| Error::Decode("could not encode the image"))?;
    Ok(buf)
}

/// Produce a grid thumbnail from original bytes.
pub fn make_thumbnail(bytes: &[u8], ext: &str) -> Result<Vec<u8>, Error> {
    let img = decode(bytes, ext)?;
    encode_scaled(&img, THUMB_EDGE, THUMB_QUALITY)
}

/// Transcode a full-size image the webview cannot display natively.
pub fn transcode_full(bytes: &[u8], ext: &str) -> Result<Vec<u8>, Error> {
    let img = decode(bytes, ext)?;
    // Full size: no downscale, just a format the webview understands.
    encode_scaled(&img, u32::MAX, FULL_QUALITY)
}

/// Total size of the cached thumbnails for one backup.
pub fn cache_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Delete every cached thumbnail under `dir`. Returns the bytes freed.
pub fn clear_cache(dir: &Path) -> u64 {
    let freed = cache_size(dir);
    let _ = fs::remove_dir_all(dir);
    freed
}

/// Drop the least recently used thumbnails until the cache fits in `limit`.
///
/// Called after writes rather than on a timer, so a long import cannot run the
/// disk down between checks.
pub fn evict_to_fit(dir: &Path, limit: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            // Fall back to modified time where the platform has no atime.
            let when = meta.accessed().or_else(|_| meta.modified()).ok()?;
            Some((when, meta.len(), e.path()))
        })
        .collect();

    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    if total <= limit {
        return;
    }

    files.sort_by_key(|(when, _, _)| *when); // oldest first
    for (_, len, path) in files {
        if total <= limit {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        })
    }

    #[test]
    fn scaling_fits_the_long_edge_and_keeps_aspect() {
        let wide = test_image(800, 400);
        let jpeg = encode_scaled(&wide, 320, 72).unwrap();
        let out = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (320, 160));

        let tall = test_image(400, 800);
        let jpeg = encode_scaled(&tall, 320, 72).unwrap();
        let out = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (160, 320));
    }

    #[test]
    fn images_smaller_than_the_target_are_not_upscaled() {
        let small = test_image(64, 48);
        let jpeg = encode_scaled(&small, 320, 72).unwrap();
        let out = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (64, 48));
    }

    #[test]
    fn an_extreme_aspect_ratio_still_has_at_least_one_pixel() {
        let sliver = test_image(2000, 3);
        let jpeg = encode_scaled(&sliver, 320, 72).unwrap();
        let out = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (320, 1));
    }

    /// Real HEVC data, not a mock: proves the decode -> resize -> JPEG path
    /// works end to end. See fixtures/README.md for how it was made and for
    /// the grid-tiling gap it does not cover.
    const HEIC_FIXTURE: &[u8] = include_bytes!("../../fixtures/gradient.heic");

    #[test]
    fn decodes_a_real_heic() {
        let img = decode(HEIC_FIXTURE, "heic").unwrap();
        assert_eq!((img.width(), img.height()), (640, 480));

        // A swirled gradient: the corners must not all be the same colour, or
        // we decoded a blank frame and called it success.
        let corner = img.get_pixel(0, 0);
        let other = img.get_pixel(639, 479);
        assert_ne!(corner, other);
    }

    #[test]
    fn makes_a_grid_thumbnail_from_a_real_heic() {
        let jpeg = make_thumbnail(HEIC_FIXTURE, "heic").unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "not a JPEG");

        let out = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (THUMB_EDGE, 240));
        // A thumbnail that is larger than its source means something is wrong.
        assert!(jpeg.len() < HEIC_FIXTURE.len() * 4);
    }

    #[test]
    fn transcodes_a_real_heic_at_full_size() {
        let jpeg = transcode_full(HEIC_FIXTURE, "heic").unwrap();
        let out = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((out.width(), out.height()), (640, 480));
    }

    #[test]
    fn corrupt_bytes_report_rather_than_panic() {
        assert!(decode(b"not an image", "jpg").is_err());
        assert!(decode(b"not an image", "heic").is_err());
        assert!(decode(&[], "png").is_err());
        // Truncating real HEIC data must fail cleanly, not panic inside the
        // decoder.
        assert!(decode(&HEIC_FIXTURE[..HEIC_FIXTURE.len() / 2], "heic").is_err());
    }

    #[test]
    fn round_trips_through_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t");
        assert!(cached(&path, "abc", 320).is_none());

        store(&path, "abc", 320, b"jpeg-bytes");
        assert_eq!(cached(&path, "abc", 320).unwrap(), b"jpeg-bytes");
        // A different size is a different entry.
        assert!(cached(&path, "abc", 640).is_none());
        assert_eq!(cache_size(&path), 10);

        assert_eq!(clear_cache(&path), 10);
        assert!(cached(&path, "abc", 320).is_none());
    }

    #[test]
    fn eviction_drops_oldest_first_until_it_fits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t");
        fs::create_dir_all(&path).unwrap();

        // 5 entries of 100 bytes, written oldest to newest.
        for i in 0..5 {
            let f = path.join(format!("id{i}_320.jpg"));
            fs::write(&f, vec![0u8; 100]).unwrap();
            let when = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(1_700_000_000 + i * 60);
            let _ = set_times(&f, when);
        }
        assert_eq!(cache_size(&path), 500);

        evict_to_fit(&path, 250);
        assert!(
            cache_size(&path) <= 250,
            "cache still {}",
            cache_size(&path)
        );
        // The newest entry must survive.
        assert!(path.join("id4_320.jpg").exists());
    }

    /// Set both timestamps so the eviction test controls ordering rather than
    /// depending on the filesystem clock's resolution.
    fn set_times(path: &Path, when: std::time::SystemTime) -> std::io::Result<()> {
        let f = fs::File::options().write(true).open(path)?;
        f.set_times(fs::FileTimes::new().set_accessed(when).set_modified(when))
    }
}

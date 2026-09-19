//! The backup keybag.
//!
//! `Manifest.plist` carries a `BackupKeyBag` blob holding one wrapped AES key
//! per data protection class. The user's backup password unlocks those keys;
//! each file in the backup then names the class whose key unwraps its own
//! per-file key. Nothing in an encrypted backup — not even the file listing —
//! is readable until this succeeds.
//!
//! The blob is a flat sequence of TLV records: a four-byte ASCII tag, a
//! four-byte big-endian length, then the value. Class entries are delimited
//! by a `UUID` record, so a new `UUID` after the header closes the previous
//! class off.

use std::collections::HashMap;

use hmac::Hmac;
use sha1::Sha1;
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

use super::crypto::aes_unwrap_key;
use super::Error;

/// Data protection class. Only the classes Apple actually assigns in backups
/// are named; the rest are carried through by number.
pub type ProtectionClass = u32;

/// WRAP values. A key wrapped with the device key (bit 0) cannot be recovered
/// from a backup at all — that key never leaves the phone.
const WRAP_PASSCODE: u32 = 2;

#[derive(Debug)]
struct ClassKey {
    class: ProtectionClass,
    wrap: u32,
    wrapped: Vec<u8>,
}

/// A parsed, still-locked keybag.
pub struct Keybag {
    salt: Vec<u8>,
    iterations: u32,
    /// iOS 10.2 and later put a second, stronger KDF pass in front. Absent on
    /// older backups, which use the SHA-1 pass alone.
    dp_salt: Option<Vec<u8>>,
    dp_iterations: Option<u32>,
    classes: Vec<ClassKey>,
}

/// An unlocked keybag: class keys in the clear, wiped on drop.
pub struct UnlockedKeybag {
    keys: HashMap<ProtectionClass, [u8; 32]>,
}

impl Drop for UnlockedKeybag {
    fn drop(&mut self) {
        for key in self.keys.values_mut() {
            key.zeroize();
        }
    }
}

impl UnlockedKeybag {
    /// Unwrap a per-file (or the manifest's) key using its class key.
    pub fn unwrap_key(
        &self,
        class: ProtectionClass,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, Error> {
        let class_key = self.keys.get(&class).ok_or(Error::MissingClassKey(class))?;
        let out = aes_unwrap_key(class_key, wrapped)?;
        let arr: [u8; 32] = out
            .as_slice()
            .try_into()
            .map_err(|_| Error::Crypto("unwrapped file key was not 32 bytes"))?;
        Ok(Zeroizing::new(arr))
    }
}

impl Keybag {
    /// Parse a `BackupKeyBag` blob.
    pub fn parse(blob: &[u8]) -> Result<Self, Error> {
        let mut salt = None;
        let mut iterations = None;
        let mut dp_salt = None;
        let mut dp_iterations = None;
        let mut classes: Vec<ClassKey> = Vec::new();

        // Fields of the class entry currently being read.
        let mut cur_class: Option<u32> = None;
        let mut cur_wrap: Option<u32> = None;
        let mut cur_wrapped: Option<Vec<u8>> = None;
        // The header also opens with a UUID; the first one must not be taken
        // as the start of a class entry.
        let mut seen_header_uuid = false;

        let push_pending = |class: &mut Option<u32>,
                            wrap: &mut Option<u32>,
                            wrapped: &mut Option<Vec<u8>>,
                            out: &mut Vec<ClassKey>| {
            if let (Some(c), Some(w)) = (class.take(), wrap.take()) {
                if let Some(k) = wrapped.take() {
                    out.push(ClassKey {
                        class: c,
                        wrap: w,
                        wrapped: k,
                    });
                }
            }
            *class = None;
            *wrap = None;
            *wrapped = None;
        };

        for (tag, value) in TlvIter::new(blob) {
            match &tag {
                b"SALT" => salt = Some(value.to_vec()),
                b"ITER" => iterations = Some(be_u32(value)?),
                b"DPSL" => dp_salt = Some(value.to_vec()),
                b"DPIC" => dp_iterations = Some(be_u32(value)?),
                b"UUID" => {
                    if seen_header_uuid {
                        push_pending(
                            &mut cur_class,
                            &mut cur_wrap,
                            &mut cur_wrapped,
                            &mut classes,
                        );
                    }
                    seen_header_uuid = true;
                }
                b"CLAS" => cur_class = Some(be_u32(value)?),
                b"WRAP" => {
                    // The header carries a WRAP too; only record it once a
                    // class entry is open.
                    if seen_header_uuid && cur_class.is_some() {
                        cur_wrap = Some(be_u32(value)?);
                    }
                }
                b"WPKY" => cur_wrapped = Some(value.to_vec()),
                _ => {}
            }
        }
        // Flush the final class, which no UUID follows.
        push_pending(
            &mut cur_class,
            &mut cur_wrap,
            &mut cur_wrapped,
            &mut classes,
        );

        let salt = salt.ok_or(Error::MalformedKeybag("no SALT record"))?;
        let iterations = iterations.ok_or(Error::MalformedKeybag("no ITER record"))?;
        if classes.is_empty() {
            return Err(Error::MalformedKeybag("no class keys"));
        }

        Ok(Keybag {
            salt,
            iterations,
            dp_salt,
            dp_iterations,
            classes,
        })
    }

    /// Derive the keybag key from the user's backup password and unwrap every
    /// class key it protects.
    ///
    /// A wrong password surfaces as `Error::WrongPassword`: the RFC 3394
    /// integrity check fails on the first class key, so this never returns
    /// plausible-looking garbage.
    pub fn unlock(&self, password: &str) -> Result<UnlockedKeybag, Error> {
        let mut derived = Zeroizing::new([0u8; 32]);

        match (&self.dp_salt, self.dp_iterations) {
            // iOS 10.2+: PBKDF2-SHA256 first, then the legacy SHA-1 pass over
            // its output. Both passes are required and neither is optional.
            (Some(dp_salt), Some(dp_iter)) => {
                let mut first = Zeroizing::new([0u8; 32]);
                pbkdf2::pbkdf2::<Hmac<Sha256>>(
                    password.as_bytes(),
                    dp_salt,
                    dp_iter,
                    first.as_mut(),
                )
                .map_err(|_| Error::Crypto("key derivation failed"))?;
                pbkdf2::pbkdf2::<Hmac<Sha1>>(
                    first.as_ref(),
                    &self.salt,
                    self.iterations,
                    derived.as_mut(),
                )
                .map_err(|_| Error::Crypto("key derivation failed"))?;
            }
            // Pre-10.2 backups.
            _ => {
                pbkdf2::pbkdf2::<Hmac<Sha1>>(
                    password.as_bytes(),
                    &self.salt,
                    self.iterations,
                    derived.as_mut(),
                )
                .map_err(|_| Error::Crypto("key derivation failed"))?;
            }
        }

        let mut keys = HashMap::new();
        let mut any_attempted = false;

        for entry in &self.classes {
            // Keys tied to the device's own hardware key are not recoverable
            // from a backup; skipping them is expected, not an error.
            if entry.wrap & WRAP_PASSCODE == 0 {
                continue;
            }
            any_attempted = true;

            match aes_unwrap_key(&derived, &entry.wrapped) {
                Ok(plain) => {
                    let arr: [u8; 32] = plain
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::Crypto("class key was not 32 bytes"))?;
                    keys.insert(entry.class, arr);
                }
                Err(Error::WrongKey) => return Err(Error::WrongPassword),
                Err(e) => return Err(e),
            }
        }

        if !any_attempted {
            return Err(Error::MalformedKeybag(
                "every class key is wrapped with the device key",
            ));
        }
        Ok(UnlockedKeybag { keys })
    }
}

/// Walks the tag/length/value records, stopping at the first malformed one
/// rather than panicking on a truncated blob.
struct TlvIter<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> TlvIter<'a> {
    fn new(buf: &'a [u8]) -> Self {
        TlvIter { buf, pos: 0 }
    }
}

impl<'a> Iterator for TlvIter<'a> {
    type Item = ([u8; 4], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos + 8 > self.buf.len() {
            return None;
        }
        let tag: [u8; 4] = self.buf[self.pos..self.pos + 4].try_into().ok()?;
        let len =
            u32::from_be_bytes(self.buf[self.pos + 4..self.pos + 8].try_into().ok()?) as usize;
        let start = self.pos + 8;
        let end = start.checked_add(len)?;
        if end > self.buf.len() {
            return None;
        }
        self.pos = end;
        Some((tag, &self.buf[start..end]))
    }
}

fn be_u32(value: &[u8]) -> Result<u32, Error> {
    // Apple writes these as 4 bytes, but shorter values appear in the wild.
    if value.is_empty() || value.len() > 4 {
        return Err(Error::MalformedKeybag("bad integer record"));
    }
    let mut buf = [0u8; 4];
    buf[4 - value.len()..].copy_from_slice(value);
    Ok(u32::from_be_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ios::crypto::aes_wrap_key;

    fn tlv(tag: &[u8; 4], value: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(value.len() as u32).to_be_bytes());
        out.extend_from_slice(value);
        out
    }

    /// Build a keybag the way iOS 10.2+ writes one, so the whole
    /// parse -> derive -> unwrap path is exercised end to end.
    fn fixture(password: &str, classes: &[(u32, [u8; 32])]) -> Vec<u8> {
        let salt = b"0123456789abcdef";
        let dp_salt = b"fedcba9876543210";
        // Deliberately tiny; the real files use ~10k.
        let (iter, dp_iter) = (100u32, 100u32);

        let mut first = [0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha256>>(password.as_bytes(), dp_salt, dp_iter, &mut first).unwrap();
        let mut derived = [0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha1>>(&first, salt, iter, &mut derived).unwrap();

        let mut blob = Vec::new();
        blob.extend(tlv(b"VERS", &3u32.to_be_bytes()));
        blob.extend(tlv(b"TYPE", &1u32.to_be_bytes()));
        blob.extend(tlv(b"UUID", &[0xAA; 16])); // header UUID
        blob.extend(tlv(b"HMCK", &[0xBB; 40]));
        blob.extend(tlv(b"WRAP", &0u32.to_be_bytes())); // header WRAP
        blob.extend(tlv(b"SALT", salt));
        blob.extend(tlv(b"ITER", &iter.to_be_bytes()));
        blob.extend(tlv(b"DPSL", dp_salt));
        blob.extend(tlv(b"DPIC", &dp_iter.to_be_bytes()));

        for (class, key) in classes {
            blob.extend(tlv(b"UUID", &[*class as u8; 16]));
            blob.extend(tlv(b"CLAS", &class.to_be_bytes()));
            blob.extend(tlv(b"WRAP", &WRAP_PASSCODE.to_be_bytes()));
            blob.extend(tlv(b"KTYP", &0u32.to_be_bytes()));
            blob.extend(tlv(b"WPKY", &aes_wrap_key(&derived, key)));
        }
        blob
    }

    #[test]
    fn parses_and_unlocks() {
        let classes = [
            (1u32, [0x11u8; 32]),
            (3u32, [0x33u8; 32]),
            (11u32, [0xEEu8; 32]),
        ];
        let bag = Keybag::parse(&fixture("hunter2", &classes)).unwrap();

        assert_eq!(bag.classes.len(), 3);
        assert_eq!(bag.iterations, 100);
        assert!(bag.dp_salt.is_some());

        let unlocked = bag.unlock("hunter2").unwrap();
        assert_eq!(unlocked.keys.len(), 3);
        for (class, key) in classes {
            assert_eq!(unlocked.keys[&class], key);
        }
    }

    #[test]
    fn a_wrong_password_is_reported_as_such() {
        let bag = Keybag::parse(&fixture("hunter2", &[(3u32, [0x33u8; 32])])).unwrap();
        assert!(matches!(bag.unlock("hunter3"), Err(Error::WrongPassword)));
    }

    #[test]
    fn unwraps_a_file_key_with_its_class_key() {
        let class_key = [0x33u8; 32];
        let bag = Keybag::parse(&fixture("pw", &[(3u32, class_key)])).unwrap();
        let unlocked = bag.unlock("pw").unwrap();

        let file_key = [0x5Au8; 32];
        let wrapped = aes_wrap_key(&class_key, &file_key);
        assert_eq!(*unlocked.unwrap_key(3, &wrapped).unwrap(), file_key);

        // A class the backup does not carry is a clear error, not a panic.
        assert!(matches!(
            unlocked.unwrap_key(7, &wrapped),
            Err(Error::MissingClassKey(7))
        ));
    }

    #[test]
    fn device_wrapped_keys_are_skipped_not_failed() {
        let mut blob = fixture("pw", &[(3u32, [0x33u8; 32])]);
        // Append a class whose key never leaves the phone (WRAP without bit 1).
        blob.extend(tlv(b"UUID", &[0x99; 16]));
        blob.extend(tlv(b"CLAS", &2u32.to_be_bytes()));
        blob.extend(tlv(b"WRAP", &1u32.to_be_bytes()));
        blob.extend(tlv(b"WPKY", &[0u8; 40]));

        let unlocked = Keybag::parse(&blob).unwrap().unlock("pw").unwrap();
        assert_eq!(unlocked.keys.len(), 1);
        assert!(unlocked.keys.contains_key(&3));
    }

    #[test]
    fn a_truncated_blob_does_not_panic() {
        let full = fixture("pw", &[(3u32, [0x33u8; 32])]);
        for cut in 0..full.len() {
            let _ = Keybag::parse(&full[..cut]);
        }
    }

    #[test]
    fn pre_10_2_backups_use_the_sha1_pass_alone() {
        let salt = b"0123456789abcdef";
        let iter = 100u32;
        let key = [0x44u8; 32];
        let mut derived = [0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha1>>(b"pw", salt, iter, &mut derived).unwrap();

        let mut blob = Vec::new();
        blob.extend(tlv(b"VERS", &3u32.to_be_bytes()));
        blob.extend(tlv(b"UUID", &[0xAA; 16]));
        blob.extend(tlv(b"SALT", salt));
        blob.extend(tlv(b"ITER", &iter.to_be_bytes()));
        blob.extend(tlv(b"UUID", &[0x03; 16]));
        blob.extend(tlv(b"CLAS", &3u32.to_be_bytes()));
        blob.extend(tlv(b"WRAP", &WRAP_PASSCODE.to_be_bytes()));
        blob.extend(tlv(b"WPKY", &aes_wrap_key(&derived, &key)));

        let bag = Keybag::parse(&blob).unwrap();
        assert!(bag.dp_salt.is_none());
        assert_eq!(bag.unlock("pw").unwrap().keys[&3], key);
    }
}

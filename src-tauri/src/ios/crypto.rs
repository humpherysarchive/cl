//! Primitives used by the backup format.
//!
//! Two things live here: RFC 3394 AES key unwrapping, which every key in a
//! keybag is protected with, and the zero-IV AES-256-CBC that file contents
//! use. Neither is exotic, but both have to match Apple's usage exactly, so
//! they are kept apart from the parsing code and tested on their own.

use aes::cipher::block_padding::NoPadding;
#[cfg(test)]
use aes::cipher::BlockEncrypt;
use aes::cipher::{BlockDecrypt, BlockDecryptMut, KeyInit, KeyIvInit};
use aes::Aes256;
use zeroize::Zeroizing;

use super::Error;

type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// The fixed check value RFC 3394 puts in A when wrapping succeeds.
const IV: [u8; 8] = [0xa6; 8];

/// AES key unwrap (RFC 3394).
///
/// Apple wraps every class key and every per-file key this way. Input is a
/// multiple of 8 bytes and at least 24; output is 8 bytes shorter. A wrong
/// key shows up as a failed integrity check rather than garbage output,
/// which is what lets a wrong backup password be reported cleanly.
pub fn aes_unwrap_key(kek: &[u8; 32], wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    if wrapped.len() < 24 || wrapped.len() % 8 != 0 {
        return Err(Error::Crypto("wrapped key has an invalid length"));
    }

    let n = wrapped.len() / 8 - 1;
    let mut a = [0u8; 8];
    a.copy_from_slice(&wrapped[..8]);

    let mut r = Zeroizing::new(wrapped[8..].to_vec());
    let cipher = Aes256::new(kek.into());

    // 6 rounds, counting down, per the spec.
    for j in (0..6i64).rev() {
        for i in (1..=n).rev() {
            let t = (n as i64 * j) + i as i64;

            let mut block = [0u8; 16];
            // A XOR t, big-endian over the low 8 bytes.
            for (k, byte) in a.iter().enumerate() {
                block[k] = byte ^ ((t >> (8 * (7 - k))) & 0xff) as u8;
            }
            block[8..].copy_from_slice(&r[(i - 1) * 8..i * 8]);

            cipher.decrypt_block((&mut block).into());

            a.copy_from_slice(&block[..8]);
            r[(i - 1) * 8..i * 8].copy_from_slice(&block[8..]);
        }
    }

    if a != IV {
        return Err(Error::WrongKey);
    }
    Ok(r)
}

/// AES key wrap (RFC 3394). Only needed to build fixtures for the tests, but
/// kept next to its inverse so the two stay in step.
#[cfg(test)]
pub fn aes_wrap_key(kek: &[u8; 32], plain: &[u8]) -> Vec<u8> {
    assert!(plain.len() >= 16 && plain.len() % 8 == 0);
    let n = plain.len() / 8;
    let mut a = IV;
    let mut r = plain.to_vec();
    let cipher = Aes256::new(kek.into());

    for j in 0..6i64 {
        for i in 1..=n {
            let mut block = [0u8; 16];
            block[..8].copy_from_slice(&a);
            block[8..].copy_from_slice(&r[(i - 1) * 8..i * 8]);

            cipher.encrypt_block((&mut block).into());

            let t = (n as i64 * j) + i as i64;
            for (k, byte) in block[..8].iter().enumerate() {
                a[k] = byte ^ ((t >> (8 * (7 - k))) & 0xff) as u8;
            }
            r[(i - 1) * 8..i * 8].copy_from_slice(&block[8..]);
        }
    }

    let mut out = Vec::with_capacity(plain.len() + 8);
    out.extend_from_slice(&a);
    out.extend_from_slice(&r);
    out
}

/// Decrypt file contents: AES-256-CBC with an all-zero IV.
///
/// Apple pads to the block size but records the true length separately, so
/// the caller passes `size` and the plaintext is truncated to it rather than
/// trusting the padding — which is absent or wrong in some backups.
pub fn decrypt_cbc(key: &[u8; 32], data: &[u8], size: Option<u64>) -> Result<Vec<u8>, Error> {
    if data.len() % 16 != 0 {
        return Err(Error::Crypto("ciphertext is not a whole number of blocks"));
    }

    let mut out = data.to_vec();
    let iv = [0u8; 16];
    Aes256CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_mut::<NoPadding>(&mut out)
        .map_err(|_| Error::Crypto("could not decrypt file contents"))?;

    match size {
        Some(n) if (n as usize) <= out.len() => out.truncate(n as usize),
        // A recorded size longer than the ciphertext means a damaged entry;
        // hand back what we have rather than failing the whole import.
        _ => {}
    }
    Ok(out)
}

/// Encrypt the way the format does, for building test fixtures. Callers pad
/// to the block size themselves and record the true length separately.
#[cfg(test)]
pub fn encrypt_cbc(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    use aes::cipher::BlockEncryptMut;
    assert_eq!(data.len() % 16, 0);
    let mut out = data.to_vec();
    let mut enc = cbc::Encryptor::<Aes256>::new(key.into(), &[0u8; 16].into());
    for block in out.chunks_mut(16) {
        enc.encrypt_block_mut(block.into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 3394 §4.6: wrapping a 256-bit key with a 256-bit KEK.
    #[test]
    fn rfc3394_vector() {
        let kek = hex("000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F");
        let plain = hex("00112233445566778899AABBCCDDEEFF000102030405060708090A0B0C0D0E0F");
        let wrapped = hex("28C9F404C4B810F4CBCCB35CFB87F8263F5786E2D80ED326\
             CBC7F0E71A99F43BFB988B9B7A02DD21");

        let kek: [u8; 32] = kek.try_into().unwrap();
        assert_eq!(aes_wrap_key(&kek, &plain), wrapped);
        assert_eq!(aes_unwrap_key(&kek, &wrapped).unwrap().to_vec(), plain);
    }

    #[test]
    fn unwrap_rejects_the_wrong_key() {
        let kek = [7u8; 32];
        let wrong = [8u8; 32];
        let wrapped = aes_wrap_key(&kek, &[1u8; 32]);
        assert!(matches!(
            aes_unwrap_key(&wrong, &wrapped),
            Err(Error::WrongKey)
        ));
    }

    #[test]
    fn unwrap_rejects_a_bad_length() {
        let kek = [7u8; 32];
        assert!(aes_unwrap_key(&kek, &[0u8; 20]).is_err());
        assert!(aes_unwrap_key(&kek, &[0u8; 25]).is_err());
    }

    #[test]
    fn cbc_truncates_to_the_recorded_size() {
        use aes::cipher::BlockEncryptMut;
        let key = [3u8; 32];
        let mut buf = [0u8; 32];
        buf[..11].copy_from_slice(b"hello there");

        // Encrypt with the same zero IV the format uses.
        let mut enc = cbc::Encryptor::<Aes256>::new(&key.into(), &[0u8; 16].into());
        for chunk in buf.chunks_mut(16) {
            enc.encrypt_block_mut(chunk.into());
        }

        let out = decrypt_cbc(&key, &buf, Some(11)).unwrap();
        assert_eq!(out, b"hello there");
    }

    fn hex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }
}

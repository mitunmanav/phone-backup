//! Password protection for backups. A backup with a password has one random data key. The
//! password, run through Argon2id, unlocks (unwraps) that key; the password itself is never
//! stored. Each protected file `X` is saved as `X.pbenc`:
//!
//! - 4 bytes `PBE1`, then a random 32-byte file id;
//! - the file in 1 MiB chunks, each sealed with AES-256-GCM (16-byte tag) under a key made for
//!   this file alone (HKDF-SHA256 of the data key and the file id). Chunk `i` uses the nonce
//!   `0000000 ‖ i (big-endian u32) ‖ last`, where `last` is 1 only on the final chunk, so a file
//!   cut short, reordered or with a changed byte fails to open instead of opening wrong.
//!
//! Files are read and written a chunk at a time, so videos never sit whole in memory.
// Callers arrive with the writers and readers in the next steps.
#![allow(dead_code)]

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{Aead, KeyInit, Nonce};
use std::io::{self, Read, Write};
use zeroize::Zeroizing;

/// The name ending of a protected file.
pub const EXT: &str = "pbenc";
const MAGIC: &[u8; 4] = b"PBE1";
const ID: usize = 32;
/// Bytes before the first chunk: magic and file id.
pub const HEAD: u64 = (MAGIC.len() + ID) as u64;
const CHUNK: usize = 1 << 20;
const TAG: usize = 16;
const FILE_INFO: &[u8] = b"phone-backup file key v1";

/// Argon2id settings: 64 MiB of memory, 3 passes, 1 lane.
pub const ARGON_M: u32 = 64 * 1024;
pub const ARGON_T: u32 = 3;
pub const ARGON_P: u32 = 1;

/// The key every file of one backup is sealed with. Wiped from memory when dropped.
#[derive(Clone)]
pub struct Key(Zeroizing<[u8; 32]>);

impl Key {
    /// A new random key.
    pub fn random() -> Key {
        Key(Zeroizing::new(random()))
    }

    pub(super) fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(super) fn from_bytes(b: [u8; 32]) -> Key {
        Key(Zeroizing::new(b))
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(…)")
    }
}

/// Where the protected copy of `path` is kept: `path` with `.pbenc` added.
pub fn sealed_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".");
    p.push(EXT);
    p.into()
}

/// Random bytes from the operating system.
pub fn random<const N: usize>() -> [u8; N] {
    let mut b = [0; N];
    // The OS random source does not fail on any Windows or Linux this app runs on.
    getrandom::fill(&mut b).expect("the system random number source failed");
    b
}

/// The key a password gives with this salt and these Argon2id settings.
pub fn from_password(password: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<Key, String> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(m, t, p, Some(32)).map_err(|e| format!("Bad key settings: {e}"))?;
    let mut out = [0; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| format!("Could not make the key: {e}"))?;
    Ok(Key::from_bytes(out))
}

/// Seals `key` with `with`: a random 12-byte nonce, then the sealed key and its tag (60 bytes).
pub fn wrap(with: &Key, key: &Key) -> Vec<u8> {
    let nonce: [u8; 12] = random();
    let sealed = Aes256Gcm::new(with.bytes().into())
        .encrypt(&Nonce::<Aes256Gcm>::from(nonce), key.bytes().as_slice())
        .expect("a 32-byte key always seals");
    [nonce.as_slice(), &sealed].concat()
}

/// Opens what `wrap` sealed. None for the wrong key or damaged bytes.
pub fn unwrap(with: &Key, wrapped: &[u8]) -> Option<Key> {
    let (nonce, sealed) = wrapped.split_at_checked(12)?;
    let nonce: [u8; 12] = nonce.try_into().ok()?;
    let open = Zeroizing::new(
        Aes256Gcm::new(with.bytes().into())
            .decrypt(&Nonce::<Aes256Gcm>::from(nonce), sealed)
            .ok()?,
    );
    Some(Key::from_bytes(open.as_slice().try_into().ok()?))
}

/// The key for one file: HKDF-SHA256 of the data key, salted with the file's id.
fn file_cipher(key: &Key, id: &[u8; ID]) -> Aes256Gcm {
    let mut k = Zeroizing::new([0; 32]);
    hkdf::Hkdf::<sha2::Sha256>::new(Some(id), key.bytes())
        .expand(FILE_INFO, k.as_mut_slice())
        .expect("32 bytes is a valid HKDF length");
    Aes256Gcm::new((&*k).into())
}

fn chunk_nonce(i: u32, last: bool) -> Nonce<Aes256Gcm> {
    let mut n = [0; 12];
    n[7..11].copy_from_slice(&i.to_be_bytes());
    n[11] = u8::from(last);
    Nonce::<Aes256Gcm>::from(n)
}

/// Reads until `buf` is full or the input ends. Returns how much was read.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

fn damaged() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "the password does not open it, or the file is damaged",
    )
}

/// Seals everything `r` gives into `w`. Returns the number of plain bytes.
pub fn encrypt(key: &Key, r: &mut impl Read, w: &mut impl Write) -> io::Result<u64> {
    encrypt_with_id(key, &random(), r, w)
}

fn encrypt_with_id(
    key: &Key,
    id: &[u8; ID],
    r: &mut impl Read,
    w: &mut impl Write,
) -> io::Result<u64> {
    let cipher = file_cipher(key, id);
    w.write_all(MAGIC)?;
    w.write_all(id)?;
    let (mut cur, mut next) = (
        Zeroizing::new(vec![0; CHUNK]),
        Zeroizing::new(vec![0; CHUNK]),
    );
    let mut n = read_full(r, &mut cur)?;
    let (mut i, mut total) = (0u32, 0u64);
    loop {
        // Only a short chunk, or a full one with nothing after it, is the last.
        let m = if n == CHUNK {
            read_full(r, &mut next)?
        } else {
            0
        };
        let last = m == 0;
        let sealed = cipher
            .encrypt(&chunk_nonce(i, last), &cur[..n])
            .map_err(|_| io::Error::other("could not seal a chunk"))?;
        w.write_all(&sealed)?;
        total += n as u64;
        if last {
            return Ok(total);
        }
        i = i
            .checked_add(1)
            .ok_or_else(|| io::Error::other("the file is too large to protect"))?;
        std::mem::swap(&mut cur, &mut next);
        n = m;
    }
}

/// Opens a sealed stream from `r` into `w`. Fails, having maybe written part, when the key is
/// wrong or the file was changed or cut short. Returns the number of plain bytes.
pub fn decrypt(key: &Key, r: &mut impl Read, w: &mut impl Write) -> io::Result<u64> {
    let mut head = [0; HEAD as usize];
    if read_full(r, &mut head)? < head.len() || &head[..4] != MAGIC {
        return Err(damaged());
    }
    let id: [u8; ID] = head[4..].try_into().expect("HEAD is magic and id");
    let cipher = file_cipher(key, &id);
    let full = CHUNK + TAG;
    let (mut cur, mut next) = (vec![0; full], vec![0; full]);
    let mut n = read_full(r, &mut cur)?;
    let (mut i, mut total) = (0u32, 0u64);
    loop {
        let m = if n == full {
            read_full(r, &mut next)?
        } else {
            0
        };
        let last = m == 0;
        if n < TAG {
            return Err(damaged());
        }
        let open = Zeroizing::new(
            cipher
                .decrypt(&chunk_nonce(i, last), &cur[..n])
                .map_err(|_| damaged())?,
        );
        w.write_all(&open)?;
        total += open.len() as u64;
        if last {
            return Ok(total);
        }
        i = i.checked_add(1).ok_or_else(damaged)?;
        std::mem::swap(&mut cur, &mut next);
        n = m;
    }
}

/// Seals bytes in memory.
pub fn encrypt_bytes(key: &Key, plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sealed_len(plain.len() as u64) as usize);
    encrypt(key, &mut &plain[..], &mut out).expect("writing to memory does not fail");
    out
}

/// Opens bytes sealed by `encrypt_bytes` or `encrypt`.
pub fn decrypt_bytes(key: &Key, sealed: &[u8]) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::new());
    decrypt(key, &mut &sealed[..], &mut *out)?;
    Ok(out)
}

/// The size of a sealed file holding `plain` bytes.
pub fn sealed_len(plain: u64) -> u64 {
    let chunks = plain.div_ceil(CHUNK as u64).max(1);
    HEAD + plain + chunks * TAG as u64
}

/// The plain size of a sealed file of `sealed` bytes, without opening it. None when no sealed
/// file has that size.
pub fn plain_len(sealed: u64) -> Option<u64> {
    let body = sealed.checked_sub(HEAD)?;
    let chunks = body.div_ceil((CHUNK + TAG) as u64).max(1);
    let plain = body.checked_sub(chunks * TAG as u64)?;
    (sealed_len(plain) == sealed).then_some(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: u8) -> Key {
        Key::from_bytes([b; 32])
    }

    fn sum(bytes: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Bytes that differ from chunk to chunk, so a swap of two chunks would show.
    fn sample(n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| (i % 251) as u8 ^ (i / CHUNK) as u8)
            .collect()
    }

    #[test]
    fn round_trips_every_size() {
        let k = key(7);
        for n in [
            0,
            1,
            15,
            16,
            17,
            CHUNK - 1,
            CHUNK,
            CHUNK + 1,
            2 * CHUNK,
            2 * CHUNK + 5,
        ] {
            let plain = sample(n);
            let sealed = encrypt_bytes(&k, &plain);
            assert_eq!(sealed.len() as u64, sealed_len(n as u64), "size {n}");
            assert_eq!(plain_len(sealed.len() as u64), Some(n as u64), "size {n}");
            assert_eq!(*decrypt_bytes(&k, &sealed).unwrap(), plain, "size {n}");
        }
    }

    #[test]
    fn same_file_seals_differently_each_time() {
        let k = key(7);
        assert_ne!(encrypt_bytes(&k, b"same"), encrypt_bytes(&k, b"same"));
    }

    #[test]
    fn known_answer_keeps_the_format_fixed() {
        // A change here means files saved by an older version would no longer open. Both sums
        // were checked against Python's `cryptography` (AESGCM, HKDF, Argon2id).
        let mut out = Vec::new();
        let plain = sample(CHUNK + 100);
        encrypt_with_id(&key(1), &[2; ID], &mut &plain[..], &mut out).unwrap();
        assert_eq!(&out[..4], b"PBE1");
        assert_eq!(&out[4..36], &[2; ID]);
        assert_eq!(sum(&out), KNOWN);
        let small = from_password("correct horse", b"0123456789abcdef", 8, 1, 1).unwrap();
        assert_eq!(sum(small.bytes()), KNOWN_ARGON);
    }

    const KNOWN: &str = "83dc082c64582525b4b3f3d34d5269f387258e116c7557d8851069356e7d8f51";
    const KNOWN_ARGON: &str = "e88cb49eb353122e64b06ad822b7ed54ac53fef3ef1d3acb896620522b151f8a";

    #[test]
    fn wrong_key_fails_cleanly() {
        let sealed = encrypt_bytes(&key(1), b"secret messages");
        let err = decrypt_bytes(&key(2), &sealed).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn cut_or_changed_files_fail() {
        let k = key(3);
        let sealed = encrypt_bytes(&k, &sample(2 * CHUNK + 10));
        // Cut at the end, cut exactly after a whole chunk, and cut inside the header.
        let chunk_end = HEAD as usize + CHUNK + TAG;
        for cut in [sealed.len() - 1, chunk_end, 10, 0] {
            assert!(decrypt_bytes(&k, &sealed[..cut]).is_err(), "cut at {cut}");
        }
        for at in [0, 5, HEAD as usize, chunk_end + 3, sealed.len() - 1] {
            let mut bad = sealed.clone();
            bad[at] ^= 1;
            assert!(decrypt_bytes(&k, &bad).is_err(), "changed byte {at}");
        }
        // Two chunks swapped.
        let one = HEAD as usize..chunk_end;
        let two = chunk_end..chunk_end + CHUNK + TAG;
        let mut swapped = sealed[..HEAD as usize].to_vec();
        swapped.extend_from_slice(&sealed[two]);
        swapped.extend_from_slice(&sealed[one]);
        swapped.extend_from_slice(&sealed[chunk_end + CHUNK + TAG..]);
        assert!(decrypt_bytes(&k, &swapped).is_err());
    }

    #[test]
    fn wraps_and_unwraps_the_data_key() {
        let (pw, data) = (key(4), Key::random());
        let wrapped = wrap(&pw, &data);
        assert_eq!(wrapped.len(), 12 + 32 + 16);
        assert_eq!(unwrap(&pw, &wrapped).unwrap().bytes(), data.bytes());
        assert!(unwrap(&key(5), &wrapped).is_none());
        assert!(unwrap(&pw, &wrapped[..20]).is_none());
    }

    #[test]
    fn password_gives_the_same_key_only_with_the_same_salt() {
        let a = from_password("pw", b"saltsaltsaltsalt", 8, 1, 1).unwrap();
        let b = from_password("pw", b"saltsaltsaltsalt", 8, 1, 1).unwrap();
        let c = from_password("pw", b"other salt here!", 8, 1, 1).unwrap();
        let d = from_password("pW", b"saltsaltsaltsalt", 8, 1, 1).unwrap();
        assert_eq!(a.bytes(), b.bytes());
        assert_ne!(a.bytes(), c.bytes());
        assert_ne!(a.bytes(), d.bytes());
    }

    #[test]
    fn no_sealed_file_has_impossible_sizes() {
        assert_eq!(plain_len(0), None);
        assert_eq!(plain_len(HEAD + TAG as u64 - 1), None);
        assert_eq!(plain_len(HEAD + TAG as u64), Some(0));
    }
}

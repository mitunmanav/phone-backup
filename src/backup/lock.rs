//! A backup's password: `encryption.json` beside `checksums.sha256`, and the keys unlocked in
//! this session. The file holds the Argon2id settings and salt, the data key wrapped by the
//! password's key, what the password protects, and the data key wrapped by the recovery key.
//! It never holds the password or the data key itself.
// Callers arrive with the writers, readers and settings in the next steps.
#![allow(dead_code)]

use super::crypt::{self, Key};
use super::*;
use std::sync::Mutex;

/// The file that marks a backup as password protected.
pub const LOCK: &str = "encryption.json";

/// What a backup's password protects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Contacts, calls, messages, calendar and picture message files (`data/`).
    Data,
    /// Everything copied from the phone: also files, SD cards and apps.
    All,
}

impl Scope {
    fn word(self) -> &'static str {
        match self {
            Scope::Data => "data",
            Scope::All => "all",
        }
    }
}

/// A backup's `encryption.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lock {
    /// Random, so an unlocked key follows the backup through a rename or a move.
    pub id: String,
    salt: Vec<u8>,
    m: u32,
    t: u32,
    p: u32,
    /// The data key wrapped by the password's key.
    wrapped: Vec<u8>,
    pub scope: Scope,
    /// The data key wrapped by the recovery key's key, when one was made.
    recovery: Option<Vec<u8>>,
}

impl Lock {
    fn to_json(&self) -> String {
        let v = serde_json::json!({
            "version": 1,
            "id": self.id,
            "kdf": "argon2id",
            "m": self.m,
            "t": self.t,
            "p": self.p,
            "salt": hex(&self.salt),
            "key": hex(&self.wrapped),
            "scope": self.scope.word(),
            "recovery": self.recovery.as_deref().map(hex),
        });
        serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
    }

    fn from_json(text: &str) -> Result<Lock, String> {
        let bad = || format!("{LOCK} is damaged or from a newer version of Phone Backup.");
        let v: serde_json::Value = serde_json::from_str(text).map_err(|_| bad())?;
        if v["version"].as_u64() != Some(1) || v["kdf"].as_str() != Some("argon2id") {
            return Err(bad());
        }
        let num = |k: &str| {
            v[k].as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(bad)
        };
        let bytes = |k: &str| v[k].as_str().and_then(unhex).ok_or_else(bad);
        Ok(Lock {
            id: v["id"].as_str().ok_or_else(bad)?.to_string(),
            salt: bytes("salt")?,
            m: num("m")?,
            t: num("t")?,
            p: num("p")?,
            wrapped: bytes("key")?,
            scope: match v["scope"].as_str() {
                Some("data") => Scope::Data,
                Some("all") => Scope::All,
                _ => return Err(bad()),
            },
            recovery: match &v["recovery"] {
                serde_json::Value::Null => None,
                r => Some(r.as_str().and_then(unhex).ok_or_else(bad)?),
            },
        })
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    s.len()
        .is_multiple_of(2)
        .then(|| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
                .collect()
        })
        .flatten()
}

/// Argon2id settings for new passwords. Tests use tiny ones so they run fast.
fn settings() -> (u32, u32, u32) {
    if cfg!(test) {
        (8, 1, 1)
    } else {
        (crypt::ARGON_M, crypt::ARGON_T, crypt::ARGON_P)
    }
}

/// The keys unlocked in this session, by backup id. Wiped when the app closes.
static KEYS: Mutex<BTreeMap<String, Key>> = Mutex::new(BTreeMap::new());

fn keys() -> std::sync::MutexGuard<'static, BTreeMap<String, Key>> {
    KEYS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The backup's password file, or None when it has no password. Err when the file is there
/// but cannot be read: such a backup must be treated as protected, never written as plain.
pub fn lock_of(root: &Path) -> Result<Option<Lock>, String> {
    match fs::read_to_string(root.join(LOCK)) {
        Ok(text) => Lock::from_json(&text).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot read {LOCK}: {e}")),
    }
}

/// True when the backup has a password, or a password file that cannot be read.
pub fn has_password(root: &Path) -> bool {
    root.join(LOCK).exists()
}

fn save(root: &Path, lock: &Lock) -> Result<(), String> {
    let part = root.join(format!("{LOCK}.part"));
    fs::write(&part, lock.to_json())
        .and_then(|()| fs::rename(&part, root.join(LOCK)))
        .map_err(|e| {
            let _ = fs::remove_file(&part);
            format!("Cannot save {LOCK}: {e}")
        })
}

/// Gives the backup a password: a new data key, wrapped by the password, saved in
/// `encryption.json` and unlocked for this session. Files already in the backup are not
/// changed here.
pub fn set_password(root: &Path, password: &str, scope: Scope) -> Result<(), String> {
    if has_password(root) {
        return Err("This backup already has a password.".into());
    }
    fs::create_dir_all(root).map_err(|e| format!("Cannot create {}: {e}", root.display()))?;
    let (m, t, p) = settings();
    let salt: [u8; 16] = crypt::random();
    let data = Key::random();
    let lock = Lock {
        id: hex(&crypt::random::<16>()),
        salt: salt.to_vec(),
        m,
        t,
        p,
        wrapped: crypt::wrap(&crypt::from_password(password, &salt, m, t, p)?, &data),
        scope,
        recovery: None,
    };
    save(root, &lock)?;
    keys().insert(lock.id, data);
    Ok(())
}

/// Unlocks the backup for this session. Wrong passwords say so.
pub fn unlock(root: &Path, password: &str) -> Result<(), String> {
    let lock = lock_of(root)?.ok_or("This backup has no password.")?;
    let with = crypt::from_password(password, &lock.salt, lock.m, lock.t, lock.p)?;
    let data =
        crypt::unwrap(&with, &lock.wrapped).ok_or("That password does not open this backup.")?;
    keys().insert(lock.id, data);
    Ok(())
}

/// Replaces the password of an unlocked backup. Only the small wrapped key changes.
pub fn change_password(root: &Path, password: &str) -> Result<(), String> {
    let mut lock = lock_of(root)?.ok_or("This backup has no password.")?;
    let data = key_of(&lock).ok_or("Unlock the backup first.")?;
    let (m, t, p) = settings();
    let salt: [u8; 16] = crypt::random();
    lock.wrapped = crypt::wrap(&crypt::from_password(password, &salt, m, t, p)?, &data);
    (lock.salt, lock.m, lock.t, lock.p) = (salt.to_vec(), m, t, p);
    save(root, &lock)
}

/// Sets what the password protects. Callers convert the files to match.
pub fn set_scope(root: &Path, scope: Scope) -> Result<(), String> {
    let mut lock = lock_of(root)?.ok_or("This backup has no password.")?;
    lock.scope = scope;
    save(root, &lock)
}

/// Removes the password file. Callers first open every protected file.
pub(super) fn drop_password(root: &Path) -> Result<(), String> {
    if let Ok(Some(lock)) = lock_of(root) {
        keys().remove(&lock.id);
    }
    match fs::remove_file(root.join(LOCK)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("Cannot remove {LOCK}: {e}"))
        }
        _ => Ok(()),
    }
}

fn key_of(lock: &Lock) -> Option<Key> {
    keys().get(&lock.id).cloned()
}

/// The backup's data key, when it has a password and is unlocked in this session.
pub fn key(root: &Path) -> Option<Key> {
    key_of(&lock_of(root).ok()??)
}

/// True when the backup has a password that was not entered in this session.
pub fn locked(root: &Path) -> bool {
    has_password(root) && key(root).is_none()
}

/// Forgets the backup's key, so the password is asked again.
pub fn forget(root: &Path) {
    if let Ok(Some(lock)) = lock_of(root) {
        keys().remove(&lock.id);
    }
}

/// Forgets every unlocked key; they are wiped from memory.
pub fn forget_keys() {
    keys().clear();
}

/// `rel` (a path inside the backup, with `/`) without the `history/<run>/` in front.
pub fn latest_rel(rel: &str) -> &str {
    rel.strip_prefix(HISTORY)
        .and_then(|r| r.strip_prefix('/'))
        .and_then(|r| r.split_once('/'))
        .map_or(rel, |(_, r)| r)
}

/// True when a file at `rel` is protected under `scope`. The backup's own lists (checksums,
/// report, names, `run.json`) and exports are never protected.
pub fn protects(scope: Scope, rel: &str) -> bool {
    let rel = latest_rel(rel);
    let top = rel.split('/').next().unwrap_or_default();
    if !rel.contains('/') {
        return false;
    }
    match scope {
        Scope::Data => top == "data",
        // Also files the app makes from them, such as the HEIC tool's JPGs.
        Scope::All => top != crate::export::EXPORTS && top != HISTORY,
    }
}

/// The error for a sealed file whose backup is not unlocked.
fn locked_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, LOCKED)
}

/// The backup folder holding `path`: the nearest folder above it with a password file.
pub fn root_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .skip(1)
        .find(|a| a.join(LOCK).is_file())
        .map(Path::to_path_buf)
}

/// True for a sealed file (`.pbenc`).
pub fn is_sealed(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == crypt::EXT)
}

/// `path` without `.pbenc`: the file's own name.
pub fn plain_path(path: &Path) -> PathBuf {
    if is_sealed(path) {
        path.with_extension("")
    } else {
        path.to_path_buf()
    }
}

/// Where the copy of `path` is kept: `path` itself, or its sealed copy. None when neither is.
pub fn stored(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }
    let sealed = crypt::sealed_path(path);
    sealed.exists().then_some(sealed)
}

/// The key that opens a sealed file: its backup's, when unlocked in this session.
fn key_near(sealed: &Path) -> std::io::Result<crypt::Key> {
    root_of(sealed)
        .and_then(|r| key(&r))
        .ok_or_else(locked_error)
}

/// Reads a file of a backup. `path` may name the file or its sealed copy; a sealed copy is
/// opened with its backup's key.
pub fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    let at = stored(path).ok_or(std::io::ErrorKind::NotFound)?;
    if !is_sealed(&at) {
        return fs::read(at);
    }
    let k = key_near(&at)?;
    let mut out = Vec::new();
    crypt::decrypt(
        &k,
        &mut std::io::BufReader::new(fs::File::open(&at)?),
        &mut out,
    )?;
    Ok(out)
}

/// `read_file` as text, or None when it cannot be read.
pub fn read_text(path: &Path) -> Option<String> {
    String::from_utf8(read_file(path).ok()?).ok()
}

struct Hasher(sha2::Sha256);

impl std::io::Write for Hasher {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        sha2::Digest::update(&mut self.0, b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// SHA-256 of a file's own bytes: a sealed copy is opened first. Read in chunks.
pub fn plain_sha256(path: &Path) -> std::io::Result<[u8; 32]> {
    let at = stored(path).ok_or(std::io::ErrorKind::NotFound)?;
    if !is_sealed(&at) {
        return sha256_bytes(&at);
    }
    let k = key_near(&at)?;
    let mut h = Hasher(sha2::Digest::new());
    crypt::decrypt(
        &k,
        &mut std::io::BufReader::new(fs::File::open(&at)?),
        &mut h,
    )?;
    Ok(sha2::Digest::finalize(h.0).into())
}

/// Copies the file at `src` (plain or sealed) to `dst`, sealing it with `seal` (then saved as
/// `dst.pbenc`). Written to a side file first. Returns where the copy is.
pub fn copy_plain(src: &Path, dst: &Path, seal: Option<&crypt::Key>) -> std::io::Result<PathBuf> {
    let at = stored(src).ok_or(std::io::ErrorKind::NotFound)?;
    let target = match seal {
        Some(_) => crypt::sealed_path(dst),
        None => dst.to_path_buf(),
    };
    if !is_sealed(&at) && seal.is_none() {
        fs::copy(&at, &target)?;
        return Ok(target);
    }
    let mut part = target.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = (|| {
        let mut r = std::io::BufReader::new(fs::File::open(&at)?);
        let mut w = std::io::BufWriter::new(fs::File::create(&part)?);
        match (is_sealed(&at), seal) {
            (true, None) => crypt::decrypt(&key_near(&at)?, &mut r, &mut w)?,
            (false, Some(k)) => crypt::encrypt(k, &mut r, &mut w)?,
            (true, Some(k)) => {
                // Opened into memory first; such copies are between two protected backups.
                let mut plain = zeroize::Zeroizing::new(Vec::new());
                crypt::decrypt(&key_near(&at)?, &mut r, &mut *plain)?;
                crypt::encrypt(k, &mut &plain[..], &mut w)?
            }
            (false, None) => unreachable!("plain to plain is a copy"),
        };
        std::io::Write::flush(&mut w)?;
        drop(w);
        fs::rename(&part, &target)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    Ok(target)
}

/// The key to seal a file at `path` with, when backup `root`'s password protects that place.
/// Err when it protects it but was not unlocked.
pub fn seal_in(root: &Path, path: &Path) -> std::io::Result<Option<crypt::Key>> {
    match lock_of(root).map_err(std::io::Error::other)? {
        Some(l) if protects(l.scope, &rel(root, path)) => {
            key_of(&l).map(Some).ok_or_else(locked_error)
        }
        _ => Ok(None),
    }
}

/// Seals the plain file `tmp` into a side file beside `sealed`, with the file time `mtime`
/// (Unix seconds) when given, and deletes `tmp`. Returns the side file, for the caller to
/// rename into place once the older copy is kept.
pub fn seal_beside(
    tmp: &Path,
    sealed: &Path,
    key: &crypt::Key,
    mtime: Option<i64>,
) -> std::io::Result<PathBuf> {
    let mut part = sealed.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = (|| {
        let mut r = std::io::BufReader::new(fs::File::open(tmp)?);
        let mut w = std::io::BufWriter::new(fs::File::create(&part)?);
        crypt::encrypt(key, &mut r, &mut w)?;
        let file = w.into_inner().map_err(|e| e.into_error())?;
        if let Some(t) = mtime {
            file.set_modified(UNIX_EPOCH + Duration::from_secs(t.max(0) as u64))?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(tmp);
    match written {
        Ok(()) => Ok(part),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

/// The plain size of the file stored at `path`, sealed or not.
pub fn plain_size(path: &Path, stored_len: u64) -> Option<u64> {
    if is_sealed(path) {
        crypt::plain_len(stored_len)
    } else {
        Some(stored_len)
    }
}

/// True when reading the backup needs a password not yet entered: for its personal data, or
/// with `files` for its files and apps, which only scope "all" protects.
pub fn needs_password(root: &Path, files: bool) -> bool {
    locked(root)
        && (!files
            || lock_of(root)
                .map(|l| l.is_none_or(|l| l.scope == Scope::All))
                .unwrap_or(true))
}

/// Writes `bytes` to `path` in backup `root`, sealed when its password protects that place.
/// Returns where the file is.
pub fn write_file(root: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    match seal_in(root, path)? {
        Some(k) => {
            let sealed = crypt::sealed_path(path);
            fs::write(&sealed, crypt::encrypt_bytes(&k, bytes))?;
            Ok(sealed)
        }
        None => {
            fs::write(path, bytes)?;
            Ok(path.to_path_buf())
        }
    }
}

/// The folder for opened copies of sealed files, emptied when the app closes.
fn open_dir() -> PathBuf {
    std::env::temp_dir().join(format!("phone-backup-open-{}", std::process::id()))
}

/// A plain copy of a sealed file in a temporary folder, for opening in another program.
/// Plain files are returned as they are.
pub fn open_copy(path: &Path) -> std::io::Result<PathBuf> {
    let at = stored(path).ok_or(std::io::ErrorKind::NotFound)?;
    if !is_sealed(&at) {
        return Ok(at);
    }
    // A folder per file keeps two attachments of the same name apart.
    let dir = open_dir().join(hex(&crypt::random::<4>()));
    fs::create_dir_all(&dir)?;
    let name = plain_path(&at);
    copy_plain(&at, &dir.join(name.file_name().unwrap_or_default()), None)
}

/// Deletes the opened copies of sealed files.
pub fn remove_open_copies() {
    let _ = fs::remove_dir_all(open_dir());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pb-lock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn password_unlocks_and_wrong_password_does_not() {
        let root = dir("unlock");
        assert!(!has_password(&root) && !locked(&root));
        set_password(&root, "pw one", Scope::Data).unwrap();
        let data = key(&root).unwrap();
        assert!(set_password(&root, "again", Scope::Data).is_err());
        let text = fs::read_to_string(root.join(LOCK)).unwrap();
        assert!(!text.contains("pw one"));
        assert!(!text.contains(&hex(data.bytes())));

        forget(&root);
        assert!(locked(&root));
        assert_eq!(
            unlock(&root, "pw two"),
            Err("That password does not open this backup.".into())
        );
        unlock(&root, "pw one").unwrap();
        assert_eq!(key(&root).unwrap().bytes(), data.bytes());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn changing_the_password_keeps_the_data_key() {
        let root = dir("change");
        set_password(&root, "old", Scope::All).unwrap();
        let data = key(&root).unwrap();
        change_password(&root, "new").unwrap();
        forget(&root);
        assert!(unlock(&root, "old").is_err());
        unlock(&root, "new").unwrap();
        assert_eq!(key(&root).unwrap().bytes(), data.bytes());
        assert_eq!(lock_of(&root).unwrap().unwrap().scope, Scope::All);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unlocked_key_follows_a_renamed_backup() {
        let root = dir("rename");
        set_password(&root, "pw", Scope::Data).unwrap();
        let moved = root.with_file_name(format!("pb-lock-moved-{}", std::process::id()));
        let _ = fs::remove_dir_all(&moved);
        fs::rename(&root, &moved).unwrap();
        assert!(key(&moved).is_some());
        let _ = fs::remove_dir_all(&moved);
    }

    #[test]
    fn reads_copies_and_hashes_sealed_files() {
        let root = dir("files");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/plain.txt"), "plain").unwrap();
        set_password(&root, "pw", Scope::Data).unwrap();
        let sms = root.join("data/sms.txt");
        let at = write_file(&root, &sms, b"hello").unwrap();
        assert_eq!(at, crypt::sealed_path(&sms));
        assert_eq!(stored(&sms), Some(at.clone()));
        assert_eq!(plain_path(&at), sms);
        assert_eq!(root_of(&at), Some(root.clone()));
        // By either name.
        assert_eq!(read_file(&sms).unwrap(), b"hello");
        assert_eq!(read_file(&at).unwrap(), b"hello");
        assert_eq!(read_file(&root.join("data/plain.txt")).unwrap(), b"plain");
        assert_eq!(plain_sha256(&at).unwrap(), {
            use sha2::Digest;
            <[u8; 32]>::from(sha2::Sha256::digest(b"hello"))
        });
        // Outside the scope stays plain.
        assert_eq!(
            write_file(&root, &root.join("note.txt"), b"x").unwrap(),
            root.join("note.txt")
        );

        let out = dir("files-out");
        let plain = copy_plain(&at, &out.join("sms.txt"), None).unwrap();
        assert_eq!(fs::read(&plain).unwrap(), b"hello");
        let other = Key::random();
        let resealed = copy_plain(&at, &out.join("again.txt"), Some(&other)).unwrap();
        assert_eq!(resealed, out.join("again.txt.pbenc"));
        let back = crypt::decrypt_bytes(&other, &fs::read(&resealed).unwrap()).unwrap();
        assert_eq!(back.as_slice(), b"hello");

        forget(&root);
        let err = read_file(&sms).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(write_file(&root, &sms, b"new").is_err());
        assert_eq!(
            read_file(&sms).map_err(|e| e.kind()),
            Err(std::io::ErrorKind::PermissionDenied)
        );
        assert!(!out.join("bad.txt.part").exists());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn json_round_trips_and_damage_is_refused() {
        let lock = Lock {
            id: "ab".into(),
            salt: vec![1, 2],
            m: 8,
            t: 1,
            p: 1,
            wrapped: vec![255, 0],
            scope: Scope::All,
            recovery: Some(vec![9]),
        };
        assert_eq!(Lock::from_json(&lock.to_json()).unwrap(), lock);
        assert!(Lock::from_json("{}").is_err());
        assert!(Lock::from_json(&lock.to_json().replace("\"all\"", "\"some\"")).is_err());
        assert!(Lock::from_json(&lock.to_json().replace("\"0102\"", "\"0x\"")).is_err());
        let root = dir("damaged");
        fs::write(root.join(LOCK), "not json").unwrap();
        assert!(has_password(&root) && locked(&root));
        assert!(lock_of(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scope_decides_which_files_are_protected() {
        for (rel, data, all) in [
            ("data/sms.txt", true, true),
            ("data/mms/12_a.jpg", true, true),
            ("history/2026-11-10_143205/data/sms.txt", true, true),
            ("files/DCIM/a.jpg", false, true),
            ("sdcard_1/x.txt", false, true),
            ("apps/com.x/base.apk", false, true),
            ("history/2026-11-10_143205/files/DCIM/a.jpg", false, true),
            ("history/2026-11-10_143205/run.json", false, false),
            ("jpg/files/DCIM/a.jpg", false, true),
            ("jpg/files/DCIM/a.jpg", false, true),
            ("checksums.sha256", false, false),
            ("report.txt", false, false),
            ("encryption.json", false, false),
            ("exports/messages.html", false, false),
            ("data", false, false),
        ] {
            assert_eq!(protects(Scope::Data, rel), data, "{rel}");
            assert_eq!(protects(Scope::All, rel), all, "{rel}");
        }
    }
}

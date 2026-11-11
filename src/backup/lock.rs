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
        Scope::All => {
            top == "data" || top == "files" || top == "apps" || top.starts_with("sdcard_")
        }
    }
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

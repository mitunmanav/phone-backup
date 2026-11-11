//! Older versions of a backup. The phone's folder always holds the latest copy of everything,
//! as plain files. Each run also gets a folder in `history/` that holds the copies it replaced
//! and a `run.json` listing what it added, replaced and found gone from the phone. Any earlier
//! state is the latest folder with later runs' replaced copies swapped back in.

use super::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// The folder inside a backup that holds older versions.
pub const HISTORY: &str = "history";
const RUN: &str = "run.json";

/// A file a run replaced: its path inside the backup, and the size and SHA-256 of the copy
/// that was replaced, which now lives in the run's history folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Replaced {
    pub path: String,
    pub size: u64,
    pub sum: Option<String>,
}

/// One backup run, as `history/<name>/run.json` records it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    /// The run's folder name, such as "2026-11-10_143205", its local start time.
    pub name: String,
    /// Start time in Unix seconds, to sort and prune by.
    pub when: i64,
    /// How the phone was read: "USB or Wi-Fi" or "MTP".
    pub how: String,
    /// Files new in the backup, with their size.
    pub added: Vec<(String, u64)>,
    pub replaced: Vec<Replaced>,
    /// Files the phone no longer had. They stay in the backup.
    pub gone: Vec<String>,
    /// False when the run was stopped or failed part way.
    pub finished: bool,
}

impl Run {
    /// "2026-11-10 14:32" from the folder name, or the name itself.
    pub fn title(&self) -> String {
        let n = &self.name;
        match (n.get(..10), n.get(11..13), n.get(13..15)) {
            (Some(day), Some(h), Some(m)) if n.as_bytes().get(10) == Some(&b'_') => {
                format!("{day} {h}:{m}")
            }
            _ => n.clone(),
        }
    }

    /// What the run did, in a few words: "12 new, 3 replaced, 1 gone from the phone".
    pub fn said(&self) -> String {
        let mut parts = Vec::new();
        if !self.added.is_empty() {
            parts.push(format!("{} new", count(self.added.len())));
        }
        if !self.replaced.is_empty() {
            parts.push(format!("{} replaced", count(self.replaced.len())));
        }
        if !self.gone.is_empty() {
            parts.push(format!("{} gone from the phone", count(self.gone.len())));
        }
        let mut said = if parts.is_empty() {
            "Nothing changed".to_string()
        } else {
            parts.join(", ")
        };
        if !self.finished {
            said += " (stopped part way)";
        }
        said
    }

    fn to_json(&self) -> String {
        let replaced: Vec<_> = self
            .replaced
            .iter()
            .map(|r| serde_json::json!({ "path": r.path, "size": r.size, "sha256": r.sum }))
            .collect();
        let added: Vec<_> = self
            .added
            .iter()
            .map(|(p, s)| serde_json::json!({ "path": p, "size": s }))
            .collect();
        let v = serde_json::json!({
            "version": 1,
            "started": self.when,
            "how": self.how,
            "finished": self.finished,
            "added": added,
            "replaced": replaced,
            "gone": self.gone,
        });
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }

    fn from_json(name: &str, text: &str) -> Option<Run> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let path = |e: &serde_json::Value| e["path"].as_str().map(str::to_string);
        let list = |key: &str| v[key].as_array().cloned().unwrap_or_default();
        Some(Run {
            name: name.to_string(),
            when: v["started"].as_i64().unwrap_or(0),
            how: v["how"].as_str().unwrap_or_default().to_string(),
            finished: v["finished"].as_bool().unwrap_or(false),
            added: list("added")
                .iter()
                .filter_map(|e| Some((path(e)?, e["size"].as_u64().unwrap_or(0))))
                .collect(),
            replaced: list("replaced")
                .iter()
                .filter_map(|e| {
                    Some(Replaced {
                        path: path(e)?,
                        size: e["size"].as_u64().unwrap_or(0),
                        sum: e["sha256"].as_str().map(str::to_string),
                    })
                })
                .collect(),
            gone: list("gone")
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect(),
        })
    }
}

/// Every run kept in a backup's history, oldest first. A run folder whose `run.json` is
/// missing (the app was closed mid-run) counts every file in it as replaced.
pub fn runs(root: &Path) -> Vec<Run> {
    let mut runs: Vec<Run> = fs::read_dir(root.join(HISTORY))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let dir = e.path();
            fs::read_to_string(dir.join(RUN))
                .ok()
                .and_then(|t| Run::from_json(&name, &t))
                .unwrap_or_else(|| Run {
                    when: fs::metadata(&dir)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs() as i64),
                    replaced: walk(&dir)
                        .into_iter()
                        .map(|(path, size)| Replaced {
                            path,
                            size,
                            sum: None,
                        })
                        .collect(),
                    name,
                    ..Run::default()
                })
        })
        .collect();
    runs.sort_by(|a, b| (a.when, &a.name).cmp(&(b.when, &b.name)));
    runs
}

/// Every file under `dir` but `run.json`, as (path inside it with `/`, size).
fn walk(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            match e.metadata() {
                Ok(m) if m.is_dir() => stack.push(p),
                Ok(m) if !(d == dir && e.file_name() == RUN) => out.push((rel(dir, &p), m.len())),
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// The backup as it was right after one run: the latest folder, with `swap` files taken from
/// a later run's history instead, and `skip` files left out because they came later.
#[derive(Default, Debug)]
pub struct AsOf {
    pub swap: HashMap<String, PathBuf>,
    pub skip: HashSet<String>,
}

impl AsOf {
    /// Where the copy of `path` (inside the backup, with `/`) is for this version, or None
    /// when the version did not have it yet.
    pub fn file(&self, root: &Path, path: &str) -> Option<PathBuf> {
        if self.skip.contains(path) {
            return None;
        }
        Some(
            self.swap
                .get(path)
                .cloned()
                .unwrap_or_else(|| root.join(path)),
        )
    }
}

/// The backup as it was right after run `name`. An unknown name gives the latest.
pub fn as_of(root: &Path, name: &str) -> AsOf {
    let runs = runs(root);
    let mut v = AsOf::default();
    let Some(at) = runs.iter().position(|r| r.name == name) else {
        return v;
    };
    // The first later run to touch a file decides: it replaced the copy this version had, or
    // added a file this version did not have.
    let mut decided = HashSet::new();
    for later in &runs[at + 1..] {
        let dir = root.join(HISTORY).join(&later.name);
        for r in &later.replaced {
            if decided.insert(r.path.clone()) {
                v.swap.insert(r.path.clone(), dir.join(&r.path));
            }
        }
        for (path, _) in &later.added {
            if decided.insert(path.clone()) {
                v.skip.insert(path.clone());
            }
        }
    }
    v
}

/// `path` (inside the backup, with `/`) as of run `version`, or the latest for None: the copy
/// itself or its sealed copy. None when that version did not have it.
pub fn file_as_of(root: &Path, version: Option<&str>, path: &str) -> Option<PathBuf> {
    let v = match version {
        None => return stored(&root.join(path)),
        Some(name) => as_of(root, name),
    };
    [path.to_string(), format!("{path}.{}", crypt::EXT)]
        .iter()
        .filter_map(|p| v.file(root, p))
        .find(|f| f.exists())
}

/// Records one run while it happens. Dropping it saves `run.json`, so a stopped or failed run
/// still lists the copies it moved aside.
pub(super) struct Recorder {
    root: PathBuf,
    dir: PathBuf,
    /// The backup's checksums from before the run, for the sums of replaced copies.
    sums: BTreeMap<String, String>,
    run: RefCell<Run>,
    /// The backup's key and what it protects, when it has a password.
    seal: Option<(crypt::Key, Scope)>,
}

impl Recorder {
    pub(super) fn start(root: &Path, how: &str) -> Recorder {
        let when = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let base = stamp_now();
        let history = root.join(HISTORY);
        let mut name = base.clone();
        let mut n = 1;
        while history.join(&name).exists() {
            n += 1;
            name = format!("{base}_{n}");
        }
        Recorder {
            root: root.to_path_buf(),
            dir: history.join(&name),
            sums: read_sums(root),
            seal: lock_of(root)
                .ok()
                .flatten()
                .and_then(|l| Some((key(root)?, l.scope))),
            run: RefCell::new(Run {
                name,
                when,
                how: how.to_string(),
                ..Run::default()
            }),
        }
    }

    /// Moves the copy at `path` aside before a new one takes its place. Same disk, so instant.
    /// Fails, leaving the file where it is, when it cannot be moved; then it must not be replaced.
    pub(super) fn keep_old(&self, path: &Path) -> std::io::Result<()> {
        let Ok(meta) = fs::metadata(path) else {
            return Ok(());
        };
        let key = rel(&self.root, path);
        let to = self.dir.join(&key);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        // A file replaced twice in one run keeps the copy from before the run.
        if to.exists() {
            return fs::remove_file(path);
        }
        let sum = self.sums.get(&key).cloned();
        fs::rename(path, &to)?;
        self.run.borrow_mut().replaced.push(Replaced {
            path: key,
            size: meta.len(),
            sum,
        });
        Ok(())
    }

    /// The key to seal the file at `path` with, when the backup's password protects it.
    pub(super) fn seal_for(&self, path: &Path) -> Option<&crypt::Key> {
        let (key, scope) = self.seal.as_ref()?;
        protects(*scope, &rel(&self.root, path)).then_some(key)
    }

    /// Notes a file that is new in the backup.
    pub(super) fn added(&self, path: &Path, size: u64) {
        self.run
            .borrow_mut()
            .added
            .push((rel(&self.root, path), size));
    }

    /// Notes files the phone no longer has (paths inside the backup). Ones an earlier run
    /// already noted are left out, so each run lists only what went since.
    pub(super) fn gone(&self, mut paths: Vec<String>) {
        let before: HashSet<String> = runs(&self.root).into_iter().flat_map(|r| r.gone).collect();
        paths.retain(|p| !before.contains(p));
        self.run.borrow_mut().gone = paths;
    }

    /// The run ended as asked.
    pub(super) fn finish(self) {
        self.run.borrow_mut().finished = true;
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let run = self.run.get_mut();
        let empty = run.added.is_empty() && run.replaced.is_empty() && run.gone.is_empty();
        // A run that did nothing before it stopped is not a version worth listing.
        if (empty && !run.finished) || !self.root.is_dir() {
            return;
        }
        run.added.sort();
        run.replaced.sort_by(|a, b| a.path.cmp(&b.path));
        run.gone.sort();
        let _ = fs::create_dir_all(&self.dir);
        let part = self.dir.join(format!("{RUN}.part"));
        if fs::write(&part, run.to_json()).is_ok() {
            let _ = fs::rename(&part, self.dir.join(RUN));
        }
    }
}

/// Paths in the backup's storage folders (and `apps` with `apps`) that a run did not list:
/// the phone no longer has them. `listed` holds the run's files, with full paths.
pub(super) fn gone_from(
    root: &Path,
    sums: &BTreeMap<String, String>,
    tops: &[String],
    listed: &HashSet<PathBuf>,
) -> Vec<String> {
    let listed: HashSet<String> = listed.iter().map(|p| rel(root, p)).collect();
    sums.keys()
        .filter(|k| {
            tops.iter().any(|t| {
                k.strip_prefix(t.as_str())
                    .is_some_and(|r| r.starts_with('/'))
            })
        })
        .filter(|k| !listed.contains(*k))
        .cloned()
        .collect()
}

/// Deletes the oldest runs' folders so that at most `keep` runs, or only runs newer than
/// `months` months, stay. Nothing in the latest folder is touched. Returns how many went.
pub fn prune(root: &Path, keep: Option<usize>, months: Option<u32>) -> Result<usize, String> {
    let runs = runs(root);
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let too_many = keep.map_or(0, |k| runs.len().saturating_sub(k));
    let mut gone = 0;
    for (i, run) in runs.iter().enumerate() {
        let old = months.is_some_and(|m| now - run.when > i64::from(m) * 30 * 86_400);
        if i < too_many || old {
            let dir = root.join(HISTORY).join(&run.name);
            fs::remove_dir_all(&dir)
                .map_err(|e| format!("Could not delete {}: {e}", dir.display()))?;
            gone += 1;
        }
    }
    let history = root.join(HISTORY);
    if fs::read_dir(&history).is_ok_and(|mut d| d.next().is_none()) {
        let _ = fs::remove_dir(&history);
    }
    Ok(gone)
}

/// Re-hashes every replaced copy kept in history. Returns (files checked, problems in words,
/// one line per version that has any).
pub(super) fn verify_history(root: &Path, report: &dyn Fn(Msg)) -> (usize, Vec<String>) {
    let runs = runs(root);
    let total: usize = runs.iter().map(|r| r.replaced.len()).sum();
    let (mut checked, mut lines) = (0, Vec::new());
    for run in &runs {
        let dir = root.join(HISTORY).join(&run.name);
        let (mut changed, mut missing) = (Vec::new(), Vec::new());
        for r in &run.replaced {
            if stopped() {
                return (checked, lines);
            }
            report(Msg::Progress {
                done: checked,
                total,
                now: format!("{HISTORY}/{}/{}", run.name, r.path),
            });
            checked += 1;
            match (sha256(&dir.join(&r.path)), &r.sum) {
                (Err(_), _) => missing.push(r.path.as_str()),
                (Ok(s), Some(want)) if s != *want => changed.push(r.path.as_str()),
                _ => {}
            }
        }
        for (list, what) in [(&changed, "changed"), (&missing, "missing")] {
            if !list.is_empty() {
                lines.push(format!(
                    "Version {}: {} {what}, for example {}.",
                    run.title(),
                    files(list.len()),
                    list.iter().take(3).copied().collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    (checked, lines)
}

/// The local date and time as a folder name: "2026-11-10_143205".
#[cfg(windows)]
fn stamp_now() -> String {
    // SAFETY: GetLocalTime only fills the struct it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    format!(
        "{}-{:02}-{:02}_{:02}{:02}{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// The UTC date and time as a folder name, where the local time zone is not read.
#[cfg(not(windows))]
fn stamp_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    stamp(secs)
}

/// Unix seconds as "2026-11-10_143205" (UTC).
#[cfg_attr(windows, allow(dead_code))]
pub(super) fn stamp(secs: i64) -> String {
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y}-{m:02}-{d:02}_{:02}{:02}{:02}",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pb-history-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// One run: writes `files` (path, text) into the latest folder, keeping replaced copies.
    fn backup(root: &Path, name: &str, when: i64, files: &[(&str, &str)]) {
        let mut rec = Recorder::start(root, "test");
        let dir = root.join(HISTORY).join(name);
        rec.dir = dir;
        rec.run.get_mut().name = name.into();
        rec.run.get_mut().when = when;
        let mut written = Vec::new();
        for (path, text) in files {
            let p = root.join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            if p.exists() {
                rec.keep_old(&p).unwrap();
            } else {
                rec.added(&p, text.len() as u64);
            }
            fs::write(&p, text).unwrap();
            written.push(p.clone());
        }
        update_sums(root, &written, &written, &|_| {}).unwrap();
        rec.finish();
    }

    #[test]
    fn protected_data_is_sealed_and_kept_like_plain_data() {
        let root = temp("sealed");
        set_password(&root, "pw", Scope::Data).unwrap();
        let k = key(&root).unwrap();
        let sms = root.join("data/sms.txt");
        fs::create_dir_all(sms.parent().unwrap()).unwrap();
        // A plain copy from before the password.
        fs::write(&sms, "old plain").unwrap();
        let open = |p: &Path| {
            String::from_utf8(
                crypt::decrypt_bytes(&k, &fs::read(p).unwrap())
                    .unwrap()
                    .to_vec(),
            )
            .unwrap()
        };

        let rec = Recorder::start(&root, "test");
        let at = keep_data(&rec, &sms, "one").unwrap();
        assert_eq!(at, crypt::sealed_path(&sms));
        assert!(!sms.exists());
        assert_eq!(open(&at), "one");
        let before = fs::read(&at).unwrap();
        keep_data(&rec, &sms, "one").unwrap();
        assert_eq!(
            fs::read(&at).unwrap(),
            before,
            "unchanged text is not rewritten"
        );
        let dir = rec.dir.clone();
        rec.finish();
        assert_eq!(
            fs::read_to_string(dir.join("data/sms.txt")).unwrap(),
            "old plain"
        );

        let rec = Recorder::start(&root, "test");
        let dir2 = rec.dir.clone();
        keep_data(&rec, &sms, "two").unwrap();
        // Files outside the scope stay plain.
        let calls = keep_data(&rec, &root.join("report-like.txt"), "plain").unwrap();
        assert_eq!(fs::read_to_string(calls).unwrap(), "plain");
        rec.finish();
        assert_eq!(open(&crypt::sealed_path(&sms)), "two");
        assert_eq!(open(&dir2.join("data/sms.txt.pbenc")), "one");
        let all = runs(&root);
        assert_eq!(all.last().unwrap().replaced[0].path, "data/sms.txt.pbenc");
        // Each version reads back by the file's own name: the plain copy from before the
        // password, the first sealed text, then the latest.
        let read = |v: Option<&str>| read_text(&file_as_of(&root, v, "data/sms.txt").unwrap());
        assert_eq!(read(Some(&all[0].name)).unwrap(), "one");
        assert_eq!(read(None).unwrap(), "two");
        assert_eq!(file_as_of(&root, None, "data/none.txt"), None);
        forget(&root);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn names_runs_by_time() {
        assert_eq!(stamp(0), "1970-01-01_000000");
        assert_eq!(stamp(1_794_407_525), "2026-11-11_143205");
        let run = Run {
            name: "2026-11-11_153205".into(),
            ..Run::default()
        };
        assert_eq!(run.title(), "2026-11-11 15:32");
    }

    #[test]
    fn keeps_each_replaced_copy() {
        let root = temp("keeps");
        backup(
            &root,
            "r1",
            1,
            &[("files/a.txt", "one"), ("files/b.txt", "bee")],
        );
        backup(&root, "r2", 2, &[("files/a.txt", "two")]);
        backup(
            &root,
            "r3",
            3,
            &[("files/a.txt", "three"), ("files/c.txt", "sea")],
        );
        let read = |p: PathBuf| fs::read_to_string(p).unwrap();
        assert_eq!(read(root.join("files/a.txt")), "three");
        assert_eq!(read(root.join("history/r2/files/a.txt")), "one");
        assert_eq!(read(root.join("history/r3/files/a.txt")), "two");
        // Unchanged files are never copied into history.
        assert!(!root.join("history/r2/files/b.txt").exists());
        assert!(!root.join("history/r3/files/b.txt").exists());

        let runs = runs(&root);
        assert_eq!(
            runs.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["r1", "r2", "r3"]
        );
        assert_eq!(runs[0].added.len(), 2);
        assert_eq!(runs[2].said(), "1 new, 1 replaced");
        // The sum of the replaced copy came from the checksums of the run before.
        assert_eq!(
            runs[1].replaced[0].sum.as_deref(),
            Some(sha256_text("one").as_str())
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn sha256_text(s: &str) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(s.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn reads_any_version() {
        let root = temp("versions");
        backup(
            &root,
            "r1",
            1,
            &[("files/a.txt", "one"), ("files/b.txt", "bee")],
        );
        backup(&root, "r2", 2, &[("files/a.txt", "two")]);
        backup(
            &root,
            "r3",
            3,
            &[("files/a.txt", "three"), ("files/c.txt", "sea")],
        );
        let text = |v: Option<&str>, p: &str| {
            file_as_of(&root, v, p).map(|f| fs::read_to_string(f).unwrap())
        };
        assert_eq!(text(Some("r1"), "files/a.txt").as_deref(), Some("one"));
        assert_eq!(text(Some("r2"), "files/a.txt").as_deref(), Some("two"));
        assert_eq!(text(Some("r3"), "files/a.txt").as_deref(), Some("three"));
        assert_eq!(text(None, "files/a.txt").as_deref(), Some("three"));
        assert_eq!(text(Some("r1"), "files/b.txt").as_deref(), Some("bee"));
        // c.txt came in run 3.
        assert_eq!(text(Some("r2"), "files/c.txt"), None);
        assert_eq!(text(Some("r3"), "files/c.txt").as_deref(), Some("sea"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_closed_run_still_counts() {
        let root = temp("closed");
        fs::create_dir_all(root.join("history/r1/files")).unwrap();
        fs::write(root.join("history/r1/files/a.txt"), "old").unwrap();
        let runs = runs(&root);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].replaced[0].path, "files/a.txt");
        assert!(!runs[0].finished);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stopped_empty_run_leaves_nothing() {
        let root = temp("empty");
        drop(Recorder::start(&root, "test"));
        assert!(runs(&root).is_empty());
        Recorder::start(&root, "test").finish();
        assert_eq!(runs(&root).len(), 1);
        assert_eq!(runs(&root)[0].said(), "Nothing changed");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prunes_oldest_runs_only() {
        let root = temp("prune");
        backup(&root, "r1", 1, &[("files/a.txt", "one")]);
        backup(&root, "r2", 2, &[("files/a.txt", "two")]);
        backup(&root, "r3", 3, &[("files/a.txt", "three")]);
        assert_eq!(prune(&root, Some(2), None), Ok(1));
        let names: Vec<String> = runs(&root).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["r2", "r3"]);
        assert_eq!(
            fs::read_to_string(root.join("files/a.txt")).unwrap(),
            "three"
        );
        // Every run here started in 1970, far over a month ago.
        assert_eq!(prune(&root, None, Some(1)), Ok(2));
        assert!(!root.join(HISTORY).exists());
        assert_eq!(
            fs::read_to_string(root.join("files/a.txt")).unwrap(),
            "three"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn checks_history_copies() {
        let root = temp("verify");
        backup(
            &root,
            "r1",
            1,
            &[("files/a.txt", "one"), ("files/b.txt", "bee")],
        );
        backup(
            &root,
            "r2",
            2,
            &[("files/a.txt", "two"), ("files/b.txt", "bea")],
        );
        assert_eq!(verify_history(&root, &|_| {}), (2, Vec::new()));
        fs::write(root.join("history/r2/files/a.txt"), "ONE").unwrap();
        fs::remove_file(root.join("history/r2/files/b.txt")).unwrap();
        let (n, lines) = verify_history(&root, &|_| {});
        assert_eq!(n, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("1 file changed") && lines[0].contains("files/a.txt"));
        assert!(lines[1].contains("missing") && lines[1].contains("files/b.txt"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lists_files_gone_from_the_phone() {
        let root = PathBuf::from("root");
        let sums: BTreeMap<String, String> = [
            "files/a",
            "files/b",
            "apps/x/base.apk",
            "data/sms.txt",
            "filesy/c",
        ]
        .iter()
        .map(|p| (p.to_string(), String::new()))
        .collect();
        let listed: HashSet<PathBuf> = [root.join("files").join("a")].into();
        assert_eq!(
            gone_from(&root, &sums, &["files".into()], &listed),
            ["files/b"]
        );
    }
}

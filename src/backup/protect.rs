//! Turning a backup's password on, changing what it protects, and turning it off, for the
//! files already in the backup. Each file is written whole under its new name, given its old
//! file time, and only then is the old one deleted, so a stop or a crash loses nothing: running
//! it again carries on where it ended.

use super::*;

/// The other name of a backup file: `X` for `X.pbenc`, `X.pbenc` for `X`.
fn other_name(rel: &str) -> String {
    match rel.strip_suffix(&format!(".{}", crypt::EXT)) {
        Some(plain) => plain.to_string(),
        None => format!("{rel}.{}", crypt::EXT),
    }
}

/// Every file of the backup that a password can protect: not its own lists, exports, side
/// files or temporary folders.
fn backup_files_all(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let top = dir == root;
            if name.starts_with('.') || (top && name == crate::export::EXPORTS) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if !top && !name.ends_with(".part") && !name.ends_with(".part-download") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Seals or opens each file so that exactly the files `scope` covers are sealed (None: none
/// are), then renames them in the checksums and the history. Needs the backup unlocked.
fn convert(root: &Path, scope: Option<Scope>, report: &dyn Fn(Msg)) -> Result<usize, String> {
    let key = key(root).ok_or(LOCKED)?;
    let todo: Vec<PathBuf> = backup_files_all(root)
        .into_iter()
        .filter(|f| {
            let want = scope.is_some_and(|s| protects(s, &rel(root, &plain_path(f))));
            is_sealed(f) != want
        })
        .collect();
    let mut done = 0;
    let mut failed = Vec::new();
    let mut fresh = HashSet::new();
    for f in &todo {
        if stopped() {
            break;
        }
        report(Msg::Progress {
            done,
            total: todo.len(),
            now: rel(root, &plain_path(f)),
        });
        let seal = (!is_sealed(f)).then_some(&key);
        let when = fs::metadata(f).and_then(|m| m.modified());
        let changed = copy_plain(f, &plain_path(f), seal).and_then(|to| {
            if let Ok(t) = when {
                fs::File::options().write(true).open(&to)?.set_modified(t)?;
            }
            fresh.insert(to);
            fs::remove_file(f)
        });
        match changed {
            Ok(()) => done += 1,
            Err(e) => failed.push(format!("{}: {e}", rel(root, f))),
        }
    }
    rename_lists(root, &fresh);
    if stopped() {
        return Err(format!(
            "You stopped it after {} of {}. Nothing is lost; choose Lock on the backup again to finish.",
            count(done),
            files(todo.len())
        ));
    }
    if !failed.is_empty() {
        return Err(format!(
            "{} could not be changed, for example {}. Nothing is lost; choose Lock on the backup again to try them again.",
            files(failed.len()),
            failed
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    Ok(done)
}

/// Points the checksums and every run's lists at each file's current name, plain or sealed.
/// Files in `fresh` were just written, so their sums are worked out again.
fn rename_lists(root: &Path, fresh: &HashSet<PathBuf>) {
    let now_named = |dir: &Path, rel: &str| {
        if dir.join(rel).exists() {
            return rel.to_string();
        }
        let other = other_name(rel);
        if dir.join(&other).exists() {
            other
        } else {
            rel.to_string()
        }
    };
    let old = read_sums(root);
    let mut sums = BTreeMap::new();
    for (path, sum) in &old {
        let now = now_named(root, path);
        if now == *path && !fresh.contains(&root.join(&now)) {
            sums.insert(now, sum.clone());
        } else if let Ok(s) = sha256(&root.join(&now)) {
            sums.insert(now, s);
        }
    }
    if sums != old {
        let _ = write_sums(root, &sums);
    }
    // Files a run added may since have moved into a later run's history, so their name follows
    // the rule rather than the disk.
    let scope = lock_of(root).ok().flatten().map(|l| l.scope);
    let by_rule = |dir: &Path, rel: &str| {
        let now = now_named(dir, rel);
        if dir.join(&now).exists() {
            return now;
        }
        let plain = rel
            .trim_end_matches(&format!(".{}", crypt::EXT))
            .to_string();
        match scope {
            Some(s) if protects(s, &plain) => format!("{plain}.{}", crypt::EXT),
            _ => plain,
        }
    };
    rename_in_runs(root, &by_rule, fresh);
}

/// Points a backup's lists at its files as they are now and works out every sum again: for a
/// backup unpacked from an archive, whose files are all plain.
pub(super) fn relist(root: &Path) {
    let all: HashSet<PathBuf> = backup_files_all(root).into_iter().collect();
    rename_lists(root, &all);
}

/// Seals the backup's files for its password's scope: on a new password, after the scope
/// changed, or to finish a change that was stopped.
pub fn protect(root: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    if locked(root) {
        report(Msg::Locked(root.to_path_buf()));
        return Err(LOCKED.into());
    }
    let lock = lock_of(root)?.ok_or("This backup has no password.")?;
    set_pending(root, Some(Pending::Protect))?;
    let n = convert(root, Some(lock.scope), report)?;
    set_pending(root, None)?;
    let what = match lock.scope {
        Scope::Data => "Contacts, calls, messages and calendar are",
        Scope::All => "Everything copied from the phone is",
    };
    Ok(format!(
        "{what} protected by the password. {} changed.",
        files(n)
    ))
}

/// Opens every sealed file of the backup, then removes its password.
pub fn unprotect(root: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    if locked(root) {
        report(Msg::Locked(root.to_path_buf()));
        return Err(LOCKED.into());
    }
    set_pending(root, Some(Pending::Remove))?;
    let n = convert(root, None, report)?;
    drop_password(root)?;
    Ok(format!(
        "The password is removed. {} {} opened; the backup is ordinary files again.",
        files(n),
        were(n)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pb-protect-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// One backup run writing `files` into the latest folder, keeping replaced copies.
    fn run(root: &Path, files: &[(&str, &str)]) {
        let rec = Recorder::start(root, "test");
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
            written.push(p);
        }
        update_sums(root, &written, &written, &|_| {}).unwrap();
        rec.finish();
        // Run folders are named by the second.
        std::thread::sleep(Duration::from_millis(1100));
    }

    #[test]
    fn turns_protection_on_wider_and_off_without_losing_anything() {
        let root = temp("cycle");
        run(
            &root,
            &[("files/DCIM/a.jpg", "photo"), ("data/sms.txt", "one")],
        );
        run(
            &root,
            &[("data/sms.txt", "two"), ("data/calls.txt", "calls")],
        );
        let first = runs(&root)[0].name.clone();
        let when = fs::metadata(root.join("files/DCIM/a.jpg"))
            .unwrap()
            .modified()
            .unwrap();
        let read = |v: Option<&str>, p: &str| read_text(&file_as_of(&root, v, p).unwrap());

        set_password(&root, "pw", Scope::Data).unwrap();
        let said = protect(&root, &|_| {}).unwrap();
        assert!(
            said.starts_with("Contacts, calls, messages and calendar are protected"),
            "{said}"
        );
        assert!(root.join("data/sms.txt.pbenc").exists() && !root.join("data/sms.txt").exists());
        assert!(root.join("files/DCIM/a.jpg").exists());
        let old = root
            .join(HISTORY)
            .join(runs(&root)[1].name.clone())
            .join("data/sms.txt.pbenc");
        assert!(old.exists(), "history copies are sealed too");
        assert!(
            verify(&root, &|_| {}).is_ok(),
            "{:?}",
            verify(&root, &|_| {})
        );
        assert_eq!(read(Some(&first), "data/sms.txt").unwrap(), "one");
        assert_eq!(read(None, "data/sms.txt").unwrap(), "two");
        // The first version did not have calls.txt yet.
        assert_eq!(file_as_of(&root, Some(&first), "data/calls.txt"), None);
        assert_eq!(lock_of(&root).unwrap().unwrap().pending, None);

        set_scope(&root, Scope::All).unwrap();
        protect(&root, &|_| {}).unwrap();
        let photo = root.join("files/DCIM/a.jpg.pbenc");
        assert!(photo.exists());
        assert_eq!(fs::metadata(&photo).unwrap().modified().unwrap(), when);
        assert!(verify(&root, &|_| {}).is_ok());

        // A crash after the sealed copy was saved but before the plain one was deleted.
        fs::write(root.join("files/DCIM/a.jpg"), "photo").unwrap();
        protect(&root, &|_| {}).unwrap();
        assert!(!root.join("files/DCIM/a.jpg").exists());
        assert!(verify(&root, &|_| {}).is_ok());

        let said = unprotect(&root, &|_| {}).unwrap();
        assert!(said.starts_with("The password is removed."), "{said}");
        assert!(!has_password(&root));
        assert_eq!(
            fs::read_to_string(root.join("files/DCIM/a.jpg")).unwrap(),
            "photo"
        );
        assert_eq!(
            fs::read_to_string(root.join("data/sms.txt")).unwrap(),
            "two"
        );
        assert!(verify(&root, &|_| {}).is_ok());
        assert_eq!(read(Some(&first), "data/sms.txt").unwrap(), "one");
        assert_eq!(file_as_of(&root, Some(&first), "data/calls.txt"), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_locked_backup_asks_first() {
        let root = temp("locked");
        set_password(&root, "pw", Scope::Data).unwrap();
        forget(&root);
        let asked = std::cell::Cell::new(false);
        let r = protect(&root, &|m| asked.set(matches!(m, Msg::Locked(_))));
        assert_eq!(r, Err(LOCKED.into()));
        assert!(asked.get());
        assert!(unprotect(&root, &|_| {}).is_err());
        assert!(has_password(&root));
        let _ = fs::remove_dir_all(&root);
    }
}

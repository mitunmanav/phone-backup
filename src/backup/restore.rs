use super::*;

/// (package, its APK files) for each app saved under the backup folder or its `apps` folder.
pub(super) fn saved_apps(folder: &Path) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
    let apps = if folder.join("apps").is_dir() {
        folder.join("apps")
    } else {
        folder.to_path_buf()
    };
    let mut packages: Vec<(String, Vec<PathBuf>)> = fs::read_dir(&apps)
        .map_err(|e| format!("Cannot open {}: {e}", apps.display()))?
        .flatten()
        .filter_map(|d| {
            let apks: Vec<PathBuf> = fs::read_dir(d.path())
                .ok()?
                .flatten()
                .map(|f| f.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("apk")))
                .collect();
            (!apks.is_empty()).then(|| (d.file_name().to_string_lossy().into_owned(), apks))
        })
        .collect();
    if packages.is_empty() {
        return Err("This folder has no saved apps. Choose the phone's backup folder, the one that holds the apps folder.".into());
    }
    packages.sort();
    Ok(packages)
}

/// The apps of `packages` with only the APK files in `only`; apps left with none are dropped.
pub(super) fn only_in(
    packages: Vec<(String, Vec<PathBuf>)>,
    only: &HashSet<PathBuf>,
) -> Vec<(String, Vec<PathBuf>)> {
    packages
        .into_iter()
        .map(|(pkg, apks)| (pkg, apks.into_iter().filter(|a| only.contains(a)).collect()))
        .filter(|(_, apks): &(String, Vec<PathBuf>)| !apks.is_empty())
        .collect()
}

/// Reinstalls the apps saved in a backup's `apps` folder (one folder of APKs per app) that are
/// not on the phone yet. Only the apps come back, not their data or logins.
/// With `check`, installs nothing and says what a restore would do. `only` limits it to those
/// APK files of the backup. Also returns how many apps failed to install.
pub fn restore_apps(
    serial: &str,
    folder: &Path,
    only: Option<&HashSet<PathBuf>>,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<(String, usize), String> {
    let mut packages = saved_apps(folder)?;
    if let Some(o) = only {
        packages = only_in(packages, o);
    }
    if !connected(serial) {
        return Err(DISCONNECTED_APPS.into());
    }
    let installed = shell(serial, "pm list packages");
    let total = packages.len();
    packages.retain(|(pkg, _)| {
        !installed
            .lines()
            .any(|l| l.trim() == format!("package:{pkg}"))
    });
    if check {
        let names: Vec<&str> = packages.iter().take(3).map(|(p, _)| p.as_str()).collect();
        let said = match packages.len() {
            0 => "Apps: every saved app is already on the phone.".into(),
            n => format!(
                "Apps: will install {} of {}, for example {}. The rest are already on the phone.",
                count(n),
                count(total),
                names.join(", ")
            ),
        };
        return Ok((said, 0));
    }
    let (mut done, mut failed) = (0, Vec::new());
    for (n, (pkg, apks)) in packages.iter().enumerate() {
        if hold() {
            return Err("You stopped the restore. Apps installed so far stay on the phone.".into());
        }
        report(Msg::Status(format!(
            "Installing app {} of {}: {pkg}",
            n + 1,
            packages.len()
        )));
        let mut c = adb();
        c.args(["-s", serial, "install-multiple", "-r"]).args(apks);
        if String::from_utf8_lossy(&run(c)).contains("Success") {
            done += 1;
        } else if !connected(serial) {
            return Err(DISCONNECTED_APPS.into());
        } else {
            failed.push(pkg.clone());
        }
    }
    let already = total - packages.len();
    let mut msg = format!(
        "Installed {done} of {} {}. {} {} already on the phone.",
        count(total),
        if total == 1 { "app" } else { "apps" },
        count(already),
        were(already)
    );
    if !failed.is_empty() {
        msg += &format!(
            " Could not install: {}. The phone may block apps from unknown sources or need a newer Android.",
            failed.join(", ")
        );
    }
    Ok((msg, failed.len()))
}

/// A phone's backup folder, as the Restore tab lists it.
pub struct Saved {
    pub path: PathBuf,
    /// The phone model, from the folder name "<model>_<id>".
    pub name: String,
    pub files: bool,
    pub apps: bool,
    /// Days since the last backup into it (its report is rewritten by every backup).
    pub age_days: Option<u64>,
    /// None until a worker has added it up.
    pub bytes: Option<u64>,
}

/// The folder itself when it is a backup, else the backups inside it, newest first.
pub fn saved_backups(folder: &Path) -> Vec<Saved> {
    if let Some(one) = saved(folder) {
        return vec![one];
    }
    let mut found: Vec<Saved> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| saved(&e.path()))
        .collect();
    found.sort_by_key(|s| s.age_days.unwrap_or(u64::MAX));
    found
}

pub(super) fn saved(path: &Path) -> Option<Saved> {
    let dirs: Vec<String> = fs::read_dir(path)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let files = dirs
        .iter()
        .any(|n| n == "files" || n.starts_with("sdcard_"));
    let apps = dirs.iter().any(|n| n == "apps");
    if !files && !apps {
        return None;
    }
    // Backups made before reports existed have errors.log instead.
    let stamp = fs::metadata(path.join(REPORT))
        .or_else(|_| fs::metadata(path.join("errors.log")))
        .or_else(|_| fs::metadata(path))
        .and_then(|m| m.modified())
        .ok();
    let folder = path.file_name()?.to_string_lossy().into_owned();
    Some(Saved {
        path: path.to_path_buf(),
        name: folder
            .rsplit_once('_')
            .map_or(folder.as_str(), |(model, _)| model)
            .to_string(),
        files,
        apps,
        age_days: stamp
            .and_then(|s| s.elapsed().ok())
            .map(|d| d.as_secs() / 86_400),
        bytes: None,
    })
}

/// Files saved in a backup folder and the phone path each goes back to: `files` goes to
/// /sdcard, `sdcard_<card>` to that SD card when the phone has it. Also returns the cards the
/// phone lacks. App folders (Android/data, Android/obb) are left out: the apps own those.
/// `volumes` lists the phone's cards (`ls /storage`); None keeps every card.
pub(crate) fn saved_files(
    folder: &Path,
    volumes: Option<&str>,
) -> Result<(Vec<Job>, Vec<String>), String> {
    // The `files` folder itself works as well as the phone's folder.
    let folder = if folder.file_name().is_some_and(|n| n == "files") {
        folder.parent().unwrap_or(folder)
    } else {
        folder
    };
    let mut roots = Vec::new();
    let mut missing = Vec::new();
    for entry in fs::read_dir(folder)
        .map_err(|e| format!("Cannot open {}: {e}", folder.display()))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "files" {
            roots.push((entry.path(), "/sdcard".to_string()));
        } else if let Some(card) = name.strip_prefix("sdcard_") {
            if volumes.is_none_or(|v| v.split_whitespace().any(|v| v == card)) {
                roots.push((entry.path(), format!("/storage/{card}")));
            } else {
                missing.push(card.to_string());
            }
        }
    }
    if roots.is_empty() {
        return Err("This folder has no saved files. Choose the phone's backup folder, the one that holds the files folder.".into());
    }
    let renamed = renamed(folder);
    let mut jobs = Vec::new();
    for (local_root, remote_root) in roots {
        let mut stack = vec![local_root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else { continue };
                let rel = path
                    .strip_prefix(&local_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let lower = rel.to_lowercase();
                if lower == "android/data" || lower == "android/obb" {
                    continue;
                }
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    let key = path
                        .strip_prefix(folder)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let remote = renamed
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| format!("{remote_root}/{rel}"));
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs() as i64);
                    jobs.push(Job {
                        remote,
                        size: meta.len(),
                        mtime,
                        local: path,
                    });
                }
            }
        }
    }
    jobs.sort_by(|a, b| a.remote.cmp(&b.remote));
    Ok((jobs, missing))
}

/// Every file a backup could restore, with its size and time (Unix seconds), in phone-path order.
pub fn backup_files(folder: &Path) -> Result<Vec<(PathBuf, u64, i64)>, String> {
    Ok(saved_files(folder, None)?
        .0
        .into_iter()
        .map(|j| (j.local, j.size, j.mtime))
        .collect())
}

/// Reasons a file may not be worth restoring, as bits of `flags`; also the button and row words.
pub const FLAGS: [(u8, &str); 4] = [
    (1, "large"),
    (2, "duplicate"),
    (4, "over 3 years old"),
    (8, "empty"),
];

/// For each file: 1 over 1 GB, 2 the same name and size as an earlier file, 4 not changed in
/// over 3 years before `now`, 8 zero bytes.
pub fn flags(list: &[(PathBuf, u64, i64)], now: i64) -> Vec<u8> {
    let mut seen = HashSet::new();
    list.iter()
        .map(|(path, size, mtime)| {
            let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase());
            let mut f = 0;
            if *size >= 1_000_000_000 {
                f |= 1;
            }
            if !seen.insert((name, *size)) {
                f |= 2;
            }
            if *mtime > 0 && now - mtime > 3 * 365 * 86_400 {
                f |= 4;
            }
            if *size == 0 {
                f |= 8;
            }
            f
        })
        .collect()
}

/// Copies the photos and files of a backup back to the phone, skipping any file the phone
/// already has, so restoring twice copies nothing twice and nothing on the phone is replaced.
/// With `check`, copies nothing and says what a restore would do. `only` limits it to those
/// files of the backup. Also returns how many files could not be copied.
pub fn restore_files(
    serial: &str,
    folder: &Path,
    only: Option<&HashSet<PathBuf>>,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<(String, usize), String> {
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let (mut jobs, missing) = saved_files(folder, Some(&shell(serial, "ls /storage")))?;
    jobs.retain(|j| only.is_none_or(|o| o.contains(&j.local)));
    let total = jobs.len();

    report(Msg::Status("Checking what is already on the phone…".into()));
    // "/sdcard" or "/storage/<card>": the storage each file goes to.
    let root_of = |remote: &str| -> String {
        if remote.starts_with("/sdcard/") {
            "/sdcard".into()
        } else {
            remote.splitn(4, '/').take(3).collect::<Vec<_>>().join("/")
        }
    };
    let roots: std::collections::BTreeSet<String> =
        jobs.iter().map(|j| root_of(&j.remote)).collect();
    let mut on_phone = HashSet::new();
    for root in &roots {
        // Android's shared storage ignores letter case, so compare names in lower case.
        on_phone.extend(
            shell(serial, &format!("find {}/ -type f", sh_quote(root)))
                .lines()
                .map(str::to_lowercase),
        );
    }
    if stopped() {
        return Err("You stopped the restore. Nothing was copied.".into());
    }
    if !connected(serial) {
        return Err(DISCONNECTED_RESTORE.into());
    }
    jobs.retain(|j| !on_phone.contains(&j.remote.to_lowercase()));
    let skipped = total - jobs.len();

    let to_copy = jobs.len();
    let needed: u64 = jobs.iter().map(|j| j.size).sum();
    // Each storage (the phone, an SD card) has its own free space.
    for root in &roots {
        let here: u64 = jobs
            .iter()
            .filter(|j| root_of(&j.remote) == *root)
            .map(|j| j.size)
            .sum();
        if here == 0 {
            continue;
        }
        let free = shell(serial, &format!("stat -f -c '%a %S' {}", sh_quote(root)));
        if let Some(free) = free
            .split_once(' ')
            .and_then(|(a, b)| Some(a.parse::<u64>().ok()? * b.trim().parse::<u64>().ok()?))
        {
            let place = if root == "/sdcard" {
                "the phone"
            } else {
                "the SD card"
            };
            // 200 MB spare so the phone itself is not left with full storage.
            if here + 200_000_000 > free {
                return Err(format!(
                    "Not enough space on {place}: the restore needs {}, but only {} is free. Free up space on {place} first.",
                    size(here),
                    size(free)
                ));
            }
        }
    }
    let no_card = format!(
        " This phone has no SD card named {}, so those files cannot be restored.",
        or_list(&missing)
    );
    if check {
        let mut msg = format!(
            "Files: will copy {} ({}) to the phone. {} already on the phone will be skipped; nothing is replaced.",
            files(to_copy),
            size(needed),
            files(skipped)
        );
        if !missing.is_empty() {
            msg += &no_card;
        }
        return Ok((msg, 0));
    }

    // One `adb push` per folder, like the backup. With several files adb needs the folder to exist.
    let mut by_dir: BTreeMap<&str, Vec<&Job>> = BTreeMap::new();
    for job in &jobs {
        by_dir
            .entry(job.remote.rsplit_once('/').map_or("/sdcard", |d| d.0))
            .or_default()
            .push(job);
    }
    shell_each(
        serial,
        "mkdir -p",
        &by_dir.keys().copied().collect::<Vec<_>>(),
    );

    let tmp = std::env::temp_dir().join(format!("phone-backup-push-{}", std::process::id()));
    let mut tried: Vec<&Job> = Vec::new();
    'dirs: for (dir, group) in &by_dir {
        // Pushing into a folder keeps the local name, so renamed files go one by one to their full path.
        let alone = |j: &Job| {
            too_long(&j.local)
                || j.local.file_name().and_then(|n| n.to_str()) != j.remote.rsplit('/').next()
        };
        let (single, many): (Vec<&Job>, Vec<&Job>) = group.iter().partition(|j| alone(j));
        // 100 paths of at most 240 characters stay under Windows' 32,767-character command line.
        for batch in many.chunks(100).chain(single.chunks(1)) {
            report(Msg::Progress {
                done: tried.len(),
                total: to_copy,
                now: dir.to_string(),
            });
            if hold() {
                break 'dirs;
            }
            let mut c = adb();
            c.args(["-s", serial, "push"]);
            if alone(batch[0]) {
                let mut src = batch[0].local.clone();
                if too_long(&src) {
                    // adb.exe cannot read past 260 characters; Rust's fs can, so copy short and push.
                    let short = tmp.join("file");
                    let _ = fs::create_dir_all(&tmp);
                    // A failed copy would push the previous file's copy; not sent, it counts as failed below.
                    if fs::copy(&src, &short).is_err() {
                        tried.extend(batch);
                        continue;
                    }
                    src = short;
                }
                c.arg(src).arg(&batch[0].remote);
            } else {
                c.args(batch.iter().map(|j| &j.local))
                    .arg(format!("{dir}/"));
            }
            run(c);
            tried.extend(batch);
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    report(Msg::Progress {
        done: tried.len(),
        total: to_copy,
        now: String::new(),
    });

    // Check every file sent. A wrong size means the copy broke off (Stop or a pulled cable);
    // the phone did not have that file before, so removing the broken copy loses nothing.
    let sent: Vec<&str> = tried.iter().map(|j| j.remote.as_str()).collect();
    let sizes: std::collections::HashMap<String, u64> =
        shell_each(serial, "stat -c '%s %n'", &sent)
            .lines()
            .filter_map(|l| {
                l.split_once(' ')
                    .and_then(|(s, p)| Some((p.to_string(), s.parse().ok()?)))
            })
            .collect();
    let broken: Vec<&Job> = tried
        .iter()
        .copied()
        .filter(|j| sizes.get(&j.remote) != Some(&j.size))
        .collect();
    shell_each(
        serial,
        "rm -f",
        &broken
            .iter()
            .filter(|j| sizes.contains_key(&j.remote))
            .map(|j| j.remote.as_str())
            .collect::<Vec<_>>(),
    );
    let copied = tried.len() - broken.len();

    // Android 10+ rescans on request so the Gallery shows the files; older phones do it on restart.
    let sdk: u32 = shell_each(serial, "getprop", &["ro.build.version.sdk"])
        .trim()
        .parse()
        .unwrap_or(0);
    if copied > 0 && sdk >= 29 {
        shell_each(
            serial,
            "content call --uri content://media --method scan_volume --arg",
            &["external_primary"],
        );
    }

    if stopped() {
        return Err(format!(
            "You stopped the restore. {} {} copied back and stay on the phone.",
            files(copied),
            were(copied)
        ));
    }
    if !connected(serial) {
        return Err(DISCONNECTED_RESTORE.into());
    }
    let mut msg = format!(
        "Copied {} back to the phone. {} {} already there.",
        files(copied),
        files(skipped),
        were(skipped)
    );
    if !broken.is_empty() {
        let names: Vec<&str> = broken.iter().take(3).map(|j| j.remote.as_str()).collect();
        msg += &format!(
            " {} could not be copied, for example {}. Restore again to retry.",
            files(broken.len()),
            names.join(", ")
        );
    }
    if copied > 0 && sdk < 29 {
        msg += " Restart the phone so the Gallery shows the restored photos.";
    }
    if !missing.is_empty() {
        msg += &no_card;
    }
    Ok((msg, broken.len()))
}

pub(super) const DISCONNECTED_RESTORE: &str = "The phone was disconnected. Files copied so far stay on the phone; plug it back in and restore again to continue.";
const DISCONNECTED_APPS: &str = "The phone was disconnected. Apps installed so far stay on the phone; plug it back in and restore again to continue.";

/// Restores photos and files first, then apps, from one backup folder. With `check`, changes
/// nothing on the phone and says what a restore would do; otherwise saves the restore report
/// in the backup folder. `phone` names the phone in that report. Also returns how many files
/// and apps failed.
pub fn restore(
    serial: &str,
    phone: &str,
    folder: &Path,
    pick: Pick,
    only: Option<&HashSet<PathBuf>>,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<(String, usize), String> {
    restore_in((serial, phone), folder, pick, only, false, check, report)
}

/// `restore` for phone (serial, name); with `apps_too`, `only` limits the apps as well as the files.
pub(super) fn restore_in(
    (serial, phone): (&str, &str),
    folder: &Path,
    pick: Pick,
    only: Option<&HashSet<PathBuf>>,
    apps_too: bool,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<(String, usize), String> {
    let (mut said, mut failed) = (Vec::new(), 0);
    let mut keep = |(m, n)| {
        said.push(m);
        failed += n;
    };
    let mut result = Ok(());
    if pick.files {
        result = restore_files(serial, folder, only, check, report).map(&mut keep);
    }
    if pick.apps && result.is_ok() {
        let apps_only = only.filter(|_| apps_too);
        result = restore_apps(serial, folder, apps_only, check, report).map(&mut keep);
    }
    let result = match result {
        Ok(()) => Ok((said.join("\n"), failed)),
        Err(e) => Err(said.into_iter().chain([e]).collect::<Vec<_>>().join("\n")),
    };
    if !check {
        let (Ok((text, _)) | Err(text)) = &result;
        let _ = save_report(
            &folder.join(RESTORE_REPORT),
            &format!("Restore to {phone}"),
            text,
        );
    }
    result
}

/// A backup folder and the files and app installers one run put or kept in it.
pub(super) type Run = (PathBuf, HashSet<PathBuf>);

/// Phone to phone, first step: backs up `from` (serial, model), then checks what restoring that
/// backup to `to` would do. Returns the new backup's folder and the files and apps of this run, once
/// made, and what was found. Only that run moves: the folder holds earlier backups of the phone too.
pub fn transfer_check(
    from: (&str, &str),
    to: (&str, &str),
    dest: &Path,
    pick: Pick,
    report: &dyn Fn(Msg),
) -> (Option<Run>, Result<String, String>) {
    let source = Pick {
        data: false,
        ..pick.clone()
    };
    let (s, ran) = match adb_backup_run(from.0, from.1, dest, source, report) {
        Ok(s) => s,
        Err(e) => return (None, Err(format!("Backing up {}: {e}", from.1))),
    };
    let mut said = format!("{} is backed up.", from.1);
    if !s.errors.is_empty() {
        said += &format!(
            " {} could not be copied from it, so they will not move.",
            files(s.errors.len())
        );
    }
    let found = restore_in(to, &s.root, pick, Some(&ran), true, true, report);
    let said = match found.map(|(m, _)| as_transfer(m)).map_err(as_transfer) {
        Ok(m) => Ok(format!("{said}\n{m}")),
        Err(e) => Err(format!("{said}\n{e}")),
    };
    (Some((s.root, ran)), said)
}

/// The restore's words as the Transfer page says them: a transfer ends in a restore.
fn as_transfer(said: String) -> String {
    said.replace("stopped the restore", "stopped the transfer")
        .replace("the restore needs", "the transfer needs")
        .replace("restore again", "transfer again")
        .replace("Restore again", "Transfer again")
}

/// Phone to phone, second step: with `safety`, first backs up all of `to` (serial, model) into
/// its own folder; then restores `ran`, the files and apps of the check's backup in `folder`, to it.
/// The failed count includes what the safety backup could not save.
pub fn transfer_now(
    to: (&str, &str),
    dest: &Path,
    folder: &Path,
    ran: &HashSet<PathBuf>,
    pick: Pick,
    safety: bool,
    report: &dyn Fn(Msg),
) -> Result<(String, usize), String> {
    let (mut said, mut missed) = (String::new(), 0);
    if safety {
        let all = Pick::default();
        let s = adb_backup(to.0, to.1, dest, all, report)
            .map_err(|e| format!("Safety backup of {}: {e} Nothing was restored.", to.1))?;
        missed = s.errors.len() + s.data.iter().filter(|d| !d.1).count();
        said = format!("Safety backup of {} saved in {}", to.1, s.root.display());
        said += &match missed {
            0 => ".\n".into(),
            1 => ", but 1 thing could not be saved; see report.txt there.\n".into(),
            n => format!(
                ", but {} things could not be saved; see report.txt there.\n",
                count(n)
            ),
        };
    }
    match restore_in(to, folder, pick, Some(ran), true, false, report)
        .map(|(m, n)| (as_transfer(m), n))
        .map_err(as_transfer)
    {
        Ok((m, n)) => Ok((said + &m, n + missed)),
        Err(e) => Err(said + &e),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn transfer_words() {
        let said = super::as_transfer(
            "You stopped the restore. Restart the phone so the Gallery shows the restored photos. Restore again to retry.".into(),
        );
        assert_eq!(
            said,
            "You stopped the transfer. Restart the phone so the Gallery shows the restored photos. Transfer again to retry."
        );
    }
}

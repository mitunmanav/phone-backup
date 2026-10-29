//! Copies an Android phone to this computer over ADB (USB or Wi-Fi) or MTP.
//! Everything runs locally; nothing is sent to any server.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, UNIX_EPOCH};

/// Set by the Stop button; the running adb command is killed and the backup ends.
pub static STOP: AtomicBool = AtomicBool::new(false);

fn stopped() -> bool {
    STOP.load(Ordering::Relaxed)
}

const STOPPED: &str = "You stopped the backup. Files copied so far are kept; press Back up again to continue.";

/// What the backup reports back to the window while it runs.
pub enum Msg {
    Status(String),
    Progress { done: usize, total: usize, now: String },
    Phones(Vec<Phone>),
    Wifi(String),
    Done(Result<Vec<Summary>, String>),
    Restored(Result<String, String>),
}

#[derive(Clone, PartialEq)]
pub enum Phone {
    Adb { serial: String, model: String, wifi: bool },
    /// Plugged in with USB debugging on, but "Allow" not tapped yet.
    Unauthorized,
    Mtp { name: String },
}

pub struct Summary {
    pub name: String,
    pub root: PathBuf,
    pub files: usize,
    pub copied: usize,
    pub errors: Vec<String>,
    /// (what, saved?) for contacts, call history and messages. Empty for MTP.
    pub data: Vec<(&'static str, bool)>,
    pub full: bool,
    /// What was asked for.
    pub pick: Pick,
}

/// What the user chose to back up.
#[derive(Clone, Copy)]
pub struct Pick {
    pub files: bool,
    pub apps: bool,
    pub data: bool,
}

/// adb.exe next to this app, then on PATH, then in the default Android SDK folder.
/// Most people never install the SDK, so shipping adb.exe beside the app is the normal case.
fn adb_path() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let on_path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    let sdk = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(r"Android\Sdk\platform-tools"));
    beside
        .iter()
        .flat_map(|d| [d.clone(), d.join("platform-tools")])
        .chain(on_path)
        .chain(sdk)
        .map(|d| d.join(if cfg!(windows) { "adb.exe" } else { "adb" }))
        .find(|p| p.is_file())
}

pub fn adb_installed() -> bool {
    adb_path().is_some()
}

/// adb without a console window flashing up on every call.
fn adb() -> Command {
    let mut c = Command::new(adb_path().unwrap_or_else(|| "adb".into()));
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000); // CREATE_NO_WINDOW
    c
}

/// Phones on ADB; if none, phones in "File transfer" (MTP) mode.
pub fn detect() -> Vec<Phone> {
    let out = adb().args(["devices", "-l"]).output();
    let mut phones = out.map(|o| parse_adb_devices(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    // One phone on both USB and Wi-Fi is listed twice; keep the faster USB entry.
    if phones.iter().filter(|p| matches!(p, Phone::Adb { .. })).count() > 1 {
        phones.sort_by_key(|p| matches!(p, Phone::Adb { wifi: true, .. }));
        let mut seen = std::collections::HashSet::new();
        phones.retain(|p| match p {
            Phone::Adb { serial, .. } => seen.insert(phone_id(serial)),
            _ => true,
        });
    }
    if !phones.is_empty() {
        return phones;
    }
    mtp_rs::mtp::MtpDevice::list_devices()
        .unwrap_or_default()
        .into_iter()
        .map(|d| Phone::Mtp { name: d.product.or(d.manufacturer).unwrap_or_else(|| "Phone".into()) })
        .collect()
}

fn parse_adb_devices(text: &str) -> Vec<Phone> {
    let mut phones = Vec::new();
    for line in text.lines().skip(1) {
        let mut words = line.split_whitespace();
        let (Some(serial), Some(state)) = (words.next(), words.next()) else { continue };
        match state {
            "device" => {
                let model = words.find_map(|w| w.strip_prefix("model:")).unwrap_or(serial).replace('_', " ");
                let wifi = serial.contains(':') || serial.contains("._adb-tls-connect");
                phones.push(Phone::Adb { serial: serial.into(), model, wifi });
            }
            // "authorizing" is the moment the Allow prompt is on screen.
            "unauthorized" | "authorizing" => phones.push(Phone::Unauthorized),
            _ => {}
        }
    }
    phones
}

/// Runs `adb pair` or `adb connect` and returns what adb said.
pub fn wifi(args: &[&str]) -> String {
    match adb().args(args).output() {
        Ok(o) => format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)).trim().to_string(),
        Err(e) => format!("Cannot run adb: {e}. Install Android SDK platform-tools."),
    }
}

// Paths in the listing that are caches or trash, not user data.
fn is_junk(path: &str) -> bool {
    path.contains("/.thumbnails/")
        || path.contains("/.trashed-")
        || path.contains("/.pending-")
        || (path.contains("/Android/data/") && path.contains("/cache/"))
}

/// Runs a shell command on the phone and returns its stdout. Stderr (e.g. "Permission denied"
/// for unreadable folders) is ignored on purpose: we keep whatever could be read.
fn shell(serial: &str, cmd: &str) -> String {
    let mut c = adb();
    c.args(["-s", serial, "shell", cmd]);
    String::from_utf8_lossy(&run(c)).into_owned()
}

/// Runs a command and returns its stdout, or what it printed so far if Stop is pressed.
/// Unlike `output()`, this kills the command on Stop: a long `find` or `adb pull` would
/// otherwise keep running after the window closes.
fn run(mut c: Command) -> Vec<u8> {
    let Ok(mut child) = c.stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return Vec::new() };
    // Read in a thread so a big listing cannot fill the pipe and stall the command.
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut out);
        }
        out
    });
    while let Ok(None) = child.try_wait() {
        if stopped() {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    reader.join().unwrap_or_default()
}

/// True while adb still sees the phone. Listing commands just return nothing when it is gone.
fn connected(serial: &str) -> bool {
    adb().args(["-s", serial, "get-state"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "device")
}

const DISCONNECTED: &str = "The phone was disconnected. Files copied so far are kept; plug it back in and press Back up again to continue.";

/// ro.serialno is the same over USB and Wi-Fi, unlike the adb serial.
fn phone_id(serial: &str) -> String {
    match shell(serial, "getprop ro.serialno").trim() {
        "" => serial.to_string(),
        s => s.to_string(),
    }
}

/// One file to copy: remote path, its size and mtime (Unix seconds), and where it goes locally.
struct Job {
    remote: String,
    size: u64,
    mtime: i64,
    local: PathBuf,
}

/// Parses a `stat -c '%s %Y %n'` line into (size, mtime, rest).
fn parse_stat(line: &str) -> Option<(u64, i64, &str)> {
    let mut it = line.splitn(3, ' ');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?))
}

pub fn adb_backup(serial: &str, model: &str, dest: &Path, pick: Pick, report: &dyn Fn(Msg)) -> Result<Summary, String> {
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let status = |s: String| report(Msg::Status(s));
    let id = phone_id(serial);
    let root = dest.join(safe_name(&format!("{model}_{id}")));

    let mut jobs = Vec::new();

    // Shared storage (photos, videos, downloads, documents, app media) plus any SD cards.
    let mut storages = Vec::new();
    if pick.files {
        storages.push(("/sdcard".to_string(), root.join("files")));
    }
    for vol in shell(serial, "ls /storage").split_whitespace().filter(|_| pick.files) {
        if !["emulated", "self"].contains(&vol) {
            storages.push((format!("/storage/{vol}"), root.join(format!("sdcard_{}", safe_name(vol)))));
        }
    }
    for (remote_root, local_root) in &storages {
        status(format!("Reading the file list on {model}"));
        let listing = shell(serial, &format!("find {remote_root}/ -type f -exec stat -c '%s %Y %n' {{}} +"));
        for (size, mtime, path) in listing.lines().filter_map(parse_stat) {
            // shortcut: file names containing a newline are skipped; rare, would need find -print0.
            let Some(rel) = path.strip_prefix(&format!("{remote_root}/")) else { continue };
            if is_junk(path) {
                continue;
            }
            let local = rel.split('/').fold(local_root.clone(), |p, part| p.join(safe_name(part)));
            jobs.push(Job { remote: path.to_string(), size, mtime, local });
        }
    }

    // Installers of apps the user installed (base + split APKs). App private data is not
    // readable without root on Android 12+, so this restores the apps, not their logins/settings.
    status("Reading the list of installed apps".into());
    let apps = if pick.apps {
        shell(
            serial,
            r#"for p in $(pm list packages -3 | cut -d: -f2); do for a in $(pm path $p | cut -d: -f2); do stat -c "%s %Y $p %n" $a; done; done"#,
        )
    } else {
        String::new()
    };
    for (size, mtime, rest) in apps.lines().filter_map(parse_stat) {
        let Some((pkg, path)) = rest.trim().split_once(' ') else { continue };
        let name = path.rsplit('/').next().unwrap_or(path);
        let local = root.join("apps").join(safe_name(pkg)).join(safe_name(name));
        jobs.push(Job { remote: path.to_string(), size, mtime, local });
    }

    if stopped() {
        return Err(STOPPED.into());
    }
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    unclash(&mut jobs);
    let files = jobs.len();
    jobs.retain(needs_copy);
    let to_copy = jobs.len();
    let needed: u64 = jobs.iter().map(|j| j.size).sum();
    if let Some(free) = free_space(dest) {
        // 200 MB spare so Windows itself is not left with a full disk.
        if needed + 200_000_000 > free {
            return Err(format!(
                "Not enough space on this computer: the backup needs {} more, but only {} is free. Free up space or choose another folder.",
                size(needed),
                size(free)
            ));
        }
    }
    let errors = pull(serial, jobs, &root, report);
    if stopped() {
        return Err(STOPPED.into());
    }
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }

    // Contacts, call log and SMS as text. Many phones refuse these to ADB; the summary says so.
    status("Saving contacts, call history and messages".into());
    let data_dir = root.join("data");
    let _ = fs::create_dir_all(if pick.data { &data_dir } else { &root });
    let mut data = Vec::new();
    for (label, file, query) in [
        ("Contacts", "contacts.txt", "content query --uri content://com.android.contacts/data --projection contact_id:mimetype:display_name:data1"),
        ("Call history", "call_log.txt", "content query --uri content://call_log/calls --projection number:name:date:duration:type"),
        ("Messages", "sms.txt", "content query --uri content://sms --projection address:date:type:body"),
    ]
    .into_iter()
    .filter(|_| pick.data)
    {
        let out = shell(serial, query);
        let ok = out.starts_with("Row:") || out.starts_with("No result");
        if ok {
            if file == "contacts.txt" {
                let _ = fs::write(data_dir.join("contacts.vcf"), vcard(&out));
            }
            let _ = fs::write(data_dir.join(file), out);
        }
        data.push((label, ok));
    }

    let _ = fs::write(root.join("errors.log"), errors.join("\n"));
    Ok(Summary { name: model.to_string(), root, files, copied: to_copy.saturating_sub(errors.len()), errors, data, full: true, pick })
}

/// Bytes the current user may still write on the drive holding `path`.
#[cfg(windows)]
fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated and outlives the call; null pointers skip the other outputs.
    let ok = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
fn free_space(_: &Path) -> Option<u64> {
    None
}

/// 1_500_000_000 -> "1.5 GB", 350_000_000 -> "350 MB"
fn size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{} MB", bytes.div_ceil(1_000_000))
    }
}

/// Windows ignores letter case, and safe_name can turn two names into one, so IMG.jpg and
/// img.jpg would overwrite each other on every run. Later ones get " (2)", " (3)"...
fn unclash(jobs: &mut [Job]) {
    jobs.sort_by(|a, b| a.remote.cmp(&b.remote));
    let mut seen = std::collections::HashSet::new();
    for job in jobs {
        let original = job.local.clone();
        let mut n = 1;
        while !seen.insert(job.local.to_string_lossy().to_lowercase()) {
            n += 1;
            let stem = original.file_stem().unwrap_or_default().to_string_lossy();
            let name = match original.extension() {
                Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
                None => format!("{stem} ({n})"),
            };
            job.local = original.with_file_name(name);
        }
    }
}

/// Turns `content query` rows of contact_id, mimetype, display_name, data1 into a vCard 3.0
/// file that Google Contacts and phones can import. Only names, phone numbers and emails.
fn vcard(rows: &str) -> String {
    let mut contacts: BTreeMap<u64, (String, Vec<String>)> = BTreeMap::new();
    for line in rows.lines() {
        // Row: 3 contact_id=12, mimetype=vnd.android.cursor.item/phone_v2, display_name=Ann, data1=+1 555
        let Some(rest) = line.strip_prefix("Row: ").and_then(|r| r.split_once(" contact_id=")).map(|r| r.1) else { continue };
        let Some((id, rest)) = rest.split_once(", mimetype=") else { continue };
        let Some((mime, rest)) = rest.split_once(", display_name=") else { continue };
        let Some((name, value)) = rest.split_once(", data1=") else { continue };
        let Ok(id) = id.parse() else { continue };
        let field = match mime {
            "vnd.android.cursor.item/phone_v2" => "TEL",
            "vnd.android.cursor.item/email_v2" => "EMAIL",
            _ => continue,
        };
        if value == "NULL" || value.is_empty() {
            continue;
        }
        let entry = contacts.entry(id).or_insert_with(|| (if name == "NULL" { String::new() } else { name.to_string() }, Vec::new()));
        let line = format!("{field}:{}", vcard_escape(value));
        if !entry.1.contains(&line) {
            entry.1.push(line);
        }
    }
    let mut out = String::new();
    for (name, lines) in contacts.values() {
        let name = vcard_escape(name);
        out += &format!("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:{name}\r\nN:;{name};;;\r\n");
        for l in lines {
            out += l;
            out += "\r\n";
        }
        out += "END:VCARD\r\n";
    }
    out
}

fn vcard_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(',', "\\,").replace(';', "\\;")
}

/// (package, its APK files) for each app saved under the backup folder or its `apps` folder.
fn saved_apps(folder: &Path) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
    let apps = if folder.join("apps").is_dir() { folder.join("apps") } else { folder.to_path_buf() };
    let mut packages: Vec<(String, Vec<PathBuf>)> = fs::read_dir(&apps)
        .map_err(|e| format!("Cannot open {}: {e}", apps.display()))?
        .flatten()
        .filter_map(|d| {
            let apks: Vec<PathBuf> = fs::read_dir(d.path()).ok()?.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("apk"))).collect();
            (!apks.is_empty()).then(|| (d.file_name().to_string_lossy().into_owned(), apks))
        })
        .collect();
    if packages.is_empty() {
        return Err("This folder has no saved apps. Choose the phone's backup folder, the one that holds the apps folder.".into());
    }
    packages.sort();
    Ok(packages)
}

/// Reinstalls the apps saved in a backup's `apps` folder (one folder of APKs per app) that are
/// not on the phone yet. Only the apps come back, not their data or logins.
pub fn restore_apps(serial: &str, folder: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    let mut packages = saved_apps(folder)?;
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let installed = shell(serial, "pm list packages");
    let total = packages.len();
    packages.retain(|(pkg, _)| !installed.lines().any(|l| l.trim() == format!("package:{pkg}")));
    let (mut done, mut failed) = (0, Vec::new());
    for (n, (pkg, apks)) in packages.iter().enumerate() {
        if stopped() {
            return Err("You stopped the restore. Apps installed so far stay on the phone.".into());
        }
        report(Msg::Status(format!("Installing app {} of {}: {pkg}", n + 1, packages.len())));
        let mut c = adb();
        c.args(["-s", serial, "install-multiple", "-r"]).args(apks);
        if String::from_utf8_lossy(&run(c)).contains("Success") {
            done += 1;
        } else {
            failed.push(pkg.clone());
        }
    }
    let mut msg = format!("Installed {done} of {total} apps. {} were already on the phone.", total - packages.len());
    if !failed.is_empty() {
        msg += &format!(" Could not install: {}. The phone may block apps from unknown sources or need a newer Android.", failed.join(", "));
    }
    Ok(msg)
}

/// Files saved in a backup folder and the phone path each goes back to: `files` goes to
/// /sdcard, `sdcard_<card>` to that SD card when the phone has it. Also returns the cards the
/// phone lacks. App folders (Android/data, Android/obb) are left out: the apps own those.
fn saved_files(folder: &Path, volumes: &str) -> Result<(Vec<Job>, Vec<String>), String> {
    // The `files` folder itself works as well as the phone's folder.
    let folder = if folder.file_name().is_some_and(|n| n == "files") { folder.parent().unwrap_or(folder) } else { folder };
    let mut roots = Vec::new();
    let mut missing = Vec::new();
    for entry in fs::read_dir(folder).map_err(|e| format!("Cannot open {}: {e}", folder.display()))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "files" {
            roots.push((entry.path(), "/sdcard".to_string()));
        } else if let Some(card) = name.strip_prefix("sdcard_") {
            if volumes.split_whitespace().any(|v| v == card) {
                roots.push((entry.path(), format!("/storage/{card}")));
            } else {
                missing.push(card.to_string());
            }
        }
    }
    if roots.is_empty() {
        return Err("This folder has no saved files. Choose the phone's backup folder, the one that holds the files folder.".into());
    }
    let mut jobs = Vec::new();
    for (local_root, remote_root) in roots {
        let mut stack = vec![local_root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else { continue };
                let rel = path.strip_prefix(&local_root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                let lower = rel.to_lowercase();
                if lower == "android/data" || lower == "android/obb" {
                    continue;
                }
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    // shortcut: names that safe_name or unclash changed go back changed; the original is not stored.
                    jobs.push(Job { remote: format!("{remote_root}/{rel}"), size: meta.len(), mtime: 0, local: path });
                }
            }
        }
    }
    jobs.sort_by(|a, b| a.remote.cmp(&b.remote));
    Ok((jobs, missing))
}

/// Wraps a path in single quotes for the phone's shell.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Runs `<cmd> 'path' 'path' ...` on the phone in batches that fit one command line, to the
/// end even after Stop (cleanup must finish), and returns everything it printed.
fn shell_each(serial: &str, cmd: &str, paths: &[&str]) -> String {
    let mut out = String::new();
    for batch in paths.chunks(100) {
        let line = std::iter::once(cmd.to_string()).chain(batch.iter().map(|p| sh_quote(p))).collect::<Vec<_>>().join(" ");
        if let Ok(o) = adb().args(["-s", serial, "shell", &line]).output() {
            out += &String::from_utf8_lossy(&o.stdout);
        }
    }
    out
}

/// 1 -> "1 file", 12 -> "12 files"
fn file_count(n: usize) -> String {
    format!("{n} {}", if n == 1 { "file" } else { "files" })
}

/// Copies the photos and files of a backup back to the phone, skipping any file the phone
/// already has, so restoring twice copies nothing twice and nothing on the phone is replaced.
pub fn restore_files(serial: &str, folder: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let (mut jobs, missing) = saved_files(folder, &shell(serial, "ls /storage"))?;
    let total = jobs.len();

    report(Msg::Status("Checking what is already on the phone".into()));
    // "/sdcard" or "/storage/<card>": the storage each file goes to.
    let roots: std::collections::BTreeSet<String> = jobs
        .iter()
        .map(|j| if j.remote.starts_with("/sdcard/") { "/sdcard".into() } else { j.remote.splitn(4, '/').take(3).collect::<Vec<_>>().join("/") })
        .collect();
    let mut on_phone = std::collections::HashSet::new();
    for root in &roots {
        // Android's shared storage ignores letter case, so compare names in lower case.
        on_phone.extend(shell(serial, &format!("find {root}/ -type f")).lines().map(str::to_lowercase));
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
    let free = shell(serial, "stat -f -c '%a %S' /sdcard");
    if let Some(free) = free.split_once(' ').and_then(|(a, b)| Some(a.parse::<u64>().ok()? * b.trim().parse::<u64>().ok()?)) {
        // 200 MB spare so the phone itself is not left with full storage.
        if needed + 200_000_000 > free {
            return Err(format!(
                "Not enough space on the phone: the restore needs {}, but only {} is free. Free up space on the phone first.",
                size(needed),
                size(free)
            ));
        }
    }

    // One `adb push` per folder, like the backup. With several files adb needs the folder to exist.
    let mut by_dir: BTreeMap<&str, Vec<&Job>> = BTreeMap::new();
    for job in &jobs {
        by_dir.entry(job.remote.rsplit_once('/').map_or("/sdcard", |d| d.0)).or_default().push(job);
    }
    shell_each(serial, "mkdir -p", &by_dir.keys().copied().collect::<Vec<_>>());

    let tmp = std::env::temp_dir().join(format!("phone-backup-push-{}", std::process::id()));
    let mut tried: Vec<&Job> = Vec::new();
    'dirs: for (dir, group) in &by_dir {
        let (long, short): (Vec<&Job>, Vec<&Job>) = group.iter().partition(|j| too_long(&j.local));
        // 100 paths of at most 240 characters stay under Windows' 32,767-character command line.
        for batch in short.chunks(100).chain(long.chunks(1)) {
            report(Msg::Progress { done: tried.len(), total: to_copy, now: dir.to_string() });
            if stopped() {
                break 'dirs;
            }
            let mut c = adb();
            c.args(["-s", serial, "push"]);
            if too_long(&batch[0].local) {
                // adb.exe cannot read past 260 characters; Rust's fs can, so copy short and push.
                let short = tmp.join(batch[0].local.file_name().unwrap_or_default());
                let _ = fs::create_dir_all(&tmp);
                let _ = fs::copy(&batch[0].local, &short);
                c.arg(short);
            } else {
                c.args(batch.iter().map(|j| &j.local));
            }
            c.arg(format!("{dir}/"));
            run(c);
            tried.extend(batch);
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    report(Msg::Progress { done: tried.len(), total: to_copy, now: String::new() });

    // Check every file sent. A wrong size means the copy broke off (Stop or a pulled cable);
    // the phone did not have that file before, so removing the broken copy loses nothing.
    let sent: Vec<&str> = tried.iter().map(|j| j.remote.as_str()).collect();
    let sizes: std::collections::HashMap<String, u64> = shell_each(serial, "stat -c '%s %n'", &sent)
        .lines()
        .filter_map(|l| l.split_once(' ').and_then(|(s, p)| Some((p.to_string(), s.parse().ok()?))))
        .collect();
    let broken: Vec<&Job> = tried.iter().copied().filter(|j| sizes.get(&j.remote) != Some(&j.size)).collect();
    shell_each(serial, "rm -f", &broken.iter().filter(|j| sizes.contains_key(&j.remote)).map(|j| j.remote.as_str()).collect::<Vec<_>>());
    let copied = tried.len() - broken.len();

    // Android 10+ rescans on request so the Gallery shows the files; older phones do it on restart.
    let sdk: u32 = shell_each(serial, "getprop", &["ro.build.version.sdk"]).trim().parse().unwrap_or(0);
    if copied > 0 && sdk >= 29 {
        shell_each(serial, "content call --uri content://media --method scan_volume --arg", &["external_primary"]);
    }

    if stopped() {
        return Err(format!("You stopped the restore. {} were copied back and stay on the phone.", file_count(copied)));
    }
    if !connected(serial) {
        return Err(DISCONNECTED_RESTORE.into());
    }
    let mut msg = format!("Copied {} back to the phone. {} were already there.", file_count(copied), file_count(skipped));
    if !broken.is_empty() {
        let names: Vec<&str> = broken.iter().take(3).map(|j| j.remote.as_str()).collect();
        msg += &format!(" {} could not be copied, for example {}. Restore again to retry.", file_count(broken.len()), names.join(", "));
    }
    if copied > 0 && sdk < 29 {
        msg += " Restart the phone so the Gallery shows the restored photos.";
    }
    if !missing.is_empty() {
        msg += &format!(" This phone has no SD card named {}, so those files were not restored.", missing.join(", "));
    }
    Ok(msg)
}

const DISCONNECTED_RESTORE: &str = "The phone was disconnected. Files copied so far stay on the phone; plug it back in and restore again to continue.";

/// True when the local copy is missing or differs in size or time.
fn needs_copy(job: &Job) -> bool {
    let Ok(meta) = fs::metadata(&job.local) else { return true };
    let local_mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    // 2 s slack: exFAT/FAT drives store times in 2-second steps.
    meta.len() != job.size || (local_mtime - job.mtime).abs() > 2
}

/// Copies jobs with one `adb pull` per folder (one per file is far slower), then checks each
/// file. Returns the files that did not arrive.
fn pull(serial: &str, jobs: Vec<Job>, root: &Path, report: &dyn Fn(Msg)) -> Vec<String> {
    let total = jobs.len();
    let mut done = 0;
    let mut errors = Vec::new();
    let mut by_dir: BTreeMap<PathBuf, Vec<Job>> = BTreeMap::new();
    let mut single = Vec::new();
    for job in jobs {
        // adb pull into a folder keeps the phone's file name, so renamed files go one by one.
        let same_name = job.local.file_name().and_then(|n| n.to_str()) == job.remote.rsplit('/').next();
        match (same_name && !too_long(&job.local), job.local.parent()) {
            (true, Some(dir)) => by_dir.entry(dir.to_path_buf()).or_default().push(job),
            _ => single.push(job),
        }
    }

    for (dir, jobs) in by_dir {
        let now = dir.strip_prefix(root).unwrap_or(&dir).display().to_string();
        if let Err(e) = fs::create_dir_all(&dir) {
            errors.extend(jobs.iter().map(|j| format!("{}: {e}", j.remote)));
            done += jobs.len();
            continue;
        }
        // Keep each command line well under Windows' 32,767-character limit, and batches small
        // enough that progress moves inside a big Camera folder.
        let mut start = 0;
        while start < jobs.len() {
            report(Msg::Progress { done: done + start, total, now: now.clone() });
            let mut end = start;
            let mut len = 0;
            while end < jobs.len() && end - start < 200 && (end == start || len + jobs[end].remote.len() < 20_000) {
                len += jobs[end].remote.len() + 3;
                end += 1;
            }
            if stopped() {
                return errors;
            }
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a"]).args(jobs[start..end].iter().map(|j| &j.remote)).arg(&dir);
            run(c);
            start = end;
        }
        record_failures(&jobs, &mut errors);
        done += jobs.len();
    }

    let tmp = root.join(".pulling");
    for job in &single {
        if stopped() {
            break;
        }
        report(Msg::Progress { done, total, now: job.remote.clone() });
        if let Some(dir) = job.local.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if too_long(&job.local) {
            // adb.exe cannot write past 260 characters; Rust's fs can, so pull short and move.
            let short = tmp.join("file");
            let _ = fs::create_dir_all(&tmp);
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a", &job.remote]).arg(&short);
            run(c);
            let _ = fs::rename(&short, &job.local);
        } else {
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a", &job.remote]).arg(&job.local);
            run(c);
        }
        done += 1;
    }
    let _ = fs::remove_dir_all(&tmp);
    record_failures(&single, &mut errors);
    report(Msg::Progress { done, total, now: String::new() });
    errors
}

/// Paths near Windows' 260-character limit, which adb.exe cannot write to.
fn too_long(path: &Path) -> bool {
    path.as_os_str().len() > 240
}

fn record_failures(jobs: &[Job], errors: &mut Vec<String>) {
    errors.extend(jobs.iter().filter(|j| needs_copy(j)).map(|j| format!("{} -> {}", j.remote, j.local.display())));
}

/// Makes a phone file name legal on Windows (no <>:"/\|?*, no trailing dot/space, no CON/NUL...).
fn safe_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c < ' ' || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    if s.ends_with(['.', ' ']) {
        s = s.trim_end_matches(['.', ' ']).to_string() + "_";
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit());
    if reserved || s.is_empty() {
        s.insert(0, '_');
    }
    s
}

#[tokio::main(flavor = "current_thread")]
pub async fn mtp_backup(dest: &Path, report: &dyn Fn(Msg)) -> Result<Summary, String> {
    use mtp_rs::mtp::{ByteRange, MtpDevice};
    use std::io::Write;

    let device = MtpDevice::open_first().await.map_err(|e| e.to_string())?;
    let info = device.device_info();
    let name = info.model.clone();
    let root = dest.join(safe_name(&format!("{}_{}", info.model, info.serial_number)));
    let (mut files, mut copied, mut errors) = (0, 0, Vec::new());

    for (n, storage) in device.storages().await.map_err(|e| e.to_string())?.iter().enumerate() {
        let desc = &storage.info().description;
        // First storage is the phone's internal storage: same folder an ADB backup uses.
        let base = if n == 0 { root.join("files") } else { root.join(format!("storage_{}", safe_name(desc))) };
        let mut stack = vec![(None, base)];
        while let Some((parent, dir)) = stack.pop() {
            if stopped() {
                return Err(STOPPED.into());
            }
            let now = dir.strip_prefix(&root).unwrap_or(&dir).display().to_string();
            // Total is unknown until the walk ends, so progress reports 0 for it.
            report(Msg::Progress { done: copied, total: 0, now });
            let objects = match storage.list_objects(parent).await {
                Ok(o) => o,
                Err(e) => {
                    errors.push(format!("{}: {e}", dir.display()));
                    continue;
                }
            };
            for obj in objects {
                let local = dir.join(safe_name(&obj.filename));
                if obj.is_folder() {
                    if obj.filename != ".thumbnails" {
                        stack.push((Some(obj.handle), local));
                    }
                    continue;
                }
                files += 1;
                // shortcut: MTP compares size only (no file times); a same-size edit is missed.
                if fs::metadata(&local).is_ok_and(|m| m.len() == obj.size) {
                    continue;
                }
                let part = local.with_extension("part-download");
                let result: Result<(), String> = async {
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
                    let mut dl = storage.download(obj.handle, ByteRange::Full).await.map_err(|e| e.to_string())?;
                    while let Some(chunk) = dl.next_chunk().await {
                        if stopped() {
                            return Err(STOPPED.to_string());
                        }
                        out.write_all(&chunk.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    }
                    drop(out);
                    // Only a fully downloaded file gets its real name.
                    fs::rename(&part, &local).map_err(|e| e.to_string())
                }
                .await;
                match result {
                    Ok(()) => copied += 1,
                    Err(e) => {
                        let _ = fs::remove_file(&part);
                        errors.push(format!("{}: {e}", local.display()));
                    }
                }
            }
        }
    }
    let _ = fs::create_dir_all(&root);
    let _ = fs::write(root.join("errors.log"), errors.join("\n"));
    Ok(Summary { name, root, files, copied, errors, data: Vec::new(), full: false, pick: Pick { files: true, apps: false, data: false } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stat_lines() {
        assert_eq!(parse_stat("1234 1700000000 /sdcard/DCIM/a b.jpg"), Some((1234, 1700000000, "/sdcard/DCIM/a b.jpg")));
        assert_eq!(parse_stat("find: /sdcard/x: Permission denied"), None);
        assert_eq!(parse_stat(""), None);
    }

    #[test]
    fn parses_adb_devices() {
        let text = "List of devices attached\n\
            emulator-5554  device product:sdk model:Pixel_8 device:emu transport_id:1\n\
            adb-R58M-x._adb-tls-connect._tcp device product:p model:SM_G991B transport_id:2\n\
            R58M  unauthorized usb:1-1 transport_id:3\n\
            ZX1  offline\n\
            ZX2  authorizing transport_id:4\n";
        let phones = parse_adb_devices(text);
        assert!(phones[0] == Phone::Adb { serial: "emulator-5554".into(), model: "Pixel 8".into(), wifi: false });
        assert!(matches!(&phones[1], Phone::Adb { wifi: true, model, .. } if model == "SM G991B"));
        assert!(phones[2] == Phone::Unauthorized);
        assert!(phones[3] == Phone::Unauthorized);
        assert_eq!(phones.len(), 4);
    }

    #[test]
    fn makes_windows_safe_names() {
        assert_eq!(safe_name("photo.jpg"), "photo.jpg");
        assert_eq!(safe_name("a:b?c*.txt"), "a_b_c_.txt");
        assert_eq!(safe_name("ends with dot."), "ends with dot_");
        assert_eq!(safe_name("CON"), "_CON");
        assert_eq!(safe_name("nul.txt"), "_nul.txt");
        assert_eq!(safe_name("COM1"), "_COM1");
        assert_eq!(safe_name("COMMA"), "COMMA");
        assert_eq!(safe_name(""), "_");
    }

    #[test]
    fn skips_junk() {
        assert!(is_junk("/sdcard/DCIM/.thumbnails/1.jpg"));
        assert!(is_junk("/sdcard/Android/data/com.x/cache/a"));
        assert!(!is_junk("/sdcard/Documents/cache/notes.txt"));
        assert!(!is_junk("/sdcard/DCIM/Camera/IMG_1.jpg"));
    }

    #[test]
    fn formats_sizes_and_reads_free_space() {
        assert_eq!(size(1_500_000_000), "1.5 GB");
        assert_eq!(size(350_000_000), "350 MB");
        assert_eq!(size(1), "1 MB");
        #[cfg(windows)]
        assert!(free_space(Path::new(".")).is_some_and(|f| f > 0));
    }

    #[test]
    fn writes_vcards() {
        let rows = "Row: 0 contact_id=7, mimetype=vnd.android.cursor.item/phone_v2, display_name=Ann; Lee, data1=+1 555\n\
            Row: 1 contact_id=7, mimetype=vnd.android.cursor.item/phone_v2, display_name=Ann; Lee, data1=+1 555\n\
            Row: 2 contact_id=7, mimetype=vnd.android.cursor.item/email_v2, display_name=Ann; Lee, data1=ann@x.com\n\
            Row: 3 contact_id=7, mimetype=vnd.android.cursor.item/name, display_name=Ann; Lee, data1=Ann\n\
            Row: 4 contact_id=9, mimetype=vnd.android.cursor.item/phone_v2, display_name=NULL, data1=112\n\
            Row: 5 contact_id=10, mimetype=vnd.android.cursor.item/note, display_name=Bo, data1=line one\n\
            line two";
        assert_eq!(
            vcard(rows),
            "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ann\\; Lee\r\nN:;Ann\\; Lee;;;\r\nTEL:+1 555\r\nEMAIL:ann@x.com\r\nEND:VCARD\r\n\
             BEGIN:VCARD\r\nVERSION:3.0\r\nFN:\r\nN:;;;;\r\nTEL:112\r\nEND:VCARD\r\n"
        );
    }

    #[test]
    fn finds_saved_apps() {
        let root = std::env::temp_dir().join(format!("phone-backup-test-{}", std::process::id()));
        let app = root.join("apps").join("com.example");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("base.apk"), "").unwrap();
        fs::write(app.join("split_config.en.APK"), "").unwrap();
        fs::write(app.join("notes.txt"), "").unwrap();
        fs::create_dir_all(root.join("apps").join("empty")).unwrap();
        let found = saved_apps(&root).unwrap();
        // The apps folder itself works as well as the phone's folder.
        assert_eq!(saved_apps(&root.join("apps")).unwrap().len(), 1);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "com.example");
        assert_eq!(found[0].1.len(), 2);
        assert!(saved_apps(&root).is_err());
    }

    #[test]
    fn maps_saved_files_to_phone_paths() {
        let root = std::env::temp_dir().join(format!("phone-backup-restore-test-{}", std::process::id()));
        for f in ["files/DCIM/Camera/a.jpg", "files/Android/data/com.x/f", "files/Android/media/com.x/m.jpg", "sdcard_1234-AB/Music/s.mp3", "sdcard_9999-ZZ/x", "apps/com.x/base.apk"] {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "abc").unwrap();
        }
        let (jobs, missing) = saved_files(&root, "emulated self 1234-AB").unwrap();
        // Picking the files folder itself works too.
        let (again, _) = saved_files(&root.join("files"), "").unwrap();
        fs::remove_dir_all(&root).unwrap();
        let remotes: Vec<_> = jobs.iter().map(|j| j.remote.as_str()).collect();
        assert_eq!(remotes, ["/sdcard/Android/media/com.x/m.jpg", "/sdcard/DCIM/Camera/a.jpg", "/storage/1234-AB/Music/s.mp3"]);
        assert_eq!(jobs[0].size, 3);
        assert_eq!(missing, ["9999-ZZ"]);
        assert_eq!(again.len(), 2);
        assert!(saved_files(&root, "").is_err());
    }

    #[test]
    fn quotes_shell_paths() {
        assert_eq!(sh_quote("/sdcard/a b.jpg"), "'/sdcard/a b.jpg'");
        assert_eq!(sh_quote("/sdcard/it's $x.jpg"), r"'/sdcard/it'\''s $x.jpg'");
    }

    #[test]
    fn renames_clashing_names() {
        let job = |remote: &str, local: &str| Job { remote: remote.into(), size: 0, mtime: 0, local: PathBuf::from(local) };
        let mut jobs = vec![job("/sdcard/b", "x/IMG.jpg"), job("/sdcard/a", "x/img.jpg"), job("/sdcard/c", "x/img.JPG"), job("/sdcard/d", "x/notes")];
        unclash(&mut jobs);
        let locals: Vec<_> = jobs.iter().map(|j| j.local.to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(locals, ["x/img.jpg", "x/IMG (2).jpg", "x/img (3).JPG", "x/notes"]);
    }

    #[test]
    fn missing_local_file_needs_copy() {
        let job = Job { remote: "/sdcard/x".into(), size: 1, mtime: 0, local: PathBuf::from("does/not/exist") };
        assert!(needs_copy(&job));
    }
}

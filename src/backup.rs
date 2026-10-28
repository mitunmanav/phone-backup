//! Copies an Android phone to this computer over ADB (USB or Wi-Fi) or MTP.
//! Everything runs locally; nothing is sent to any server.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

/// What the backup reports back to the window while it runs.
pub enum Msg {
    Status(String),
    Progress { done: usize, total: usize, now: String },
    Phones(Vec<Phone>),
    Wifi(String),
    Done(Result<Vec<Summary>, String>),
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
    adb()
        .args(["-s", serial, "shell", cmd])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
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

pub fn adb_backup(serial: &str, model: &str, dest: &Path, report: &dyn Fn(Msg)) -> Result<Summary, String> {
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let status = |s: String| report(Msg::Status(s));
    let id = phone_id(serial);
    let root = dest.join(safe_name(&format!("{model}_{id}")));

    let mut jobs = Vec::new();

    // Shared storage (photos, videos, downloads, documents, app media) plus any SD cards.
    let mut storages = vec![("/sdcard".to_string(), root.join("files"))];
    for vol in shell(serial, "ls /storage").split_whitespace() {
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
    let apps = shell(
        serial,
        r#"for p in $(pm list packages -3 | cut -d: -f2); do for a in $(pm path $p | cut -d: -f2); do stat -c "%s %Y $p %n" $a; done; done"#,
    );
    for (size, mtime, rest) in apps.lines().filter_map(parse_stat) {
        let Some((pkg, path)) = rest.trim().split_once(' ') else { continue };
        let name = path.rsplit('/').next().unwrap_or(path);
        let local = root.join("apps").join(safe_name(pkg)).join(safe_name(name));
        jobs.push(Job { remote: path.to_string(), size, mtime, local });
    }

    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    unclash(&mut jobs);
    let files = jobs.len();
    jobs.retain(needs_copy);
    let to_copy = jobs.len();
    let errors = pull(serial, jobs, &root, report);
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }

    // Contacts, call log and SMS as text. Many phones refuse these to ADB; the summary says so.
    status("Saving contacts, call history and messages".into());
    let data_dir = root.join("data");
    let _ = fs::create_dir_all(&data_dir);
    let mut data = Vec::new();
    for (label, file, query) in [
        ("Contacts", "contacts.txt", "content query --uri content://com.android.contacts/data --projection display_name:mimetype:data1"),
        ("Call history", "call_log.txt", "content query --uri content://call_log/calls --projection number:name:date:duration:type"),
        ("Messages", "sms.txt", "content query --uri content://sms --projection address:date:type:body"),
    ] {
        let out = shell(serial, query);
        let ok = out.starts_with("Row:") || out.starts_with("No result");
        if ok {
            let _ = fs::write(data_dir.join(file), out);
        }
        data.push((label, ok));
    }

    let _ = fs::write(root.join("errors.log"), errors.join("\n"));
    Ok(Summary { name: model.to_string(), root, files, copied: to_copy.saturating_sub(errors.len()), errors, data, full: true })
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
            let _ = adb()
                .args(["-s", serial, "pull", "-a"])
                .args(jobs[start..end].iter().map(|j| &j.remote))
                .arg(&dir)
                .output();
            start = end;
        }
        record_failures(&jobs, &mut errors);
        done += jobs.len();
    }

    let tmp = root.join(".pulling");
    for job in &single {
        report(Msg::Progress { done, total, now: job.remote.clone() });
        if let Some(dir) = job.local.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if too_long(&job.local) {
            // adb.exe cannot write past 260 characters; Rust's fs can, so pull short and move.
            let short = tmp.join("file");
            let _ = fs::create_dir_all(&tmp);
            let _ = adb().args(["-s", serial, "pull", "-a", &job.remote]).arg(&short).output();
            let _ = fs::rename(&short, &job.local);
        } else {
            let _ = adb().args(["-s", serial, "pull", "-a", &job.remote]).arg(&job.local).output();
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
    Ok(Summary { name, root, files, copied, errors, data: Vec::new(), full: false })
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

//! Copies an Android phone to this computer over ADB (USB or Wi-Fi) or MTP.
//! Everything runs locally; nothing is sent to any server.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

/// Set by the Stop button; the running adb command is killed and the backup ends.
pub static STOP: AtomicBool = AtomicBool::new(false);

fn stopped() -> bool {
    STOP.load(Ordering::Relaxed)
}

const STOPPED: &str =
    "You stopped the backup. Files copied so far are kept; press Back up again to continue.";

/// What the backup reports back to the window while it runs.
pub enum Msg {
    Status(String),
    Progress {
        done: usize,
        total: usize,
        now: String,
    },
    Phones(Vec<Phone>),
    Wifi(Result<String, String>),
    Done(Result<Vec<Summary>, String>),
    /// What `measure` found on the phone with this serial.
    Sizes(String, Vec<(String, u64)>, u64),
    Restored(Result<String, String>),
    /// The backup folder checked and what the check found.
    Verified(PathBuf, Result<String, String>),
    /// An export ended: its folder, and what it made or why it stopped.
    Exported(PathBuf, Result<String, String>),
    /// A phone-to-phone check: the source phone's serial and backup folder, once backed up.
    Transferred(Option<(String, PathBuf)>, Result<String, String>),
    /// A copy between backups, or a delete, ended.
    Tidied(Result<String, String>),
    /// Sizes of backup folders, in bytes.
    FolderSizes(Vec<(PathBuf, u64)>),
}

#[derive(Clone, PartialEq)]
pub enum Phone {
    Adb {
        serial: String,
        model: String,
        wifi: bool,
        /// The battery level, only when it is under 20% and the phone is not charging.
        battery: Option<u8>,
    },
    /// Plugged in with USB debugging on, but "Allow" not tapped yet.
    Unauthorized,
    Mtp {
        name: String,
    },
}

pub struct Summary {
    pub name: String,
    pub root: PathBuf,
    pub files: usize,
    pub copied: usize,
    pub errors: Vec<String>,
    /// (what, saved?) for contacts, call history, messages and the calendar. Empty for MTP.
    pub data: Vec<(&'static str, bool)>,
    pub full: bool,
    /// What was asked for.
    pub pick: Pick,
}

/// What the user chose to back up.
#[derive(Clone, PartialEq)]
pub struct Pick {
    pub files: bool,
    pub apps: bool,
    pub data: bool,
    /// Top-level folders to copy, matched on the phone and its SD cards; empty copies them all.
    /// "" stands for files that are not in any folder.
    pub folders: BTreeSet<String>,
    /// Which of `KINDS` to copy.
    pub kinds: [bool; 5],
    /// Files bigger than this many bytes are skipped.
    pub max_size: Option<u64>,
}

impl Default for Pick {
    fn default() -> Self {
        Pick {
            files: true,
            apps: true,
            data: true,
            folders: BTreeSet::new(),
            kinds: [true; 5],
            max_size: None,
        }
    }
}

impl Pick {
    /// True when the file at `rel` (its path inside the phone's storage) is to be copied.
    pub fn wants(&self, rel: &str, size: u64) -> bool {
        self.kinds[kind(rel)] && self.max_size.is_none_or(|m| size <= m) && self.in_folders(rel)
    }

    /// The folder, type and size choices in words, or "" when they copy everything.
    pub fn limits(&self) -> String {
        let and = |items: Vec<String>| match items.split_last() {
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
            None => String::new(),
        };
        let mut said = Vec::new();
        if !self.folders.is_empty() {
            let mut names: Vec<_> = self.folders.iter().collect();
            names.sort_by_key(|f| f.is_empty());
            let names = names
                .into_iter()
                .map(|f| {
                    if f.is_empty() {
                        "files not in a folder".into()
                    } else {
                        f.clone()
                    }
                })
                .collect();
            said.push(format!("Only {}.", and(names)));
        }
        if self.kinds != [true; 5] {
            let kinds = (0..5)
                .filter(|&k| self.kinds[k])
                .map(|k| KINDS[k].to_lowercase())
                .collect();
            said.push(format!("Only {}.", and(kinds)));
        }
        if let Some(max) = self.max_size {
            said.push(format!("Skips files over {}.", size(max)));
        }
        said.join(" ")
    }

    fn in_folders(&self, rel: &str) -> bool {
        let top = rel.split_once('/').map_or("", |(top, _)| top);
        self.folders.is_empty() || self.folders.contains(top)
    }
}

pub const KINDS: [&str; 5] = ["Photos", "Videos", "Music", "Documents", "Other files"];

/// Which of `KINDS` a file is, by its extension.
pub fn kind(path: &str) -> usize {
    let ext = path
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    [
        &[
            "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "bmp", "dng", "tif", "tiff", "raw",
        ][..],
        &["mp4", "mkv", "mov", "3gp", "webm", "avi", "m4v", "ts"],
        &[
            "mp3", "m4a", "aac", "flac", "wav", "ogg", "opus", "amr", "wma", "mid",
        ],
        &[
            "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "odt", "ods", "odp", "rtf",
            "csv", "epub", "md",
        ],
    ]
    .iter()
    .position(|list| list.contains(&ext.as_str()))
    .unwrap_or(4)
}

/// Sizes of each choice, for the files on the phone: (path inside its storage, size).
/// A folder's size counts the chosen types; a type's size counts the chosen folders.
pub struct Tally {
    pub folders: BTreeMap<String, u64>,
    pub kinds: [u64; 5],
    pub total: u64,
    pub count: usize,
}

pub fn tally(files: &[(String, u64)], pick: &Pick) -> Tally {
    let mut t = Tally {
        folders: BTreeMap::new(),
        kinds: [0; 5],
        total: 0,
        count: 0,
    };
    for (rel, size) in files {
        let k = kind(rel);
        let fits = pick.max_size.is_none_or(|m| *size <= m);
        let top = rel.split_once('/').map_or("", |(top, _)| top);
        let folder = t.folders.entry(top.to_string()).or_default();
        if fits && pick.kinds[k] {
            *folder += size;
        }
        if fits && pick.in_folders(rel) {
            t.kinds[k] += size;
        }
        if pick.wants(rel, *size) {
            t.total += size;
            t.count += 1;
        }
    }
    t
}

/// adb.exe next to this app, then on PATH, then in the default Android SDK folder.
/// Most people never install the SDK, so shipping adb.exe beside the app is the normal case.
fn adb_path() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let sdk = std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join(r"Android\Sdk\platform-tools"));
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
    let mut phones = out
        .map(|o| parse_adb_devices(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    // One phone on both USB and Wi-Fi is listed twice; keep the faster USB entry.
    if phones
        .iter()
        .filter(|p| matches!(p, Phone::Adb { .. }))
        .count()
        > 1
    {
        phones.sort_by_key(|p| matches!(p, Phone::Adb { wifi: true, .. }));
        let mut seen = std::collections::HashSet::new();
        phones.retain(|p| match p {
            Phone::Adb { serial, .. } => seen.insert(phone_id(serial)),
            _ => true,
        });
    }
    for p in &mut phones {
        if let Phone::Adb {
            serial, battery, ..
        } = p
        {
            // Not stoppable like `shell`, for the same reason as phone_id.
            let out = adb()
                .args(["-s", serial, "shell", "dumpsys battery"])
                .output()
                .map(|o| o.stdout)
                .unwrap_or_default();
            *battery = low_battery(&String::from_utf8_lossy(&out));
        }
    }
    if !phones.is_empty() {
        return phones;
    }
    mtp_rs::mtp::MtpDevice::list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| !NOT_PHONES.contains(&d.vendor_id))
        .map(|d| Phone::Mtp {
            name: d
                .product
                .or(d.manufacturer)
                .unwrap_or_else(|| "Phone".into()),
        })
        .collect()
}

/// USB makers of cameras and watches, which speak the same file protocol as phones.
// shortcut: a list of makers that sell no Android phones; add one when its device shows up as a phone.
const NOT_PHONES: [u16; 8] = [
    0x04a9, // Canon
    0x04b0, // Nikon
    0x04cb, // Fujifilm
    0x054c, // Sony cameras (Xperia phones use 0x0fce)
    0x07b4, // Olympus, OM System
    0x091e, // Garmin
    0x25fb, // Pentax, Ricoh
    0x2672, // GoPro
];

fn parse_adb_devices(text: &str) -> Vec<Phone> {
    let mut phones = Vec::new();
    for line in text.lines().skip(1) {
        let mut words = line.split_whitespace();
        let (Some(serial), Some(state)) = (words.next(), words.next()) else {
            continue;
        };
        match state {
            "device" => {
                let model = words
                    .find_map(|w| w.strip_prefix("model:"))
                    .unwrap_or(serial)
                    .replace('_', " ");
                let wifi = serial.contains(':') || serial.contains("._adb-tls-connect");
                phones.push(Phone::Adb {
                    serial: serial.into(),
                    model,
                    wifi,
                    battery: None,
                });
            }
            // "authorizing" is the moment the Allow prompt is on screen.
            "unauthorized" | "authorizing" => phones.push(Phone::Unauthorized),
            _ => {}
        }
    }
    phones
}

/// The level from `dumpsys battery` when it is under 20% and no charger (AC, USB, wireless,
/// dock) is powering the phone.
fn low_battery(dumpsys: &str) -> Option<u8> {
    let mut level = None;
    for line in dumpsys.lines().map(str::trim) {
        if line.ends_with("powered: true") {
            return None;
        }
        if let Some(l) = line.strip_prefix("level: ") {
            level = l.parse().ok();
        }
    }
    level.filter(|l| *l < 20)
}

const NO_ADB: &str = "This computer does not have adb yet. Download Platform Tools and put adb.exe next to this app.";

/// Runs `adb pair` or `adb connect` and returns what adb said.
pub fn wifi(args: &[&str]) -> Result<String, String> {
    match adb().args(args).output() {
        Ok(o) => wifi_result(
            args[0],
            &format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        ),
        Err(_) => Err(NO_ADB.into()),
    }
}

/// The QR code being waited for; bump it to stop that wait.
pub static QR_WAIT: AtomicUsize = AtomicUsize::new(0);

/// A fresh service name and password, and the QR text Android's "Pair device with QR code"
/// reads: `WIFI:T:ADB;S:<name>;P:<password>;;`.
pub fn qr_code() -> (String, String, String) {
    use std::hash::BuildHasher;
    // Letters and digits only, so nothing needs escaping in the QR text; no 0/O or 1/l.
    let word = |n: usize| {
        let mut r =
            std::collections::hash_map::RandomState::new().hash_one(std::time::SystemTime::now());
        (0..n)
            .map(|_| {
                let c = b"abcdefghijkmnpqrstuvwxyz23456789"[(r % 32) as usize];
                r /= 32;
                c as char
            })
            .collect::<String>()
    };
    let (name, password) = (format!("phone-backup-{}", word(6)), word(10));
    let text = format!("WIFI:T:ADB;S:{name};P:{password};;");
    (name, password, text)
}

/// Waits for the phone that scanned the QR code to show up on the network, pairs with it and
/// connects. None when the wait was stopped. `id` is the QR_WAIT value of this code.
pub fn qr_pair(name: &str, password: &str, id: usize) -> Option<Result<String, String>> {
    if !adb_installed() {
        return Some(Err(NO_ADB.into()));
    }
    // adb started with mDNS off answers "ERROR: mdns daemon unavailable"; it would never see the phone.
    let check = adb().args(["mdns", "check"]).output();
    if check.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("unavailable")) {
        return Some(Err("adb on this computer cannot look for phones on the Wi-Fi. Pair with a code below instead.".into()));
    }
    // Only the wait for the scan can be stopped; once paired, the connect is seen through.
    let find = |service: &str, secs: u64, stoppable: bool, want: &dyn Fn(&str, &str) -> bool| {
        let end = std::time::Instant::now() + Duration::from_secs(secs);
        while std::time::Instant::now() < end
            && (!stoppable || QR_WAIT.load(Ordering::Relaxed) == id)
        {
            let out = adb().args(["mdns", "services"]).output().ok()?;
            if let Some(addr) = find_service(&String::from_utf8_lossy(&out.stdout), service, want) {
                return Some(addr);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        None
    };
    let Some(pair) = find("_adb-tls-pairing", 120, true, &|n, _| n == name) else {
        return (QR_WAIT.load(Ordering::Relaxed) == id).then(|| Err("No phone scanned the code in 2 minutes. Check that both are on the same Wi-Fi, then show a new code or pair with a code below.".into()));
    };
    if wifi(&["pair", &pair, password]).is_err() {
        return Some(Err(
            "The phone scanned the code but did not pair. Show a new code and scan it again."
                .into(),
        ));
    }
    // A paired phone announces its connect address on the same IP; adb may also connect by itself.
    let ip = format!("{}:", pair.rsplit_once(':').map_or("", |(ip, _)| ip));
    let connect = find("_adb-tls-connect", 20, false, &|_, a| a.starts_with(&ip));
    Some(match connect {
        Some(addr) => wifi(&["connect", &addr]),
        None => Ok("Paired. Now connect with step 2.".into()),
    })
}

/// The address of the first `adb mdns services` line for `service` whose name and address pass `want`.
/// Lines hold a name, a service type and an address, like
/// `phone-backup-abc _adb-tls-pairing._tcp 192.168.1.5:37145`, split by tabs.
fn find_service(list: &str, service: &str, want: &dyn Fn(&str, &str) -> bool) -> Option<String> {
    list.lines().find_map(|line| {
        let mut w = line.split_whitespace();
        let (name, kind, addr) = (w.next()?, w.next()?, w.next()?);
        (kind.starts_with(service) && want(name, addr)).then(|| addr.to_string())
    })
}

/// Turns what `adb pair` or `adb connect` printed into plain words. `adb connect` exits 0
/// even when it could not connect, so the words are all there is to go on.
pub fn wifi_result(cmd: &str, out: &str) -> Result<String, String> {
    let out = out.to_lowercase();
    match cmd {
        "pair" if out.contains("successfully paired") => Ok("Paired. Now connect with step 2.".into()),
        "pair" => Err("The phone did not pair. The code and port change each time the pairing screen opens, so type the ones it shows now.".into()),
        // "connected to" and "already connected to"; the failures say "connect to".
        _ if out.contains("connected to") => Ok("Connected. The phone shows up here in a moment.".into()),
        _ if out.contains("authenticate") => Err("The phone did not accept this computer. Pair it first with step 1.".into()),
        _ => Err("The phone did not answer at that address. Check that both are on the same Wi-Fi and the address matches the Wireless debugging screen.".into()),
    }
}

// Paths in the listing that are caches, trash or copies, not user data: app caches, picture
// previews, WhatsApp stickers and the copies WhatsApp keeps of media you sent.
fn is_junk(path: &str) -> bool {
    path.contains("/.thumbnails/")
        || path.contains("/.trashed-")
        || path.contains("/.pending-")
        || path.contains("/.cache/")
        || (path.contains("/Android/data/") && path.contains("/cache/"))
        || (path.contains("/WhatsApp/Media/")
            && (path.contains("/WhatsApp Stickers/") || path.contains("/Sent/")))
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
    let Ok(mut child) = c.stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return Vec::new();
    };
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
    adb()
        .args(["-s", serial, "get-state"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "device")
}

const DISCONNECTED: &str = "The phone was disconnected. Files copied so far are kept; plug it back in and press Back up again to continue.";

/// ro.serialno is the same over USB and Wi-Fi, unlike the adb serial.
/// Not stoppable like `shell`: the phone list calls it too, and Stop stays set after a job ends.
fn phone_id(serial: &str) -> String {
    let out = adb()
        .args(["-s", serial, "shell", "getprop ro.serialno"])
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    match String::from_utf8_lossy(&out).trim() {
        "" => serial.to_string(),
        s => s.to_string(),
    }
}

/// One file to copy: remote path, its size and mtime (Unix seconds), and where it goes locally.
#[derive(Clone)]
struct Job {
    remote: String,
    size: u64,
    mtime: i64,
    local: PathBuf,
}

/// Parses a `stat -c '%s %Y %n'` line into (size, mtime, rest).
fn parse_stat(line: &str) -> Option<(u64, i64, &str)> {
    let mut it = line.splitn(3, ' ');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?,
    ))
}

/// Lists every installed app's installer files as `size mtime package path`.
const APPS: &str = r#"for p in $(pm list packages -3 | cut -d: -f2); do for a in $(pm path $p | cut -d: -f2); do stat -c "%s %Y $p %n" $a; done; done"#;

/// The phone's shared storage and any SD cards: (folder on the phone, folder in the backup).
fn storages(serial: &str) -> Vec<(String, String)> {
    let mut all = vec![("/sdcard".to_string(), "files".to_string())];
    for vol in shell(serial, "ls /storage").split_whitespace() {
        if !["emulated", "self"].contains(&vol) {
            all.push((
                format!("/storage/{vol}"),
                format!("sdcard_{}", safe_name(vol)),
            ));
        }
    }
    all
}

/// Every file under `remote_root` that is not junk: (size, mtime, path on the phone).
fn listing(serial: &str, remote_root: &str) -> Vec<(u64, i64, String)> {
    let out = shell(
        serial,
        &format!("find {remote_root}/ -type f -exec stat -c '%s %Y %n' {{}} +"),
    );
    // shortcut: file names containing a newline are skipped; rare, would need find -print0.
    out.lines()
        .filter_map(parse_stat)
        .filter(|(.., path)| !is_junk(path))
        .map(|(size, mtime, path)| (size, mtime, path.to_string()))
        .collect()
}

/// What the phone holds, to show the size of each choice before a backup: every file as
/// (path inside its storage, size), and the total size of the installed apps.
pub fn measure(serial: &str) -> (Vec<(String, u64)>, u64) {
    let mut files = Vec::new();
    for (remote_root, _) in storages(serial) {
        let prefix = format!("{remote_root}/");
        files.extend(
            listing(serial, &remote_root)
                .into_iter()
                .filter_map(|(size, _, path)| {
                    Some((path.strip_prefix(&prefix)?.to_string(), size))
                }),
        );
    }
    let apps = shell(serial, APPS)
        .lines()
        .filter_map(parse_stat)
        .map(|(size, ..)| size)
        .sum();
    (files, apps)
}

pub fn adb_backup(
    serial: &str,
    model: &str,
    dest: &Path,
    pick: Pick,
    report: &dyn Fn(Msg),
) -> Result<Summary, String> {
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let status = |s: String| report(Msg::Status(s));
    let id = phone_id(serial);
    let root = dest.join(safe_name(&format!("{model}_{id}")));

    let mut jobs = Vec::new();

    // Shared storage (photos, videos, downloads, documents, app media) plus any SD cards.
    let storages: Vec<(String, PathBuf)> = if pick.files {
        storages(serial)
            .into_iter()
            .map(|(remote, name)| (remote, root.join(name)))
            .collect()
    } else {
        Vec::new()
    };
    for (remote_root, local_root) in &storages {
        status(format!("Reading the file list on {model}…"));
        for (size, mtime, path) in listing(serial, remote_root) {
            let Some(rel) = path.strip_prefix(&format!("{remote_root}/")) else {
                continue;
            };
            if !pick.wants(rel, size) {
                continue;
            }
            let local = rel
                .split('/')
                .fold(local_root.clone(), |p, part| p.join(safe_name(part)));
            jobs.push(Job {
                remote: path,
                size,
                mtime,
                local,
            });
        }
    }

    // Installers of apps the user installed (base + split APKs). App private data is not
    // readable without root on Android 12+, so this restores the apps, not their logins/settings.
    let apps = if pick.apps {
        status("Reading the list of installed apps…".into());
        shell(serial, APPS)
    } else {
        String::new()
    };
    for (size, mtime, rest) in apps.lines().filter_map(parse_stat) {
        let Some((pkg, path)) = rest.trim().split_once(' ') else {
            continue;
        };
        let name = path.rsplit('/').next().unwrap_or(path);
        let local = root.join("apps").join(safe_name(pkg)).join(safe_name(name));
        jobs.push(Job {
            remote: path.to_string(),
            size,
            mtime,
            local,
        });
    }

    if stopped() {
        return Err(STOPPED.into());
    }
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    unclash(&mut jobs);
    if pick.files {
        // Names changed for Windows go back to the phone under their real names; see saved_files.
        let mut map = renamed(&root);
        map.extend(jobs.iter().filter_map(|j| {
            let (rel, local_rel) = storages.iter().find_map(|(r, l)| {
                Some((
                    j.remote.strip_prefix(&format!("{r}/"))?,
                    j.local.strip_prefix(l).ok()?,
                ))
            })?;
            let local = j
                .local
                .strip_prefix(&root)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            (local_rel != Path::new(rel)).then(|| (local, j.remote.clone()))
        }));
        let _ = fs::create_dir_all(&root);
        let _ = fs::write(
            root.join(RENAMED),
            map.iter()
                .map(|(local, remote)| format!("{local}\t{remote}\n"))
                .collect::<String>(),
        );
    }
    let files = jobs.len();
    let mut listed: Vec<PathBuf> = jobs.iter().map(|j| j.local.clone()).collect();
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
    let tried = jobs.clone();
    // Old sums of files about to be replaced go first, so a stopped backup leaves no wrong sum.
    let replaced: Vec<PathBuf> = jobs.iter().map(|j| j.local.clone()).collect();
    update_sums(&root, &replaced, &[], report);
    let errors = pull(serial, jobs, &root, report);
    if stopped() {
        return Err(STOPPED.into());
    }
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }

    // Contacts, call log and SMS as text. Many phones refuse these to ADB; the summary says so.
    status("Saving contacts, calls, messages and calendar…".into());
    let data_dir = root.join("data");
    let _ = fs::create_dir_all(if pick.data { &data_dir } else { &root });
    let mut data = Vec::new();
    let mut written = Vec::new();
    for (label, file, query) in [
        ("Contacts", "contacts.txt", "content query --uri content://com.android.contacts/data --projection contact_id:mimetype:display_name:data1"),
        ("Call history", "call_log.txt", "content query --uri content://call_log/calls --projection number:name:date:duration:type"),
        ("Messages", "sms.txt", "content query --uri content://sms --projection address:date:type:body"),
        ("Calendar", "calendar.txt", "content query --uri content://com.android.calendar/events --projection title:dtstart:dtend:allDay:eventLocation:description:rrule"),
        // Senders live under each message (mms/<id>/addr), so one loop on the phone reads them all.
        ("Picture messages", "mms.txt", "content query --uri content://mms --projection _id:date:msg_box:sub; echo; echo Parts:; content query --uri content://mms/part --projection mid:ct:cl:text; echo; echo Addresses:; for id in $(content query --uri content://mms --projection _id | grep _id= | sed 's/.*_id=//'); do content query --uri content://mms/$id/addr --projection msg_id:address:type; done"),
    ]
    .into_iter()
    .filter(|_| pick.data)
    {
        let out = shell(serial, query);
        let ok = out.starts_with("Row:") || out.starts_with("No result");
        if ok {
            if file == "contacts.txt" {
                let _ = fs::write(data_dir.join("contacts.vcf"), vcard(&out));
                written.push(data_dir.join("contacts.vcf"));
            }
            if file == "mms.txt" {
                mms_attachments(serial, &data_dir.join("mms"));
            }
            let _ = fs::write(data_dir.join(file), out);
            written.push(data_dir.join(file));
        }
        data.push((label, ok));
    }
    listed.extend(written.iter().cloned());
    update_sums(&root, &written, &listed, report);

    let copied: Vec<PathBuf> = tried
        .into_iter()
        .filter(|j| !needs_copy(j))
        .map(|j| j.local)
        .collect();
    let s = Summary {
        name: model.to_string(),
        root,
        files,
        copied: to_copy.saturating_sub(errors.len()),
        errors,
        data,
        full: true,
        pick,
    };
    write_report(&s, &copied);
    Ok(s)
}

/// Saves the pictures, videos and other files sent in picture messages as `<part id>_<name>`.
fn mms_attachments(serial: &str, dir: &Path) {
    let parts = shell(
        serial,
        "content query --uri content://mms/part --projection _id:ct:cl",
    );
    for (id, name) in parts.lines().filter_map(mms_part) {
        let _ = fs::create_dir_all(dir);
        let mut c = adb();
        c.args([
            "-s",
            serial,
            "exec-out",
            &format!("content read --uri content://mms/part/{id}"),
        ]);
        let _ = fs::write(dir.join(safe_name(&format!("{id}_{name}"))), run(c));
    }
}

/// Reads `Row: 2 _id=3, ct=image/jpeg, cl=pic.jpg` into (3, "pic.jpg"). Message text and
/// layout parts are skipped: their text is already in mms.txt.
fn mms_part(row: &str) -> Option<(u64, &str)> {
    let (id, rest) = row.split_once("_id=")?.1.split_once(", ct=")?;
    let (ct, cl) = rest.split_once(", cl=")?;
    if ["NULL", "text/plain", "application/smil"].contains(&ct) {
        return None;
    }
    Some((
        id.parse().ok()?,
        if cl == "NULL" { "part" } else { cl.trim() },
    ))
}

/// Rewritten by every backup; also tells the Restore tab when the last backup ran.
pub const REPORT: &str = "report.txt";
/// Written by every restore, next to the backup's own report.
pub const RESTORE_REPORT: &str = "restore-report.txt";

/// Saves a plain-text report the user can open, keep or share.
pub fn save_report(path: &Path, title: &str, body: &str) {
    let _ = fs::write(
        path,
        format!(
            "Phone Backup {}\n{title}, {}\n\n{body}\n",
            env!("CARGO_PKG_VERSION"),
            now_text()
        ),
    );
}

/// The backup's report: what it holds, every file copied this time and every file that failed.
fn write_report(s: &Summary, copied: &[PathBuf]) {
    let held = s.files - s.errors.len().min(s.files);
    let mut body = format!(
        "Copied {} this time. The backup holds {}.\n",
        files(copied.len()),
        files(held)
    );
    for (what, ok) in &s.data {
        let state = if *ok {
            "saved"
        } else {
            "the phone did not allow reading it"
        };
        body += &format!("{what}: {state}\n");
    }
    let limits = s.pick.limits();
    if s.pick.files && !limits.is_empty() {
        body += &format!("Files chosen: {limits}\n");
    }
    let list = |head: &str, items: Vec<String>| match items.len() {
        0 => String::new(),
        n => format!("\n{head} ({}):\n{}\n", count(n), items.join("\n")),
    };
    body += &list("Could not copy", s.errors.clone());
    body += &list(
        "Copied",
        copied
            .iter()
            .map(|p| p.strip_prefix(&s.root).unwrap_or(p).display().to_string())
            .collect(),
    );
    save_report(
        &s.root.join(REPORT),
        &format!("Backup of {}", s.name),
        &body,
    );
}

/// The local date and time as "2026-10-31 14:05".
#[cfg(windows)]
fn now_text() -> String {
    // SAFETY: GetLocalTime only fills the struct it is given.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
        t
    };
    format!(
        "{}-{:02}-{:02} {:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute
    )
}

#[cfg(not(windows))]
fn now_text() -> String {
    String::new()
}

/// The Windows "done" sound.
pub fn beep() {
    // SAFETY: plays a system sound; no pointers involved. 0 is MB_OK.
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(0);
    }
}

/// Facts that help someone fix a problem: app, Windows and adb versions and the phones adb sees.
pub fn diagnostics(dest: &Path) -> String {
    let out = |c: &mut Command| {
        c.output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|e| e.to_string())
    };
    let windows = if cfg!(windows) {
        out(Command::new("cmd").args(["/c", "ver"]))
    } else {
        String::new()
    };
    let adb_at = adb_path().map_or("not found".into(), |p| p.display().to_string());
    format!(
        "Phone Backup {}\n{windows}\nBackup folder: {}\n\nadb: {adb_at}\n{}\n\n{}\n",
        env!("CARGO_PKG_VERSION"),
        dest.display(),
        out(adb().arg("version")),
        out(adb().args(["devices", "-l"])),
    )
}

/// Lists files whose name was changed to be legal on Windows: "files/Docs/_CON.txt<TAB>/sdcard/Docs/CON.txt".
const RENAMED: &str = "renamed.txt";

/// The renamed files of a backup folder: path inside it (with `/`) to the phone path.
fn renamed(root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(root.join(RENAMED))
        .unwrap_or_default()
        .lines()
        // safe_name turns tabs into `_`, so the first tab always ends the local path.
        .filter_map(|l| {
            l.split_once('\t')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        })
        .collect()
}

/// SHA-256 of every file in a backup, in `sha256sum` format so `sha256sum -c` can check it too.
pub const CHECKSUMS: &str = "checksums.sha256";

/// The sums of a backup folder: path inside it (with `/`) to the hex SHA-256.
fn read_sums(root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(root.join(CHECKSUMS))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            l.split_once("  ")
                .map(|(sum, path)| (path.to_string(), sum.to_string()))
        })
        .collect()
}

/// `path` inside `root`, with `/` as `sha256sum` writes it.
fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn sha256(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut fs::File::open(path)?, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Drops the sums of `fresh` files (just rewritten) and of files gone from the folder, then
/// hashes each `known` file that has no sum. Old sums of untouched files are kept, so a later
/// backup hashes only what it copied.
fn update_sums(root: &Path, fresh: &[PathBuf], known: &[PathBuf], report: &dyn Fn(Msg)) {
    let old = read_sums(root);
    let mut sums = old.clone();
    sums.retain(|path, _| root.join(path).is_file());
    for p in fresh {
        sums.remove(&rel(root, p));
    }
    let todo: Vec<&PathBuf> = known
        .iter()
        .filter(|p| !sums.contains_key(&rel(root, p)))
        .collect();
    for (n, p) in todo.iter().enumerate() {
        if stopped() {
            break;
        }
        if n % 100 == 0 {
            report(Msg::Status(format!(
                "Saving checksums: {} of {}…",
                count(n),
                files(todo.len())
            )));
        }
        // A file that failed to copy is not there to hash; it gets its sum next time.
        if let Ok(sum) = sha256(p) {
            sums.insert(rel(root, p), sum);
        }
    }
    if sums != old {
        let _ = fs::write(
            root.join(CHECKSUMS),
            sums.iter()
                .map(|(path, sum)| format!("{sum}  {path}\n"))
                .collect::<String>(),
        );
    }
}

/// Re-hashes every file listed in a backup's checksums and says which changed or went missing.
pub fn verify(root: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    let sums = read_sums(root);
    if sums.is_empty() {
        return Err("This backup has no checksums yet. Back it up again to add them.".into());
    }
    let (mut changed, mut missing) = (Vec::new(), Vec::new());
    for (n, (path, sum)) in sums.iter().enumerate() {
        if stopped() {
            return Err("You stopped the check. The backup was not changed.".into());
        }
        report(Msg::Progress {
            done: n,
            total: sums.len(),
            now: path.clone(),
        });
        match sha256(&root.join(path)) {
            Ok(s) if s == *sum => {}
            Ok(_) => changed.push(path.as_str()),
            Err(_) => missing.push(path.as_str()),
        }
    }
    if changed.is_empty() && missing.is_empty() {
        return Ok(format!(
            "All {} match. Nothing has changed since they were backed up.",
            files(sums.len())
        ));
    }
    let mut msg = String::new();
    for (list, what) in [(&changed, "changed"), (&missing, "missing")] {
        if !list.is_empty() {
            msg += &format!(
                "{} {} {what}, for example {}.\n",
                files(list.len()),
                if list.len() == 1 { "is" } else { "are" },
                list.iter().take(3).copied().collect::<Vec<_>>().join(", ")
            );
        }
    }
    Err(msg + "Back up again to copy them from the phone, if it still has them.")
}

/// Keeps Windows from sleeping while this thread runs a job; `false` lets it sleep again.
pub fn keep_awake(on: bool) {
    // SAFETY: only sets a flag for the calling thread; no pointers involved.
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Power::*;
        SetThreadExecutionState(if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        });
    }
}

/// Bytes the current user may still write on the drive holding `path`.
#[cfg(windows)]
fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated and outlives the call; null pointers skip the other outputs.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
fn free_space(_: &Path) -> Option<u64> {
    None
}

/// 1_500_000_000 -> "1.5 GB", 350_000_000 -> "350 MB", 1_500 -> "2 KB"
pub fn size(bytes: u64) -> String {
    if bytes <= 999_000 {
        format!("{} KB", bytes.div_ceil(1_000))
    } else if bytes >= 1_000_000_000 {
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
    let mut out = String::new();
    for (name, fields) in crate::export::people(rows) {
        let name = vcard_escape(&name);
        out += &format!("BEGIN:VCARD\r\nVERSION:3.0\r\nFN:{name}\r\nN:;{name};;;\r\n");
        for (field, value) in fields {
            out += &format!("{field}:{}\r\n", vcard_escape(&value));
        }
        out += "END:VCARD\r\n";
    }
    out
}

fn vcard_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

/// (package, its APK files) for each app saved under the backup folder or its `apps` folder.
fn saved_apps(folder: &Path) -> Result<Vec<(String, Vec<PathBuf>)>, String> {
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

/// Reinstalls the apps saved in a backup's `apps` folder (one folder of APKs per app) that are
/// not on the phone yet. Only the apps come back, not their data or logins.
/// With `check`, installs nothing and says what a restore would do.
pub fn restore_apps(
    serial: &str,
    folder: &Path,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<String, String> {
    let mut packages = saved_apps(folder)?;
    if !connected(serial) {
        return Err(DISCONNECTED.into());
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
        return Ok(match packages.len() {
            0 => "Apps: every saved app is already on the phone.".into(),
            n => format!(
                "Apps: will install {} of {}, for example {}. The rest are already on the phone.",
                count(n),
                count(total),
                names.join(", ")
            ),
        });
    }
    let (mut done, mut failed) = (0, Vec::new());
    for (n, (pkg, apks)) in packages.iter().enumerate() {
        if stopped() {
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
    Ok(msg)
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

fn saved(path: &Path) -> Option<Saved> {
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
fn saved_files(folder: &Path, volumes: Option<&str>) -> Result<(Vec<Job>, Vec<String>), String> {
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

/// Wraps a path in single quotes for the phone's shell.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Runs `<cmd> 'path' 'path' ...` on the phone in batches that fit one command line, to the
/// end even after Stop (cleanup must finish), and returns everything it printed.
fn shell_each(serial: &str, cmd: &str, paths: &[&str]) -> String {
    let mut out = String::new();
    for batch in paths.chunks(100) {
        let line = std::iter::once(cmd.to_string())
            .chain(batch.iter().map(|p| sh_quote(p)))
            .collect::<Vec<_>>()
            .join(" ");
        if let Ok(o) = adb().args(["-s", serial, "shell", &line]).output() {
            out += &String::from_utf8_lossy(&o.stdout);
        }
    }
    out
}

/// 1280 -> "1,280"
pub fn count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 1 -> "1 file", 1280 -> "1,280 files"
pub fn files(n: usize) -> String {
    format!("{} {}", count(n), if n == 1 { "file" } else { "files" })
}

/// The verb for a count: "1 file was", "2 files were".
pub fn were(n: usize) -> &'static str {
    if n == 1 { "was" } else { "were" }
}

/// ["a"] -> "a", ["a", "b"] -> "a or b", ["a", "b", "c"] -> "a, b or c"
pub fn or_list(items: &[String]) -> String {
    match items.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Copies the photos and files of a backup back to the phone, skipping any file the phone
/// already has, so restoring twice copies nothing twice and nothing on the phone is replaced.
/// With `check`, copies nothing and says what a restore would do. `only` limits it to those
/// files of the backup.
pub fn restore_files(
    serial: &str,
    folder: &Path,
    only: Option<&HashSet<PathBuf>>,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<String, String> {
    if !connected(serial) {
        return Err(DISCONNECTED.into());
    }
    let (mut jobs, missing) = saved_files(folder, Some(&shell(serial, "ls /storage")))?;
    jobs.retain(|j| only.is_none_or(|o| o.contains(&j.local)));
    let total = jobs.len();

    report(Msg::Status("Checking what is already on the phone…".into()));
    // "/sdcard" or "/storage/<card>": the storage each file goes to.
    let roots: std::collections::BTreeSet<String> = jobs
        .iter()
        .map(|j| {
            if j.remote.starts_with("/sdcard/") {
                "/sdcard".into()
            } else {
                j.remote
                    .splitn(4, '/')
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("/")
            }
        })
        .collect();
    let mut on_phone = HashSet::new();
    for root in &roots {
        // Android's shared storage ignores letter case, so compare names in lower case.
        on_phone.extend(
            shell(serial, &format!("find {root}/ -type f"))
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
    let free = shell(serial, "stat -f -c '%a %S' /sdcard");
    if let Some(free) = free
        .split_once(' ')
        .and_then(|(a, b)| Some(a.parse::<u64>().ok()? * b.trim().parse::<u64>().ok()?))
    {
        // 200 MB spare so the phone itself is not left with full storage.
        if needed + 200_000_000 > free {
            return Err(format!(
                "Not enough space on the phone: the restore needs {}, but only {} is free. Free up space on the phone first.",
                size(needed),
                size(free)
            ));
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
        return Ok(msg);
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
            if stopped() {
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
                    let _ = fs::copy(&src, &short);
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
    Ok(msg)
}

const DISCONNECTED_RESTORE: &str = "The phone was disconnected. Files copied so far stay on the phone; plug it back in and restore again to continue.";

/// Restores photos and files first, then apps, from one backup folder. With `check`, changes
/// nothing on the phone and says what a restore would do; otherwise saves the restore report
/// in the backup folder. `phone` names the phone in that report.
pub fn restore(
    serial: &str,
    phone: &str,
    folder: &Path,
    pick: Pick,
    only: Option<&HashSet<PathBuf>>,
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<String, String> {
    let mut said = Vec::new();
    let mut result = Ok(());
    if pick.files {
        result = restore_files(serial, folder, only, check, report).map(|m| said.push(m));
    }
    if pick.apps && result.is_ok() {
        result = restore_apps(serial, folder, check, report).map(|m| said.push(m));
    }
    let result = match result {
        Ok(()) => Ok(said.join("\n")),
        Err(e) => Err(said.into_iter().chain([e]).collect::<Vec<_>>().join("\n")),
    };
    if !check {
        let (Ok(text) | Err(text)) = &result;
        save_report(
            &folder.join(RESTORE_REPORT),
            &format!("Restore to {phone}"),
            text,
        );
    }
    result
}

/// Phone to phone, first step: backs up `from` (serial, model), then checks what restoring that
/// backup to `to` would do. Returns the new backup's folder, once made, and what was found.
pub fn transfer_check(
    from: (&str, &str),
    to: (&str, &str),
    dest: &Path,
    pick: Pick,
    report: &dyn Fn(Msg),
) -> (Option<PathBuf>, Result<String, String>) {
    let source = Pick {
        data: false,
        ..pick.clone()
    };
    let s = match adb_backup(from.0, from.1, dest, source, report) {
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
    let found = restore(to.0, to.1, &s.root, pick, None, true, report);
    let said = match found {
        Ok(m) => Ok(format!("{said}\n{m}")),
        Err(e) => Err(format!("{said}\n{e}")),
    };
    (Some(s.root), said)
}

/// Phone to phone, second step: with `safety`, first backs up all of `to` (serial, model) into
/// its own folder; then restores the backup in `folder` to it.
pub fn transfer_now(
    to: (&str, &str),
    dest: &Path,
    folder: &Path,
    pick: Pick,
    safety: bool,
    report: &dyn Fn(Msg),
) -> Result<String, String> {
    let mut said = String::new();
    if safety {
        let all = Pick::default();
        let s = adb_backup(to.0, to.1, dest, all, report)
            .map_err(|e| format!("Safety backup of {}: {e} Nothing was restored.", to.1))?;
        said = format!("Safety backup of {} saved in {}.\n", to.1, s.root.display());
    }
    match restore(to.0, to.1, folder, pick, None, false, report) {
        Ok(m) => Ok(said + &m),
        Err(e) => Err(said + &e),
    }
}

/// Total bytes of the files in a folder and the folders inside it.
pub fn folder_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        // DirEntry metadata does not follow links, so a link cannot loop the walk.
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => folder_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Gives a backup folder a new name in the same place. Returns its new path.
pub fn rename_backup(path: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Type a name.".into());
    }
    if safe_name(name) != name {
        return Err(r#"Windows does not allow that name. Leave out \ / : * ? " < > | and do not end it with a dot."#.into());
    }
    let new = path.with_file_name(name);
    if new.exists() {
        return Err("A folder with that name is already there.".into());
    }
    fs::rename(path, &new).map_err(|e| {
        format!("Windows could not rename it: {e}. Close any window showing files from it and try again.")
    })?;
    Ok(new)
}

/// Moves a backup folder to the Recycle Bin. When the folder is too big for the bin, or its
/// drive has none, Windows asks before deleting it for good.
pub fn delete_backup(path: &Path) -> Result<String, String> {
    // Never anything but a backup, whatever the caller passes.
    if saved(path).is_none() {
        return Err("This folder is not a phone backup, so it was left alone.".into());
    }
    recycle(path)?;
    if path.exists() {
        return Err(format!(
            "Part of {} could not be deleted. Close any window showing files from it and try again.",
            path.display()
        ));
    }
    Ok(format!("Moved {} to the Recycle Bin.", path.display()))
}

#[cfg(windows)]
fn recycle(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
    };
    // A list of paths that ends with two NULs.
    let from: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: (FOF_ALLOWUNDO
            | FOF_NOCONFIRMATION
            | FOF_WANTNUKEWARNING
            | FOF_SILENT
            | FOF_NOERRORUI) as u16,
        ..Default::default()
    };
    // SAFETY: `from` is double-NUL-terminated and outlives the call; the other pointers are null.
    let code = unsafe { SHFileOperationW(&mut op) };
    if op.fAnyOperationsAborted != 0 {
        Err("You chose to keep it, so the backup was not deleted.".into())
    } else if code != 0 {
        Err(format!(
            "Windows could not delete {} (code {code}). Close any window showing files from it and try again.",
            path.display()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn recycle(_: &Path) -> Result<(), String> {
    Err("Deleting backups works on Windows only.".into())
}

/// True when both files hold the same bytes.
fn same_file(a: &Path, b: &Path) -> bool {
    use std::io::BufReader;
    let (Ok(fa), Ok(fb)) = (fs::File::open(a), fs::File::open(b)) else {
        return false;
    };
    if fa.metadata().map(|m| m.len()).ok() != fb.metadata().map(|m| m.len()).ok() {
        return false;
    }
    let (mut ra, mut rb) = (BufReader::new(fa), BufReader::new(fb));
    let (mut ba, mut bb) = (vec![0u8; 65536], vec![0u8; 65536]);
    loop {
        let Ok(n) = ra.read(&mut ba) else {
            return false;
        };
        // The sizes match, so the end of one file is the end of both.
        if n == 0 {
            return true;
        }
        if rb.read_exact(&mut bb[..n]).is_err() || ba[..n] != bb[..n] {
            return false;
        }
    }
}

/// Copies every file of backup `from` into backup `into`. A file already there with the same
/// bytes is skipped; one with different bytes is never replaced: the copy gets " (2)", " (3)"...
/// The reports are left out (each backup keeps its own). Returns what it did, in plain words.
pub fn merge_backups(from: &Path, into: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    if saved(from).is_none() || saved(into).is_none() {
        return Err("Both folders must be phone backups.".into());
    }
    if from == into {
        return Err("Choose two different backups.".into());
    }
    let mut todo = Vec::new();
    let mut stack = vec![from.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|e| format!("Cannot open {}: {e}", dir.display()))?
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if !(dir == from
                && [REPORT, RESTORE_REPORT, RENAMED]
                    .contains(&&*entry.file_name().to_string_lossy()))
            {
                todo.push(path);
            }
        }
    }
    let (mut copied, mut same, mut renamed_n) = (0, 0, 0);
    let mut kept_names = Vec::new();
    for (n, src) in todo.iter().enumerate() {
        if stopped() {
            return Err(format!(
                "You stopped copying. {} copied so far stay in the backup.",
                files(copied + renamed_n)
            ));
        }
        let rel = src.strip_prefix(from).unwrap_or(src);
        report(Msg::Progress {
            done: n,
            total: todo.len(),
            now: rel.display().to_string(),
        });
        let first = into.join(rel);
        let mut target = first.clone();
        let mut k = 1;
        let free = loop {
            if !target.exists() {
                break true;
            }
            if same_file(src, &target) {
                break false;
            }
            k += 1;
            let stem = first.file_stem().unwrap_or_default().to_string_lossy();
            target = first.with_file_name(match first.extension() {
                Some(ext) => format!("{stem} ({k}).{}", ext.to_string_lossy()),
                None => format!("{stem} ({k})"),
            });
        };
        if !free {
            same += 1;
            continue;
        }
        if let Some(dir) = target.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| format!("Cannot write in {}: {e}", dir.display()))?;
        }
        fs::copy(src, &target).map_err(|e| format!("Could not copy {}: {e}", src.display()))?;
        if k == 1 {
            copied += 1;
            kept_names.push(rel.to_string_lossy().replace('\\', "/"));
        } else {
            renamed_n += 1;
        }
    }
    // Files that kept their name keep their real phone name too; " (2)" copies restore as named.
    let mut map = renamed(into);
    for (local, remote) in renamed(from) {
        if kept_names.contains(&local) {
            map.entry(local).or_insert(remote);
        }
    }
    let _ = fs::write(
        into.join(RENAMED),
        map.iter()
            .map(|(l, r)| format!("{l}\t{r}\n"))
            .collect::<String>(),
    );
    let mut msg = format!(
        "Copied {}. {} {} already there.",
        files(copied + renamed_n),
        files(same),
        were(same)
    );
    if renamed_n > 0 {
        msg += &format!(
            " {} had the same name as a different file, so {} saved with \" (2)\" added.",
            files(renamed_n),
            if renamed_n == 1 {
                "it was"
            } else {
                "they were"
            }
        );
    }
    Ok(msg)
}

/// True when the local copy is missing or differs in size or time.
fn needs_copy(job: &Job) -> bool {
    let Ok(meta) = fs::metadata(&job.local) else {
        return true;
    };
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
        let same_name =
            job.local.file_name().and_then(|n| n.to_str()) == job.remote.rsplit('/').next();
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
            report(Msg::Progress {
                done: done + start,
                total,
                now: now.clone(),
            });
            let mut end = start;
            let mut len = 0;
            while end < jobs.len()
                && end - start < 200
                && (end == start || len + jobs[end].remote.len() < 20_000)
            {
                len += jobs[end].remote.len() + 3;
                end += 1;
            }
            if stopped() {
                return errors;
            }
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a"])
                .args(jobs[start..end].iter().map(|j| &j.remote))
                .arg(&dir);
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
        report(Msg::Progress {
            done,
            total,
            now: job.remote.clone(),
        });
        if let Some(dir) = job.local.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if too_long(&job.local) {
            // adb.exe cannot write past 260 characters; Rust's fs can, so pull short and move.
            let short = tmp.join("file");
            let _ = fs::create_dir_all(&tmp);
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a", &job.remote])
                .arg(&short);
            run(c);
            let _ = fs::rename(&short, &job.local);
        } else {
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a", &job.remote])
                .arg(&job.local);
            run(c);
        }
        done += 1;
    }
    let _ = fs::remove_dir_all(&tmp);
    record_failures(&single, &mut errors);
    report(Msg::Progress {
        done,
        total,
        now: String::new(),
    });
    errors
}

/// Paths near Windows' 260-character limit, which adb.exe cannot write to.
fn too_long(path: &Path) -> bool {
    path.as_os_str().len() > 240
}

fn record_failures(jobs: &[Job], errors: &mut Vec<String>) {
    errors.extend(
        jobs.iter()
            .filter(|j| needs_copy(j))
            .map(|j| format!("{} -> {}", j.remote, j.local.display())),
    );
}

/// Makes a phone file name legal on Windows (no <>:"/\|?*, no trailing dot/space, no CON/NUL...).
fn safe_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c < ' ' || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s.ends_with(['.', ' ']) {
        s = s.trim_end_matches(['.', ' ']).to_string() + "_";
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved || s.is_empty() {
        s.insert(0, '_');
    }
    s
}

#[tokio::main(flavor = "current_thread")]
pub async fn mtp_backup(dest: &Path, pick: &Pick, report: &dyn Fn(Msg)) -> Result<Summary, String> {
    use mtp_rs::mtp::{ByteRange, MtpDevice};
    use std::io::Write;

    let device = MtpDevice::open_first().await.map_err(|e| e.to_string())?;
    let info = device.device_info();
    let name = info.model.clone();
    let root = dest.join(safe_name(&format!("{}_{}", info.model, info.serial_number)));
    let (mut files, mut copied, mut errors) = (0, Vec::new(), Vec::new());
    let mut kept = Vec::new();

    for (n, storage) in device
        .storages()
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .enumerate()
    {
        let desc = &storage.info().description;
        // First storage is the phone's internal storage: same folder an ADB backup uses.
        let base = if n == 0 {
            root.join("files")
        } else {
            root.join(format!("storage_{}", safe_name(desc)))
        };
        // The folder's path inside the storage goes along, for the folder and type choices.
        let mut stack = vec![(None, base, String::new())];
        while let Some((parent, dir, rel)) = stack.pop() {
            if stopped() {
                // Copied files may replace older ones whose sums no longer match.
                update_sums(&root, &copied, &[], report);
                return Err(STOPPED.into());
            }
            let now = dir
                .strip_prefix(&root)
                .unwrap_or(&dir)
                .display()
                .to_string();
            // Total is unknown until the walk ends, so progress reports 0 for it.
            report(Msg::Progress {
                done: copied.len(),
                total: 0,
                now,
            });
            let objects = match storage.list_objects(parent).await {
                Ok(o) => o,
                Err(e) => {
                    errors.push(format!("{}: {e}", dir.display()));
                    continue;
                }
            };
            for obj in objects {
                let local = dir.join(safe_name(&obj.filename));
                let path = if rel.is_empty() {
                    obj.filename.clone()
                } else {
                    format!("{rel}/{}", obj.filename)
                };
                if obj.is_folder() {
                    if obj.filename != ".thumbnails" {
                        stack.push((Some(obj.handle), local, path));
                    }
                    continue;
                }
                if is_junk(&format!("/sdcard/{path}")) || !pick.wants(&path, obj.size) {
                    continue;
                }
                files += 1;
                // shortcut: MTP compares size only (no file times); a same-size edit is missed.
                if fs::metadata(&local).is_ok_and(|m| m.len() == obj.size) {
                    kept.push(local);
                    continue;
                }
                let part = local.with_extension("part-download");
                let result: Result<(), String> = async {
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
                    let mut dl = storage
                        .download(obj.handle, ByteRange::Full)
                        .await
                        .map_err(|e| e.to_string())?;
                    while let Some(chunk) = dl.next_chunk().await {
                        if stopped() {
                            return Err(STOPPED.to_string());
                        }
                        out.write_all(&chunk.map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    }
                    drop(out);
                    // Only a fully downloaded file gets its real name.
                    fs::rename(&part, &local).map_err(|e| e.to_string())
                }
                .await;
                match result {
                    Ok(()) => copied.push(local),
                    Err(e) => {
                        let _ = fs::remove_file(&part);
                        errors.push(format!("{}: {e}", local.display()));
                    }
                }
            }
        }
    }
    let _ = fs::create_dir_all(&root);
    kept.extend(copied.iter().cloned());
    update_sums(&root, &copied, &kept, report);
    let s = Summary {
        name,
        root,
        files,
        copied: copied.len(),
        errors,
        data: Vec::new(),
        full: false,
        pick: Pick {
            files: true,
            apps: false,
            data: false,
            ..pick.clone()
        },
    };
    write_report(&s, &copied);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_adb_wifi_output() {
        assert!(
            wifi_result(
                "pair",
                "Successfully paired to 192.168.1.5:37145 [guid=adb-R5CT-x]"
            )
            .is_ok()
        );
        assert!(wifi_result("pair", "Failed: Unable to start pairing client.").is_err());
        assert!(wifi_result("connect", "connected to 192.168.1.5:41235").is_ok());
        assert!(wifi_result("connect", "already connected to 192.168.1.5:41235").is_ok());
        assert!(
            wifi_result(
                "connect",
                "failed to connect to '192.168.1.5:41235': refused (10061)"
            )
            .is_err()
        );
        assert!(
            wifi_result(
                "connect",
                "cannot connect to 192.168.1.5:41235: refused (10061)"
            )
            .is_err()
        );
        let auth =
            wifi_result("connect", "failed to authenticate to 192.168.1.5:41235").unwrap_err();
        assert!(auth.contains("Pair it first"));
    }

    #[test]
    fn finds_qr_pairing_services() {
        let (name, password, text) = qr_code();
        assert_eq!(text, format!("WIFI:T:ADB;S:{name};P:{password};;"));
        assert!(password.len() == 10 && password.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(qr_code().1, password);
        // Newer adb drops the trailing dot after the service type.
        let list = "List of discovered mdns services\n\
            adb-R58M-AbCd\t_adb-tls-connect._tcp\t192.168.1.9:40001\n\
            phone-backup-other\t_adb-tls-pairing._tcp.\t192.168.1.9:37001\n\
            phone-backup-abc\t_adb-tls-pairing._tcp.\t192.168.1.5:37145\n\
            adb-R5CT-XyZw\t_adb-tls-connect._tcp\t192.168.1.5:41235\n";
        let pair = find_service(list, "_adb-tls-pairing", &|n, _| n == "phone-backup-abc");
        assert_eq!(pair.as_deref(), Some("192.168.1.5:37145"));
        let connect = find_service(list, "_adb-tls-connect", &|_, a| {
            a.starts_with("192.168.1.5:")
        });
        assert_eq!(connect.as_deref(), Some("192.168.1.5:41235"));
        assert_eq!(
            find_service(list, "_adb-tls-pairing", &|n, _| n == "x"),
            None
        );
    }

    #[test]
    fn parses_stat_lines() {
        assert_eq!(
            parse_stat("1234 1700000000 /sdcard/DCIM/a b.jpg"),
            Some((1234, 1700000000, "/sdcard/DCIM/a b.jpg"))
        );
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
        assert!(
            phones[0]
                == Phone::Adb {
                    serial: "emulator-5554".into(),
                    model: "Pixel 8".into(),
                    wifi: false,
                    battery: None
                }
        );
        assert!(matches!(&phones[1], Phone::Adb { wifi: true, model, .. } if model == "SM G991B"));
        assert!(phones[2] == Phone::Unauthorized);
        assert!(phones[3] == Phone::Unauthorized);
        assert_eq!(phones.len(), 4);
    }

    #[test]
    fn warns_only_on_low_battery_without_charger() {
        let dump = |level: u8, usb: bool| {
            format!(
                "Current Battery Service state:\n  AC powered: false\n  USB powered: {usb}\n  Wireless powered: false\n  level: {level}\n"
            )
        };
        assert_eq!(low_battery(&dump(12, false)), Some(12));
        assert_eq!(low_battery(&dump(12, true)), None);
        assert_eq!(low_battery(&dump(20, false)), None);
        assert_eq!(low_battery(""), None);
    }

    #[test]
    fn keeps_and_checks_sums() {
        let root = std::env::temp_dir().join(format!("phone-backup-sums-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (a, b) = (root.join("files/DCIM/a.jpg"), root.join("data/sms.txt"));
        fs::create_dir_all(a.parent().unwrap()).unwrap();
        fs::create_dir_all(b.parent().unwrap()).unwrap();
        fs::write(&a, "abc").unwrap();
        fs::write(&b, "one").unwrap();
        fs::write(root.join(REPORT), "r").unwrap();
        let quiet = |_: Msg| {};
        let known = [a.clone(), b.clone()];
        update_sums(&root, &[], &known, &quiet);
        let first = fs::read_to_string(root.join(CHECKSUMS)).unwrap();
        // sha256sum format, forward slashes, and the report is left out.
        assert!(first.contains(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  files/DCIM/a.jpg\n"
        ));
        assert_eq!(first.lines().count(), 2);
        assert!(
            verify(&root, &quiet)
                .unwrap()
                .starts_with("All 2 files match")
        );

        // A file not copied again keeps its old sum, even if it changed on disk since.
        let a_sum = read_sums(&root)["files/DCIM/a.jpg"].clone();
        fs::write(&a, "abd").unwrap();
        fs::write(&b, "two").unwrap();
        update_sums(&root, std::slice::from_ref(&b), &known, &quiet);
        let second = read_sums(&root);
        assert_eq!(second["files/DCIM/a.jpg"], a_sum);
        assert_eq!(second["data/sms.txt"], sha256(&b).unwrap());
        fs::remove_file(&b).unwrap();
        let err = verify(&root, &quiet).unwrap_err();
        assert!(
            err.contains("1 file is changed, for example files/DCIM/a.jpg."),
            "{err}"
        );
        assert!(
            err.contains("1 file is missing, for example data/sms.txt."),
            "{err}"
        );
        // A gone file loses its sum on the next backup.
        update_sums(&root, &[], &[], &quiet);
        assert_eq!(read_sums(&root).len(), 1);
        fs::remove_dir_all(&root).unwrap();
        assert!(
            verify(&root, &quiet)
                .unwrap_err()
                .contains("no checksums yet")
        );
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
        assert!(is_junk("/sdcard/Movies/.cache/x.bin"));
        let wa = "/sdcard/Android/media/com.whatsapp/WhatsApp/Media";
        assert!(is_junk(&format!("{wa}/WhatsApp Stickers/s.webp")));
        assert!(is_junk(&format!("{wa}/WhatsApp Images/Sent/IMG-1.jpg")));
        assert!(!is_junk(&format!("{wa}/WhatsApp Images/IMG-2.jpg")));
        assert!(!is_junk("/sdcard/Documents/Sent/letter.pdf"));
    }

    #[test]
    fn flags_files_before_restore() {
        let now = 2_000_000_000;
        let list = [
            (PathBuf::from("a/IMG.jpg"), 5, now),
            (PathBuf::from("b/img.JPG"), 5, now),
            (PathBuf::from("b/img.JPG"), 6, now - 4 * 365 * 86_400),
            (PathBuf::from("big.mp4"), 2_000_000_000, 0),
            (PathBuf::from("e.txt"), 0, now),
        ];
        assert_eq!(flags(&list, now), [0, 2, 4, 1, 8]);
    }

    #[test]
    fn picks_folders_types_and_sizes() {
        let files: Vec<(String, u64)> = [
            ("DCIM/Camera/a.JPG", 10),
            ("DCIM/b.mp4", 2000),
            ("Music/s.mp3", 30),
            ("notes.txt", 5),
        ]
        .iter()
        .map(|(p, s)| (p.to_string(), *s))
        .collect();
        let mut pick = Pick::default();
        assert_eq!(pick.limits(), "");
        assert_eq!(tally(&files, &pick).total, 2045);
        pick.folders = BTreeSet::from(["DCIM".to_string(), String::new()]);
        pick.kinds[1] = false;
        pick.max_size = Some(1000);
        assert!(pick.wants("DCIM/x/y.png", 1000) && !pick.wants("DCIM/y.png", 1001));
        assert!(!pick.wants("DCIM/v.mp4", 1) && !pick.wants("Music/s.mp3", 1));
        assert!(pick.wants("notes.txt", 1));
        let t = tally(&files, &pick);
        // Unchosen folders still show their size, so the user sees what ticking them adds.
        assert_eq!(t.folders["Music"], 30);
        assert_eq!(t.folders["DCIM"], 10);
        assert_eq!(t.kinds, [10, 0, 0, 5, 0]);
        assert_eq!((t.total, t.count), (15, 2));
        assert_eq!(
            pick.limits(),
            "Only DCIM and files not in a folder. Only photos, music, documents and other files. Skips files over 1 KB."
        );
    }

    #[test]
    fn formats_sizes_and_reads_free_space() {
        assert_eq!(size(1_500_000_000), "1.5 GB");
        assert_eq!(size(350_000_000), "350 MB");
        assert_eq!(size(1), "1 KB");
        assert_eq!(size(999_001), "1 MB");
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
        let root =
            std::env::temp_dir().join(format!("phone-backup-restore-test-{}", std::process::id()));
        for f in [
            "files/DCIM/Camera/a.jpg",
            "files/Android/data/com.x/f",
            "files/Android/media/com.x/m.jpg",
            "sdcard_1234-AB/Music/s.mp3",
            "sdcard_9999-ZZ/x",
            "apps/com.x/base.apk",
        ] {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "abc").unwrap();
        }
        let (jobs, missing) = saved_files(&root, Some("emulated self 1234-AB")).unwrap();
        // Picking the files folder itself works too.
        let (again, _) = saved_files(&root.join("files"), Some("")).unwrap();
        // Listing for the picker keeps every card, even one this phone lacks.
        let all = backup_files(&root).unwrap();
        fs::remove_dir_all(&root).unwrap();
        let remotes: Vec<_> = jobs.iter().map(|j| j.remote.as_str()).collect();
        assert_eq!(
            remotes,
            [
                "/sdcard/Android/media/com.x/m.jpg",
                "/sdcard/DCIM/Camera/a.jpg",
                "/storage/1234-AB/Music/s.mp3"
            ]
        );
        assert_eq!(jobs[0].size, 3);
        assert_eq!(missing, ["9999-ZZ"]);
        assert_eq!(again.len(), 2);
        assert_eq!(all.len(), 4);
        assert!(all[0].2 > 0, "file times are read for the restore filter");
        assert!(saved_files(&root, None).is_err());
    }

    #[test]
    fn lists_saved_backups() {
        let root =
            std::env::temp_dir().join(format!("phone-backup-list-test-{}", std::process::id()));
        fs::create_dir_all(root.join("Pixel 8_ABC123").join("files")).unwrap();
        fs::create_dir_all(root.join("SM G991B_R58M").join("apps")).unwrap();
        fs::create_dir_all(root.join("Holiday photos")).unwrap();
        let mut found = saved_backups(&root);
        // A backup folder chosen directly is listed alone.
        let one = saved_backups(&root.join("Pixel 8_ABC123"));
        fs::remove_dir_all(&root).unwrap();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].name.as_str(), found[0].files, found[0].apps),
            ("Pixel 8", true, false)
        );
        assert_eq!(
            (found[1].name.as_str(), found[1].files, found[1].apps),
            ("SM G991B", false, true)
        );
        assert_eq!(found[0].age_days, Some(0));
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn merges_backups_without_overwriting() {
        let root =
            std::env::temp_dir().join(format!("phone-backup-merge-test-{}", std::process::id()));
        let (a, b) = (root.join("Old_A"), root.join("New_B"));
        fs::create_dir_all(a.join("files/DCIM")).unwrap();
        fs::create_dir_all(b.join("files/DCIM")).unwrap();
        fs::write(a.join("files/DCIM/same.jpg"), "same").unwrap();
        fs::write(b.join("files/DCIM/same.jpg"), "same").unwrap();
        fs::write(a.join("files/DCIM/clash.jpg"), "from a").unwrap();
        fs::write(b.join("files/DCIM/clash.jpg"), "from b").unwrap();
        fs::write(a.join("files/DCIM/new.jpg"), "new").unwrap();
        fs::write(a.join(REPORT), "a's report").unwrap();
        fs::write(b.join(REPORT), "b's report").unwrap();
        let msg = merge_backups(&a, &b, &|_| {}).unwrap();
        // Running it again changes nothing: every file is now already there.
        let again = merge_backups(&a, &b, &|_| {}).unwrap();
        let read = |p: &str| fs::read_to_string(b.join(p)).unwrap();
        let checks = (
            read("files/DCIM/clash.jpg"),
            read("files/DCIM/clash (2).jpg"),
            read("files/DCIM/new.jpg"),
            read(REPORT),
            fs::read_to_string(a.join("files/DCIM/clash.jpg")).unwrap(),
        );
        let refused = merge_backups(&a, &root, &|_| {});
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(checks.0, "from b");
        assert_eq!(checks.1, "from a");
        assert_eq!(checks.2, "new");
        assert_eq!(checks.3, "b's report");
        assert_eq!(checks.4, "from a");
        assert!(
            msg.starts_with("Copied 2 files. 1 file was already there."),
            "{msg}"
        );
        assert!(again.starts_with("Copied 0 files. 3 files were"), "{again}");
        assert!(refused.is_err());
    }

    #[test]
    fn renames_and_deletes_backups() {
        let root =
            std::env::temp_dir().join(format!("phone-backup-tidy-test-{}", std::process::id()));
        let (a, b, other) = (
            root.join("Pixel 8_A"),
            root.join("Pixel 8_B"),
            root.join("Notes"),
        );
        for d in [a.join("files"), b.join("files"), other.clone()] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(a.join("files/x.jpg"), "12345").unwrap();
        fs::write(other.join("keep.txt"), "keep").unwrap();
        let size = folder_size(&a);
        let taken = rename_backup(&a, "Pixel 8_B");
        let bad = rename_backup(&a, "Mum: old");
        let moved = rename_backup(&a, " Mum's old phone ").unwrap();
        let not_backup = delete_backup(&other);
        let deleted = delete_backup(&moved);
        let left = (moved.exists(), b.exists(), other.join("keep.txt").exists());
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(size, 5);
        assert!(taken.is_err() && bad.is_err());
        assert_eq!(moved, root.join("Mum's old phone"));
        assert!(not_backup.is_err());
        assert!(deleted.is_ok(), "{deleted:?}");
        // Only the chosen backup went; its neighbour and the other folder stayed.
        assert_eq!(left, (false, true, true));
    }

    #[test]
    fn compares_file_bytes() {
        let dir = std::env::temp_dir().join(format!("phone-backup-same-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
        let mut other = big.clone();
        other[150_000] ^= 1;
        fs::write(dir.join("a"), &big).unwrap();
        fs::write(dir.join("b"), &big).unwrap();
        fs::write(dir.join("c"), &other).unwrap();
        let r = (
            same_file(&dir.join("a"), &dir.join("b")),
            same_file(&dir.join("a"), &dir.join("c")),
        );
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(r, (true, false));
    }

    #[test]
    fn writes_backup_report() {
        let root =
            std::env::temp_dir().join(format!("phone-backup-report-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let s = Summary {
            name: "Pixel 8".into(),
            root: root.clone(),
            files: 3,
            copied: 1,
            errors: vec![r"/sdcard/b.jpg -> C:\b.jpg".into()],
            data: vec![("Contacts", true), ("Messages", false)],
            full: true,
            pick: Pick {
                apps: false,
                folders: BTreeSet::from(["DCIM".to_string()]),
                max_size: Some(1_000_000_000),
                ..Pick::default()
            },
        };
        write_report(&s, &[root.join("files").join("a.jpg")]);
        let text = fs::read_to_string(root.join(REPORT)).unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert!(text.contains("Backup of Pixel 8, "));
        assert!(
            text.contains("Files chosen: Only DCIM. Skips files over 1.0 GB."),
            "{text}"
        );
        assert!(text.contains("Copied 1 file this time. The backup holds 2 files."));
        assert!(text.contains("Messages: the phone did not allow reading it"));
        assert!(text.contains("Could not copy (1):\n/sdcard/b.jpg"));
        assert!(text.contains(&format!(
            "Copied (1):\n{}",
            Path::new("files").join("a.jpg").display()
        )));
    }

    #[test]
    fn quotes_shell_paths() {
        assert_eq!(sh_quote("/sdcard/a b.jpg"), "'/sdcard/a b.jpg'");
        assert_eq!(sh_quote("/sdcard/it's $x.jpg"), r"'/sdcard/it'\''s $x.jpg'");
    }

    #[test]
    fn renames_clashing_names() {
        let job = |remote: &str, local: &str| Job {
            remote: remote.into(),
            size: 0,
            mtime: 0,
            local: PathBuf::from(local),
        };
        let mut jobs = vec![
            job("/sdcard/b", "x/IMG.jpg"),
            job("/sdcard/a", "x/img.jpg"),
            job("/sdcard/c", "x/img.JPG"),
            job("/sdcard/d", "x/notes"),
        ];
        unclash(&mut jobs);
        let locals: Vec<_> = jobs
            .iter()
            .map(|j| j.local.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            locals,
            ["x/img.jpg", "x/IMG (2).jpg", "x/img (3).JPG", "x/notes"]
        );
    }

    /// Backs up and restores a real phone or emulator. Run with
    /// `PB_E2E_SERIAL=emulator-5554 cargo test e2e -- --ignored --nocapture`.
    /// Expects the files that docs/e2e-seed.sh puts in /sdcard/E2E.
    #[test]
    #[ignore]
    fn e2e_backup_and_restore() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let dest = std::env::temp_dir().join(format!("pb-e2e-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);
        let all = Pick {
            files: true,
            apps: true,
            data: true,
            ..Pick::default()
        };
        let quiet = |_: Msg| {};
        let first = adb_backup(&serial, "E2E", &dest, all.clone(), &quiet).unwrap();
        println!(
            "first: files {} copied {} errors {:?} data {:?}",
            first.files, first.copied, first.errors, first.data
        );
        let report = fs::read_to_string(first.root.join(REPORT)).unwrap();
        assert!(
            report.contains(
                &["files", "E2E", "Camera", "IMG.jpg"]
                    .iter()
                    .collect::<PathBuf>()
                    .display()
                    .to_string()
            ),
            "{report}"
        );
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        let e2e = first.root.join("files").join("E2E");
        for (name, body) in [
            // Shared storage ignores case, so img.jpg replaced IMG.jpg on the phone itself.
            ("Camera/IMG.jpg", "two2\n"),
            ("Docs/it's $x.txt", "q\n"),
            ("Docs/trailing dot_", "d\n"),
            ("Docs/_CON.txt", "e\n"),
            ("Docs/ünïcødé 日本.txt", "u\n"),
            ("Docs/empty.bin", ""),
            ("Docs/ leading space.txt", "lead\n"),
            ("Docs/-dash.txt", "dash\n"),
            ("sp ace dir/x y.txt", "s\n"),
            ("many/f450.txt", "450\n"),
        ] {
            assert_eq!(
                fs::read_to_string(e2e.join(name)).unwrap_or_else(|e| panic!("{name}: {e}")),
                body,
                "{name}"
            );
        }
        assert!(!e2e.join(".thumbnails").exists());
        assert_eq!(
            fs::metadata(e2e.join("Camera/big.bin")).unwrap().len(),
            20 << 20
        );
        let deep: PathBuf = (1..=12)
            .fold(e2e.join("deep"), |p, i| {
                p.join(format!("folder_with_a_fairly_long_name_{i}"))
            })
            .join("deepfile.txt");
        assert_eq!(fs::read_to_string(&deep).unwrap(), "deep\n");
        assert!(first.root.join("apps").read_dir().unwrap().count() > 0);
        let vcf = fs::read_to_string(first.root.join("data").join("contacts.vcf")).unwrap();
        println!("{vcf}");
        assert!(vcf.contains("TEL:+1 555 0100"));
        let data = first.root.join("data");
        let read = |name: &str| fs::read_to_string(data.join(name)).unwrap_or_default();
        assert!(read("calendar.txt").contains("title=E2E dentist"));
        let mms = read("mms.txt");
        assert!(
            mms.contains("text=Hello from MMS") && mms.contains("address=+15550100"),
            "{mms}"
        );
        let pic = fs::read_dir(data.join("mms"))
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().ends_with("_e2e.jpg"))
            .expect("the picture of the picture message");
        assert_eq!(fs::read_to_string(pic.path()).unwrap(), "JPEGDATA\n");
        let wa = e2e.join("WhatsApp/Media/WhatsApp Images");
        assert!(wa.join("IMG-received.jpg").exists());
        assert!(!wa.join("Sent").exists() && !e2e.join(".cache").exists());
        assert!(!e2e.join("WhatsApp/Media/WhatsApp Stickers").exists());

        let sums = fs::read_to_string(first.root.join(CHECKSUMS)).unwrap();
        assert!(sums.contains("  files/E2E/Docs/_CON.txt\n"), "{sums}");
        assert!(sums.contains("  data/contacts.vcf\n"), "{sums}");
        assert!(!sums.contains(&format!("  {REPORT}\n")), "{sums}");

        // Sizes before a backup see the same files, without the junk.
        let (sizes, apps) = measure(&serial);
        assert!(apps > 0);
        assert!(sizes.contains(&("E2E/Camera/big.bin".into(), 20 << 20)));
        assert!(!sizes.iter().any(|(p, _)| p.starts_with("E2E/.cache/")));

        // Only one folder, only documents, nothing over 1 MB.
        let some = Pick {
            apps: false,
            data: false,
            folders: BTreeSet::from(["E2E".to_string()]),
            kinds: [false, false, false, true, false],
            max_size: Some(1_000_000),
            ..Pick::default()
        };
        let dest2 = dest.join("some");
        let part = adb_backup(&serial, "E2E", &dest2, some, &quiet).unwrap();
        let held = backup_files(&part.root).unwrap();
        assert!(held.iter().all(|(p, ..)| {
            p.starts_with(part.root.join("files").join("E2E"))
                && p.extension().is_some_and(|e| e == "txt")
        }));
        assert_eq!(held.len(), part.files);
        assert!(part.root.join("files/E2E/Docs/a_b.txt").exists());
        let _ = fs::remove_dir_all(&dest2);

        let again = adb_backup(&serial, "E2E", &dest, all.clone(), &quiet).unwrap();
        assert_eq!(
            (again.copied, again.files, again.errors.len()),
            (0, first.files, 0)
        );
        assert_eq!(
            fs::read_to_string(first.root.join(CHECKSUMS)).unwrap(),
            sums
        );
        let msg = verify(&first.root, &quiet).unwrap();
        assert!(msg.starts_with("All "), "{msg}");

        // Restore: delete some files on the phone, then bring them back.
        let deep_remote = (1..=12).fold("/sdcard/E2E/deep".to_string(), |p, i| {
            format!("{p}/folder_with_a_fairly_long_name_{i}")
        }) + "/deepfile.txt";
        let gone = [
            "/sdcard/E2E/Docs/it's $x.txt",
            "/sdcard/E2E/Docs/ünïcødé 日本.txt",
            "/sdcard/E2E/Docs/empty.bin",
            "/sdcard/E2E/many/f7.txt",
            "/sdcard/E2E/Docs/CON.txt",
            "/sdcard/E2E/Docs/trailing dot.",
            &deep_remote,
        ];
        shell_each(&serial, "rm -f", &gone);
        // adb shell on Windows turns LF into CRLF.
        let sh = |cmd: &str| shell(&serial, cmd).replace("\r\n", "\n");
        // Choosing files restores only those.
        let one = HashSet::from([first.root.join("files/E2E/many/f7.txt")]);
        let msg = restore_files(&serial, &first.root, Some(&one), true, &quiet).unwrap();
        assert!(msg.starts_with("Files: will copy 1 file ("), "{msg}");
        let msg = restore_files(&serial, &first.root, None, true, &quiet).unwrap();
        assert!(
            msg.starts_with(&format!("Files: will copy {} (", files(gone.len()))),
            "{msg}"
        );
        let msg = restore_files(&serial, &first.root, None, false, &quiet).unwrap();
        println!("restore: {msg}");
        assert!(
            msg.starts_with(&format!("Copied {} back", files(gone.len()))),
            "{msg}"
        );
        assert_eq!(sh("cat '/sdcard/E2E/Docs/it'\\''s $x.txt'"), "q\n");
        assert_eq!(
            sh("cat '/sdcard/E2E/Docs/CON.txt' '/sdcard/E2E/Docs/trailing dot.'"),
            "e\nd\n"
        );
        assert_eq!(sh(&format!("cat {}", sh_quote(&deep_remote))), "deep\n");
        // Windows-safe names stay on the computer, never copied back as extra files.
        assert_eq!(sh("ls /sdcard/E2E/Docs | grep -c -e _CON -e 'dot_'"), "0\n");
        let msg = restore_files(&serial, &first.root, None, false, &quiet).unwrap();
        assert!(msg.starts_with("Copied 0 files back"), "{msg}");
        // Every saved app is still installed, so nothing is reinstalled.
        let msg = restore_apps(&serial, &first.root, true, &quiet).unwrap();
        assert_eq!(msg, "Apps: every saved app is already on the phone.");
        let msg = restore_apps(&serial, &first.root, false, &quiet).unwrap();
        assert!(msg.starts_with("Installed 0 of"), "{msg}");
        fs::remove_dir_all(&dest).unwrap();
    }

    /// Phone to phone with the same emulator on USB (PB_E2E_SERIAL) and on TCP
    /// (`adb connect 127.0.0.1:5555`) as the two ends. It is one phone, so this tests the flow,
    /// not files really moving between two phones: every file is already "on the other phone".
    #[test]
    #[ignore]
    fn e2e_phone_to_phone() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let tcp = "127.0.0.1:5555";
        let dest = std::env::temp_dir().join(format!("pb-e2e-p2p-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);
        let pick = Pick {
            files: true,
            apps: false,
            data: false,
            ..Pick::default()
        };
        let quiet = |_: Msg| {};
        let (folder, found) =
            transfer_check((&serial, "From"), (tcp, "To"), &dest, pick.clone(), &quiet);
        let folder = folder.expect("the first phone was backed up");
        let found = found.unwrap();
        assert!(
            found.starts_with("From is backed up.\nFiles: will copy 0 files"),
            "{found}"
        );
        assert!(
            !folder.join(RESTORE_REPORT).exists(),
            "a check writes no report"
        );
        let done = transfer_now((tcp, "To"), &dest, &folder, pick, true, &quiet).unwrap();
        println!("{done}");
        assert!(done.starts_with("Safety backup of To saved in"), "{done}");
        // The safety backup has its own folder beside the source's.
        let backups = saved_backups(&dest);
        assert_eq!(backups.len(), 2);
        assert!(folder.join(RESTORE_REPORT).exists());
        fs::remove_dir_all(&dest).unwrap();
    }

    /// Needs the same emulator on USB (PB_E2E_SERIAL) and on TCP (`adb connect 127.0.0.1:5555`).
    #[test]
    #[ignore]
    fn e2e_stop_disconnect_and_duplicate() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let tcp = "127.0.0.1:5555";
        let adb_count = || {
            detect()
                .iter()
                .filter(|p| matches!(p, Phone::Adb { .. }))
                .count()
        };
        assert_eq!(adb_count(), 1, "one phone on USB and TCP is listed once");

        let dest = std::env::temp_dir().join(format!("pb-e2e-stop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);
        let pick = Pick {
            files: true,
            apps: false,
            data: false,
            ..Pick::default()
        };
        let quiet = |_: Msg| {};
        // Interrupt after the first folder is copied, not after a fixed time: a fast phone
        // finishes the whole seed before any timer fires.
        fn after_first_folder(act: impl Fn()) -> impl Fn(Msg) {
            let seen = std::cell::Cell::new(0);
            move |m| {
                if matches!(m, Msg::Progress { .. }) {
                    seen.set(seen.get() + 1);
                    if seen.get() == 2 {
                        act();
                    }
                }
            }
        }
        let stop = || STOP.store(true, Ordering::Relaxed);
        let r = adb_backup(
            &serial,
            "E2E",
            &dest,
            pick.clone(),
            &after_first_folder(stop),
        );
        assert_eq!(r.err().as_deref(), Some(STOPPED));
        // The phone list keeps working after a stopped job.
        assert_eq!(adb_count(), 1, "still listed once after Stop");
        STOP.store(false, Ordering::Relaxed);

        let unplug = || {
            let _ = adb().args(["disconnect", tcp]).output();
        };
        let _ = fs::remove_dir_all(&dest);
        let r = adb_backup(tcp, "E2E", &dest, pick.clone(), &after_first_folder(unplug));
        assert_eq!(r.err().as_deref(), Some(DISCONNECTED));
        // Interrupted copies are never mistaken for finished ones.
        let again = adb_backup(&serial, "E2E", &dest, pick.clone(), &quiet).unwrap();
        assert!(
            again.errors.is_empty() && again.copied > 0,
            "{:?}",
            again.errors
        );
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn reads_mms_parts() {
        assert_eq!(
            mms_part("Row: 2 _id=3, ct=image/jpeg, cl=pic.jpg"),
            Some((3, "pic.jpg"))
        );
        assert_eq!(
            mms_part("Row: 3 _id=4, ct=video/mp4, cl=NULL"),
            Some((4, "part"))
        );
        assert_eq!(mms_part("Row: 1 _id=2, ct=text/plain, cl=NULL"), None);
        assert_eq!(mms_part("Row: 0 _id=1, ct=NULL, cl=NULL"), None);
    }

    #[test]
    fn missing_local_file_needs_copy() {
        let job = Job {
            remote: "/sdcard/x".into(),
            size: 1,
            mtime: 0,
            local: PathBuf::from("does/not/exist"),
        };
        assert!(needs_copy(&job));
    }
}

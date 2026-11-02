//! Copies an Android phone to this computer over ADB (USB or Wi-Fi) or MTP.
//! Everything runs locally; nothing is sent to any server.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

mod adb;
mod checksums;
mod merge;
mod mtp;
mod report;
mod restore;

pub use adb::*;
pub use checksums::*;
pub use merge::*;
pub use mtp::*;
pub use report::*;
pub use restore::*;

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
    /// The download of adb ended.
    GotAdb(Result<(), String>),
    /// What the update check found: a newer version, or None for up to date.
    Update(Result<Option<String>, String>),
    Done(Result<Vec<Summary>, String>),
    /// What `measure` found on the phone with this serial.
    Sizes(String, Vec<(String, u64)>, u64),
    Restored(Result<String, String>),
    /// The backup folder checked and what the check found.
    Verified(PathBuf, Result<String, String>),
    /// An export ended: its folder, and what it made or why it stopped.
    Exported(PathBuf, Result<String, String>),
    /// A phone-to-phone check: the source phone's serial, its backup folder and the files and
    /// apps of this run, once backed up.
    Transferred(
        Option<(String, PathBuf, HashSet<PathBuf>)>,
        Result<String, String>,
    ),
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
    /// No phone found, and Windows could not use some USB device: maybe this phone on a bad cable.
    UsbFailed,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn installs_adb_only_when_the_download_is_whole() {
        let root = std::env::temp_dir().join(format!("phone-backup-adb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (src, part, dir) = (root.join("src"), root.join("part"), root.join("app"));
        let tools = src.join("platform-tools");
        fs::create_dir_all(&tools).unwrap();
        fs::create_dir_all(&part).unwrap();
        fs::create_dir_all(&dir).unwrap();
        let zip_of = |name: &str| {
            let zip = root.join(name);
            let status = adb::windows_tool("tar.exe")
                .arg("-a")
                .arg("-cf")
                .arg(&zip)
                .arg("-C")
                .arg(&src)
                .arg("platform-tools")
                .status()
                .unwrap();
            assert!(status.success());
            zip
        };
        fs::write(tools.join("readme.txt"), "").unwrap();
        let broken = zip_of("broken.zip");
        fs::write(tools.join("adb.exe"), "").unwrap();
        let whole = zip_of("whole.zip");

        assert!(adb::install_adb(&broken, &part, &dir).is_err());
        assert!(!dir.join("platform-tools").exists());

        fs::remove_dir_all(part.join("platform-tools")).unwrap();
        adb::install_adb(&whole, &part, &dir).unwrap();
        assert!(dir.join("platform-tools/adb.exe").is_file());

        // A folder already there is the user's: refuse, and leave it alone.
        fs::remove_file(dir.join("platform-tools/adb.exe")).unwrap();
        fs::write(dir.join("platform-tools/mine.txt"), "").unwrap();
        fs::remove_dir_all(part.join("platform-tools")).ok();
        assert!(adb::install_adb(&whole, &part, &dir).is_err());
        assert!(dir.join("platform-tools/mine.txt").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    /// Checks that Google's address still works: `cargo test google_adb -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "downloads about 8 MB from Google"]
    fn downloads_google_adb() {
        let dir = std::env::temp_dir().join(format!("phone-backup-google-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        adb::get_adb_into(&dir).unwrap();
        let adb_exe = dir.join("platform-tools/adb.exe");
        assert!(
            Command::new(adb_exe)
                .arg("version")
                .status()
                .unwrap()
                .success()
        );
        assert!(!dir.join("platform-tools.part").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Checks that GitHub's address still works: `cargo test github_update -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "asks GitHub over the internet"]
    fn github_update_address_answers() {
        println!("{:?}", adb::check_update().unwrap());
    }

    #[test]
    fn spots_a_newer_release() {
        let at = |v: &str| format!("https://github.com/a/b/releases/tag/v{v}");
        let new = adb::newer_release;
        assert_eq!(new(&at("0.10.0"), "0.9.0"), Ok(Some("0.10.0".into())));
        assert_eq!(new(&at("1.0.0"), "0.9.9"), Ok(Some("1.0.0".into())));
        assert_eq!(new(&at("0.9.0"), "0.9.0"), Ok(None));
        assert_eq!(new(&at("0.8.0"), "0.9.0"), Ok(None));
        assert!(new("", "0.9.0").is_err());
        assert!(new("https://github.com/a/b/releases", "0.9.0").is_err());
        assert!(new(&at("1.0.0-rc1"), "0.9.0").is_err());
    }

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
    fn spots_a_usb_device_windows_could_not_read() {
        let device = |id: &str, class: &str| {
            format!(
                "Instance ID:    {id}\r\nClass Name:     {class}\r\nStatus:     Problem\r\n\r\n"
            )
        };
        let unread = device(r"USB\VID_0000&PID_0002\5&2d7e1a2c&0&3", "USB");
        let file_transfer = device(r"USB\VID_22B8&PID_2E82\ABC123", "WPD");
        let camera = device(r"USB\VID_30C9&PID_00C2&MI_00\6&1234ABCD", "Camera");
        let other_bus = device(r"PCI\VEN_0000&DEV_0002\1", "WPD");
        assert!(usb_failed(&unread));
        assert!(usb_failed(&file_transfer));
        assert!(!usb_failed(&camera));
        assert!(!usb_failed(&other_bus));
        // The class of one device must not count for the USB ID of another.
        assert!(!usb_failed(&(camera + &other_bus)));
        assert!(!usb_failed("No devices were found on the system.\r\n"));
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
    fn scopes_apps_to_one_run() {
        let a = |p: &str| PathBuf::from(p);
        let packages = vec![
            ("old.app".to_string(), vec![a("apps/old.app/base.apk")]),
            (
                "new.app".to_string(),
                vec![a("apps/new.app/base.apk"), a("apps/new.app/split.apk")],
            ),
        ];
        let ran = HashSet::from([a("apps/new.app/base.apk"), a("files/x.jpg")]);
        assert_eq!(
            only_in(packages, &ran),
            vec![("new.app".to_string(), vec![a("apps/new.app/base.apk")])]
        );
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

        // A file that changed on the phone replaces the old copy whole, through the temp folder.
        shell(&serial, "echo changed > /sdcard/E2E/Docs/-dash.txt");
        let changed = adb_backup(&serial, "E2E", &dest, all.clone(), &quiet).unwrap();
        assert_eq!((changed.copied, changed.errors.len()), (1, 0));
        assert_eq!(
            fs::read_to_string(e2e.join("Docs/-dash.txt")).unwrap(),
            "changed\n"
        );
        assert!(!first.root.join(".pulling").exists());
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
        let msg = restore_apps(&serial, &first.root, None, true, &quiet).unwrap();
        assert_eq!(msg, "Apps: every saved app is already on the phone.");
        let msg = restore_apps(&serial, &first.root, None, false, &quiet).unwrap();
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
        let (run, found) =
            transfer_check((&serial, "From"), (tcp, "To"), &dest, pick.clone(), &quiet);
        let (folder, ran) = run.expect("the first phone was backed up");
        let found = found.unwrap();
        assert!(
            found.starts_with("From is backed up.\nFiles: will copy 0 files"),
            "{found}"
        );
        assert!(
            !folder.join(RESTORE_REPORT).exists(),
            "a check writes no report"
        );
        // A file from an earlier backup of the first phone is in the folder but not in this run.
        let stale = folder.join("files/E2E/stale-from-earlier-run.txt");
        fs::write(&stale, "old").unwrap();
        let done = transfer_now((tcp, "To"), &dest, &folder, &ran, pick, true, &quiet).unwrap();
        println!("{done}");
        assert!(done.starts_with("Safety backup of To saved in"), "{done}");
        // The safety backup has its own folder beside the source's.
        let backups = saved_backups(&dest);
        assert_eq!(backups.len(), 2);
        assert!(folder.join(RESTORE_REPORT).exists());
        assert!(
            !shell(tcp, "ls /sdcard/E2E").contains("stale-from-earlier-run"),
            "a file outside this run must not be restored"
        );
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

    /// Needs the emulator (PB_E2E_SERIAL). Stop during the contacts, calls and messages step ends
    /// the backup and leaves the last full copy of that data as it was.
    #[test]
    #[ignore]
    fn e2e_stop_in_data_step_keeps_old_data() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let dest = std::env::temp_dir().join(format!("pb-e2e-data-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);
        let pick = Pick {
            files: false,
            apps: false,
            data: true,
            ..Pick::default()
        };
        let first = adb_backup(&serial, "E2E", &dest, pick.clone(), &|_| {}).unwrap();
        let sms = first.root.join("data").join("sms.txt");
        let before = fs::read_to_string(&sms).unwrap();
        assert!(!before.is_empty());
        let stop_when_saving = |m: Msg| {
            if matches!(&m, Msg::Status(s) if s.starts_with("Saving contacts")) {
                STOP.store(true, Ordering::Relaxed);
            }
        };
        let r = adb_backup(&serial, "E2E", &dest, pick, &stop_when_saving);
        STOP.store(false, Ordering::Relaxed);
        assert_eq!(r.err().as_deref(), Some(STOPPED));
        assert_eq!(fs::read_to_string(&sms).unwrap(), before);
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

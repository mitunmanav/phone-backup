use super::*;
use std::process::Output;
use std::time::Instant;

/// adb.exe next to this app, then on PATH, then in the default Android SDK folder.
/// Most people never install the SDK, so shipping adb.exe beside the app is the normal case.
pub(crate) fn adb_path() -> Option<PathBuf> {
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
pub(crate) fn adb() -> Command {
    let mut c = Command::new(adb_path().unwrap_or_else(|| "adb".into()));
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000); // CREATE_NO_WINDOW
    c
}

/// Phones on ADB; if none, phones in "File transfer" (MTP) mode.
pub fn detect() -> Vec<Phone> {
    let out = logged(adb().args(["devices", "-l"]));
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
            let out = logged(adb().args(["-s", serial, "shell", "dumpsys battery"]))
                .map(|o| o.stdout)
                .unwrap_or_default();
            *battery = low_battery(&String::from_utf8_lossy(&out));
        }
    }
    if !phones.is_empty() {
        return phones;
    }
    let mtp: Vec<Phone> = mtp_rs::mtp::MtpDevice::list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| !NOT_PHONES.contains(&d.vendor_id))
        .map(|d| Phone::Mtp {
            name: d
                .product
                .or(d.manufacturer)
                .unwrap_or_else(|| "Phone".into()),
        })
        .collect();
    if !mtp.is_empty() {
        return mtp;
    }
    let problems =
        logged(windows_tool("pnputil.exe").args(["/enum-devices", "/connected", "/problem"]))
            .map(|o| o.stdout)
            .unwrap_or_default();
    if usb_failed(&String::from_utf8_lossy(&problems)) {
        vec![Phone::UsbFailed]
    } else {
        Vec::new()
    }
}

/// In pnputil's list of devices with a problem: a USB device Windows could not read (it gets a
/// made-up maker number of 0000), or one whose file transfer driver would not start (class WPD).
/// Only these values stay the same in every Windows language; the labels beside them do not.
pub(super) fn usb_failed(pnputil: &str) -> bool {
    pnputil.replace('\r', "").split("\n\n").any(|device| {
        device.contains(r"USB\VID_0000&PID_")
            || device.contains(r"USB\VID_") && device.lines().any(|l| l.ends_with(" WPD"))
    })
}

/// USB makers of cameras and watches, which speak the same file protocol as phones.
// shortcut: a list of makers that sell no Android phones; add one when its device shows up as a phone.
pub(super) const NOT_PHONES: [u16; 8] = [
    0x04a9, // Canon
    0x04b0, // Nikon
    0x04cb, // Fujifilm
    0x054c, // Sony cameras (Xperia phones use 0x0fce)
    0x07b4, // Olympus, OM System
    0x091e, // Garmin
    0x25fb, // Pentax, Ricoh
    0x2672, // GoPro
];

pub(super) fn parse_adb_devices(text: &str) -> Vec<Phone> {
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
pub(super) fn low_battery(dumpsys: &str) -> Option<u8> {
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

pub(super) const NO_ADB: &str =
    "This computer does not have adb yet. Get it in Settings, under Phone connection.";

/// Google publishes no checksum for this download, so HTTPS is the only check on it.
const PLATFORM_TOOLS: &str =
    "https://dl.google.com/android/repository/platform-tools-latest-windows.zip";

/// curl or tar from Windows itself. Never searched for: this app's folder may hold anything,
/// such as a Downloads folder with a file of the same name.
pub(super) fn windows_tool(name: &str) -> Command {
    let root = std::env::var_os("SystemRoot").map_or_else(|| r"C:\Windows".into(), PathBuf::from);
    let mut c = Command::new(root.join("System32").join(name));
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000); // CREATE_NO_WINDOW
    c
}

/// Runs a Windows tool; on failure, what it printed.
fn run_tool(c: &mut Command) -> Result<(), String> {
    let out = c.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
}

/// Downloads Google's Platform Tools next to this app, where `adb_path` looks. The caller has
/// the user's agreement to Google's terms.
pub fn get_adb() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("This app's folder cannot be found.")?;
    get_adb_into(dir)
}

pub(super) fn get_adb_into(dir: &Path) -> Result<(), String> {
    let part = dir.join("platform-tools.part");
    let _ = fs::remove_dir_all(&part);
    fs::create_dir_all(&part).map_err(|e| {
        format!(
            "Cannot save to {}: {e}. Move this app to a folder you can change, such as your Desktop, then try again.",
            dir.display()
        )
    })?;
    let zip = part.join("platform-tools.zip");
    let result = run_tool(
        windows_tool("curl.exe")
            .args(["--fail", "--location", "--silent", "--show-error"])
            .args(["--proto", "=https", "--proto-redir", "=https"])
            // Give up on a dead connection, not on a slow one.
            .args([
                "--connect-timeout",
                "30",
                "--speed-limit",
                "1",
                "--speed-time",
                "60",
            ])
            .arg("--output")
            .arg(&zip)
            .arg(PLATFORM_TOOLS),
    )
    .map_err(|e| {
        format!("Could not download adb ({e}). Check the internet connection, then try again.")
    })
    .and_then(|()| install_adb(&zip, &part, dir));
    let _ = fs::remove_dir_all(&part);
    result
}

/// Asks GitHub where its latest-release address points. Some(version) when that is newer than
/// this app. Only ever called from a click.
pub fn check_update() -> Result<Option<String>, String> {
    let reach = |e: String| {
        format!("Could not reach GitHub ({e}). Check the internet connection, then try again.")
    };
    let out = windows_tool("curl.exe")
        .args(["--silent", "--show-error", "--proto", "=https"])
        .args(["--max-time", "30", "--output", "NUL"])
        .args(["--write-out", "%{redirect_url}"])
        .arg(concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/latest"))
        .output()
        .map_err(|e| reach(e.to_string()))?;
    if !out.status.success() {
        return Err(reach(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    newer_release(
        &String::from_utf8_lossy(&out.stdout),
        env!("CARGO_PKG_VERSION"),
    )
}

/// The version in a ".../releases/tag/v1.2.3" address, when it is newer than `current`.
pub(super) fn newer_release(address: &str, current: &str) -> Result<Option<String>, String> {
    let numbers = |v: &str| {
        v.split('.')
            .map(|n| n.parse::<u32>().ok())
            .collect::<Option<Vec<_>>>()
    };
    let latest = address.rsplit_once("/tag/v").map_or("", |x| x.1.trim());
    let Some(new) = numbers(latest) else {
        return Err("GitHub's answer could not be read. Try again later.".into());
    };
    Ok((new > numbers(current).unwrap_or_default()).then(|| latest.to_string()))
}

/// Unpacks the download into `part`, a folder of its own, and moves it into place only once
/// adb.exe is there, so a broken download never leaves half a tool behind.
pub(super) fn install_adb(zip: &Path, part: &Path, dir: &Path) -> Result<(), String> {
    run_tool(
        windows_tool("tar.exe")
            .arg("-xf")
            .arg(zip)
            .arg("-C")
            .arg(part),
    )
    .map_err(|e| format!("Could not unpack adb ({e}). Try again."))?;
    let unpacked = part.join("platform-tools");
    if !unpacked.join("adb.exe").is_file() {
        return Err("The download did not contain adb. Try again.".into());
    }
    let target = dir.join("platform-tools");
    if target.exists() {
        return Err(format!(
            "{} already exists without adb.exe in it. Move or rename that folder, then try again.",
            target.display()
        ));
    }
    fs::rename(&unpacked, &target)
        .map_err(|e| format!("Cannot save to {}: {e}. Try again.", dir.display()))
}

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
    let check = logged(adb().args(["mdns", "check"]));
    if check.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("unavailable")) {
        return Some(Err("adb on this computer cannot look for phones on the Wi-Fi. Pair with a code below instead.".into()));
    }
    // Only the wait for the scan can be stopped; once paired, the connect is seen through.
    let find = |service: &str, secs: u64, stoppable: bool, want: &dyn Fn(&str, &str) -> bool| {
        let end = std::time::Instant::now() + Duration::from_secs(secs);
        while std::time::Instant::now() < end
            && (!stoppable || QR_WAIT.load(Ordering::Relaxed) == id)
        {
            let out = logged(adb().args(["mdns", "services"])).ok()?;
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
pub(super) fn find_service(
    list: &str,
    service: &str,
    want: &dyn Fn(&str, &str) -> bool,
) -> Option<String> {
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

/// Runs a shell command on the phone and returns its stdout. Stderr (e.g. "Permission denied"
/// for unreadable folders) is ignored on purpose: we keep whatever could be read.
pub(super) fn shell(serial: &str, cmd: &str) -> String {
    let mut c = adb();
    c.args(["-s", serial, "shell", cmd]);
    String::from_utf8_lossy(&run(c)).into_owned()
}

/// Runs a command and returns its stdout, or what it printed so far if Stop is pressed.
/// Unlike `output()`, this kills the command on Stop: a long `find` or `adb pull` would
/// otherwise keep running after the window closes.
pub(super) fn run(mut c: Command) -> Vec<u8> {
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
    let started = Instant::now();
    let mut code = None;
    loop {
        match child.try_wait() {
            Ok(None) if stopped() => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Ok(Some(status)) => {
                code = status.code();
                break;
            }
            Err(_) => break,
        }
    }
    log_adb(&c, code, started);
    reader.join().unwrap_or_default()
}

/// `Command::output`, with the command in the debug log.
pub(super) fn logged(c: &mut Command) -> std::io::Result<Output> {
    let started = Instant::now();
    let out = c.output();
    let code = out.as_ref().ok().and_then(|o| o.status.code());
    log_adb(c, code, started);
    out
}

/// With PB_LOG set, adds the command, its exit code and its time to a file in the temp folder.
fn log_adb(c: &Command, code: Option<i32>, started: Instant) {
    use std::io::Write;
    if std::env::var_os("PB_LOG").is_some()
        && let Ok(mut log) = fs::File::options()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("phone-backup-adb.log"))
    {
        let ms = started.elapsed().as_millis();
        let _ = writeln!(log, "{code:?} {ms} ms {c:?}");
    }
}

/// True while adb still sees the phone. Listing commands just return nothing when it is gone.
pub(crate) fn connected(serial: &str) -> bool {
    logged(adb().args(["-s", serial, "get-state"]))
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "device")
}

pub(super) const DISCONNECTED: &str = "The phone was disconnected. Files copied so far are kept; plug it back in and press Back up again to continue.";

/// Why the running job must end now: Stop was pressed or the phone is gone. Output read from the
/// phone just before this answered Some is cut short and must not be saved.
pub(super) fn interrupted(serial: &str) -> Option<&'static str> {
    if stopped() {
        Some(STOPPED)
    } else if !connected(serial) {
        Some(DISCONNECTED)
    } else {
        None
    }
}

/// ro.serialno is the same over USB and Wi-Fi, unlike the adb serial.
/// Not stoppable like `shell`: the phone list calls it too, and Stop stays set after a job ends.
pub(super) fn phone_id(serial: &str) -> String {
    let out = logged(adb().args(["-s", serial, "shell", "getprop ro.serialno"]))
        .map(|o| o.stdout)
        .unwrap_or_default();
    match String::from_utf8_lossy(&out).trim() {
        "" => serial.to_string(),
        s => s.to_string(),
    }
}

/// One file to copy: remote path, its size and mtime (Unix seconds), and where it goes locally.
#[derive(Clone)]
pub(crate) struct Job {
    pub(crate) remote: String,
    pub(crate) size: u64,
    pub(super) mtime: i64,
    pub(crate) local: PathBuf,
}

/// Parses a `stat -c '%s %Y %n'` line into (size, mtime, rest).
pub(super) fn parse_stat(line: &str) -> Option<(u64, i64, &str)> {
    let mut it = line.splitn(3, ' ');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?,
    ))
}

/// Lists every installed app's installer files as `size mtime package path`.
pub(super) const APPS: &str = r#"for p in $(pm list packages -3 | cut -d: -f2); do for a in $(pm path $p | cut -d: -f2); do stat -c "%s %Y $p %n" $a; done; done"#;

/// The phone's shared storage and any SD cards: (folder on the phone, folder in the backup).
pub(super) fn storages(serial: &str) -> Vec<(String, String)> {
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
pub(super) fn listing(serial: &str, remote_root: &str) -> Vec<(u64, i64, String)> {
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
    adb_backup_run(serial, model, dest, pick, report).map(|(s, _)| s)
}

/// `adb_backup`, plus the files and app installers this run covered. The backup folder keeps
/// everything from earlier runs too, so a transfer restores only this list.
pub(super) fn adb_backup_run(
    serial: &str,
    model: &str,
    dest: &Path,
    pick: Pick,
    report: &dyn Fn(Msg),
) -> Result<(Summary, HashSet<PathBuf>), String> {
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
    let ran: HashSet<PathBuf> = listed.iter().cloned().collect();
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

    // shortcut: why the helper failed is dropped, so the user only sees contacts or messages not saved; show it when a test phone refuses the helper.
    let helper = pick.data.then(|| {
        status("Starting the helper on the phone. Allow it if the phone asks…".into());
        super::helper::start(serial).ok()
    });
    let helper = helper.flatten();

    // Contacts, call log and SMS as text. Many phones refuse these to ADB; the summary says so.
    status("Saving contacts, calls, messages and calendar…".into());
    let data_dir = root.join("data");
    let _ = fs::create_dir_all(if pick.data { &data_dir } else { &root });
    let mut data = Vec::new();
    let mut written = Vec::new();
    let mut cut = None;
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
        let (out, json) = super::helper::fuller(
            shell(serial, query),
            helper.as_ref().and_then(|h| h.read(file)),
        );
        // A cut-off answer still starts with "Row:" and would replace the last full copy.
        if let Some(e) = interrupted(serial) {
            cut = Some(e);
            break;
        }
        let ok = out.starts_with("Row:") || out.starts_with("No result");
        if ok {
            if file == "contacts.txt" {
                let _ = fs::write(data_dir.join("contacts.vcf"), vcard(&out));
                written.push(data_dir.join("contacts.vcf"));
            }
            if let Some(json) = json {
                let path = data_dir.join(file).with_extension("json");
                let _ = fs::write(&path, json);
                written.push(path);
            }
            if file == "mms.txt" {
                mms_attachments(serial, &data_dir.join("mms"));
                cut = interrupted(serial);
            }
            let _ = fs::write(data_dir.join(file), out);
            written.push(data_dir.join(file));
        }
        data.push((label, ok));
    }
    drop(helper);
    if let Some(e) = cut {
        // Their old sums no longer match what was just written.
        update_sums(&root, &written, &[], report);
        return Err(e.into());
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
    Ok((s, ran))
}

/// Saves the pictures, videos and other files sent in picture messages as `<part id>_<name>`.
pub(super) fn mms_attachments(serial: &str, dir: &Path) {
    let parts = shell(
        serial,
        "content query --uri content://mms/part --projection _id:ct:cl",
    );
    for (id, name) in parts.lines().filter_map(mms_part) {
        let file = dir.join(safe_name(&format!("{id}_{name}")));
        // A part never changes once saved, so only new ones are read.
        if fs::metadata(&file).is_ok_and(|m| m.len() > 0) {
            continue;
        }
        let _ = fs::create_dir_all(dir);
        let mut c = adb();
        c.args([
            "-s",
            serial,
            "exec-out",
            &format!("content read --uri content://mms/part/{id}"),
        ]);
        let bytes = run(c);
        // Stop or an unplug cuts the read short; a short file would be kept for good.
        if interrupted(serial).is_some() {
            return;
        }
        if !bytes.is_empty() {
            let _ = fs::write(file, bytes);
        }
    }
}

/// Reads `Row: 2 _id=3, ct=image/jpeg, cl=pic.jpg` into (3, "pic.jpg"). Message text and
/// layout parts are skipped: their text is already in mms.txt.
pub(super) fn mms_part(row: &str) -> Option<(u64, &str)> {
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

/// Windows ignores letter case, and safe_name can turn two names into one, so IMG.jpg and
/// img.jpg would overwrite each other on every run. Later ones get " (2)", " (3)"...
pub(super) fn unclash(jobs: &mut [Job]) {
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
pub(super) fn vcard(rows: &str) -> String {
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

pub(super) fn vcard_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

/// Wraps a path in single quotes for the phone's shell.
pub(super) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Runs `<cmd> 'path' 'path' ...` on the phone in batches that fit one command line, to the
/// end even after Stop (cleanup must finish), and returns everything it printed.
pub(crate) fn shell_each(serial: &str, cmd: &str, paths: &[&str]) -> String {
    let mut out = String::new();
    for batch in paths.chunks(100) {
        let line = std::iter::once(cmd.to_string())
            .chain(batch.iter().map(|p| sh_quote(p)))
            .collect::<Vec<_>>()
            .join(" ");
        if let Ok(o) = logged(adb().args(["-s", serial, "shell", &line])) {
            out += &String::from_utf8_lossy(&o.stdout);
        }
    }
    out
}

/// True when the local copy is missing or differs in size or time.
pub(super) fn needs_copy(job: &Job) -> bool {
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
pub(super) fn pull(serial: &str, jobs: Vec<Job>, root: &Path, report: &dyn Fn(Msg)) -> Vec<String> {
    let total = jobs.len();
    let mut done = 0;
    let mut errors = Vec::new();
    let mut by_dir: BTreeMap<PathBuf, Vec<Job>> = BTreeMap::new();
    let mut single = Vec::new();
    for job in jobs {
        // adb pull into a folder keeps the phone's file name, so renamed files go one by one.
        let same_name =
            job.local.file_name().and_then(|n| n.to_str()) == job.remote.rsplit('/').next();
        // A file that is already there goes one by one through a temp file: pulling over it
        // would leave it cut short when Stop or an unplug ends the pull.
        // shortcut: changed files copy one by one, slow only when thousands changed at once.
        let fresh = same_name && !too_long(&job.local) && !job.local.exists();
        match (fresh, job.local.parent()) {
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
        if too_long(&job.local) || job.local.exists() {
            // adb.exe cannot write past 260 characters; Rust's fs can, so pull short and move.
            let short = tmp.join("file");
            let _ = fs::create_dir_all(&tmp);
            let _ = fs::remove_file(&short);
            let mut c = adb();
            c.args(["-s", serial, "pull", "-a", &job.remote])
                .arg(&short);
            run(c);
            // Only a whole file replaces the one in the backup.
            if fs::metadata(&short).is_ok_and(|m| m.len() == job.size) {
                let _ = fs::rename(&short, &job.local);
            }
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
pub(super) fn too_long(path: &Path) -> bool {
    path.as_os_str().len() > 240
}

pub(super) fn record_failures(jobs: &[Job], errors: &mut Vec<String>) {
    errors.extend(
        jobs.iter()
            .filter(|j| needs_copy(j))
            .map(|j| format!("{} -> {}", j.remote, j.local.display())),
    );
}

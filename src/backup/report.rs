use super::*;

/// Rewritten by every backup; also tells the Restore tab when the last backup ran.
pub const REPORT: &str = "report.txt";
/// Written by every restore, next to the backup's own report.
pub const RESTORE_REPORT: &str = "restore-report.txt";

/// Saves a plain-text report the user can open, keep or share.
pub fn save_report(path: &Path, title: &str, body: &str) -> std::io::Result<()> {
    fs::write(
        path,
        format!(
            "Phone Backup {}\n{title}, {}\n\n{body}\n",
            env!("CARGO_PKG_VERSION"),
            now_text()
        ),
    )
}

/// Saves a page of what the phone says about itself, to print or show a buyer. The serial number
/// and IMEI stay out, since this page is meant to be handed over.
pub fn phone_report(serial: &str, path: &Path) -> Result<(), String> {
    // Not stoppable like `shell`, for the same reason as phone_id.
    let ask = |cmd: &str| {
        logged(adb().args(["-s", serial, "shell", cmd]))
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let body = phone_report_body(
        &ask("getprop"),
        &ask("dumpsys battery"),
        &ask("df -k /data"),
    );
    if body.is_empty() {
        return Err(
            "The phone did not answer. Check that it is still connected, then try again.".into(),
        );
    }
    save_report(path, "Phone report", &body)
        .map_err(|e| format!("Cannot save the report: {e}. Choose another folder, then try again."))
}

/// The report's lines from `getprop`, `dumpsys battery` and `df -k`. A value the phone does not
/// give is left out, never guessed.
pub(super) fn phone_report_body(getprop: &str, battery: &str, df: &str) -> String {
    let prop = |key: &str| {
        getprop.lines().find_map(|l| {
            let value = l.strip_prefix(&format!("[{key}]: ["))?.strip_suffix(']')?;
            (!value.is_empty()).then(|| value.to_string())
        })
    };
    let reading = |key: &str| {
        battery
            .lines()
            .find_map(|l| l.trim().strip_prefix(key)?.strip_prefix(": "))
    };
    // The last line ends in total, used, free, percent, mount; a long disk name can take its own
    // line before it.
    let storage = df.lines().last().and_then(|l| {
        let words: Vec<&str> = l.split_whitespace().collect();
        let kb = |word: &str| word.parse::<u64>().ok().map(|n| size(n * 1024));
        let at = words.len().checked_sub(4)?;
        Some(format!(
            "{} used, {} free",
            kb(words[at])?,
            kb(words[at + 1])?
        ))
    });
    // The numbers are Android's BatteryManager.BATTERY_HEALTH_* values; 1 means unknown.
    let condition = reading("health").and_then(|h| match h {
        "2" => Some("Good"),
        "3" => Some("Overheating"),
        "4" => Some("Dead"),
        "5" => Some("Over voltage"),
        "6" => Some("Faulty"),
        "7" => Some("Too cold"),
        _ => None,
    });
    let lines = [
        ("Model", prop("ro.product.model")),
        ("Maker", prop("ro.product.manufacturer")),
        ("Android version", prop("ro.build.version.release")),
        ("Security patch", prop("ro.build.version.security_patch")),
        ("Storage", storage),
        ("Battery level", reading("level").map(|l| format!("{l}%"))),
        (
            "Battery condition, as the phone reports it",
            condition.map(|c| format!("{c}. This does not measure how worn the battery is.")),
        ),
    ];
    lines
        .into_iter()
        .filter_map(|(what, value)| Some(format!("{what}: {}", value?)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The backup's report: what it holds, every file copied this time and every file that failed.
pub(super) fn write_report(s: &Summary, copied: &[PathBuf]) {
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
    let _ = save_report(
        &s.root.join(REPORT),
        &format!("Backup of {}", s.name),
        &body,
    );
}

/// The local date and time as "2026-10-31 14:05".
#[cfg(windows)]
pub(super) fn now_text() -> String {
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
pub(super) fn now_text() -> String {
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

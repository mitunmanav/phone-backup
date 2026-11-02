use super::*;

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
    save_report(
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

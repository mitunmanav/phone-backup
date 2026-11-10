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
    for note in &s.notes {
        body += &format!("{note}\n");
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

/// Notes: pitch in Hz, then start and length in seconds.
pub type Tune = &'static [(f32, f32, f32)];

/// When a job ends with a problem, whatever tune is picked: a soft step down, so it never
/// sounds like success.
pub const PROBLEM: Tune = &[(523.3, 0.0, 0.3), (392.0, 0.16, 0.8)];

/// The tunes for the end of a job.
pub const SOUNDS: [(&str, Tune); 6] = [
    (
        "Chime",
        &[(523.3, 0.0, 0.5), (659.3, 0.12, 0.5), (784.0, 0.24, 0.9)],
    ),
    ("Bell", &[(880.0, 0.0, 1.6)]),
    ("Two notes", &[(784.0, 0.0, 0.35), (1046.5, 0.18, 0.9)]),
    (
        "Marimba",
        &[
            (523.3, 0.0, 0.22),
            (587.3, 0.1, 0.22),
            (784.0, 0.2, 0.22),
            (1046.5, 0.3, 0.6),
        ],
    ),
    (
        "Soft rise",
        &[
            (349.2, 0.0, 0.8),
            (440.0, 0.22, 0.8),
            (523.3, 0.44, 0.8),
            (698.5, 0.66, 1.2),
        ],
    ),
    (
        "Little tune",
        &[
            (659.3, 0.0, 0.25),
            (784.0, 0.14, 0.25),
            (880.0, 0.28, 0.25),
            (784.0, 0.42, 0.25),
            (1046.5, 0.56, 0.9),
        ],
    ),
];

/// A 16-bit mono WAV in memory, ready for Windows to play.
#[cfg(any(windows, test))]
fn wav(notes: &[(f32, f32, f32)]) -> Vec<u8> {
    const RATE: usize = 22050;
    let end = notes.iter().map(|n| n.1 + n.2).fold(0.0, f32::max);
    let mut mixed = vec![0.0f32; (end * RATE as f32) as usize + 1];
    for (pitch, start, len) in notes {
        let first = (start * RATE as f32) as usize;
        for k in 0..(len * RATE as f32) as usize {
            let at = k as f32 / RATE as f32;
            // Quick attack, long fade, a quiet octave on top: a struck sound, not a beep.
            let shape = (at / 0.006).min(1.0) * (-4.5 * at / len).exp();
            let phase = std::f32::consts::TAU * pitch * at;
            mixed[first + k] += shape * (phase.sin() + 0.22 * (phase * 2.0).sin());
        }
    }
    let data = (mixed.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for v in mixed {
        out.extend_from_slice(&(((v * 0.35).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Plays on its own thread so the window never waits; a new sound cuts off the last one.
pub fn play_sound(tune: Tune) {
    #[cfg(windows)]
    std::thread::spawn(move || {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT};
        let bytes = wav(tune);
        // SAFETY: without SND_ASYNC the call returns only when the sound ends, so `bytes` outlives it.
        unsafe {
            PlaySoundW(
                bytes.as_ptr().cast(),
                std::ptr::null_mut(),
                SND_MEMORY | SND_NODEFAULT,
            )
        };
    });
    #[cfg(not(windows))]
    let _ = tune;
}

/// Facts that help someone fix a problem: app, Windows and adb versions and the phones adb sees.
pub fn diagnostics(dest: &Path) -> String {
    let out = |c: &mut Command| {
        c.output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|e| e.to_string())
    };
    let windows = if cfg!(windows) {
        out(windows_tool("cmd.exe").args(["/c", "ver"]))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_are_valid_wav() {
        for notes in SOUNDS.iter().map(|s| s.1).chain([PROBLEM]) {
            let w = wav(notes);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(
                u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize,
                w.len() - 8
            );
            assert_eq!(
                u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize,
                w.len() - 44
            );
        }
    }
}

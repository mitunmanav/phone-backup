use super::*;

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
pub(super) fn recycle(path: &Path) -> Result<(), String> {
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
pub(super) fn recycle(_: &Path) -> Result<(), String> {
    Err("Deleting backups works on Windows only.".into())
}

/// True when both files hold the same bytes.
pub(super) fn same_file(a: &Path, b: &Path) -> bool {
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

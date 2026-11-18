use super::*;

#[tokio::main(flavor = "current_thread")]
pub async fn mtp_backup(dest: &Path, pick: &Pick, report: &dyn Fn(Msg)) -> Result<Summary, String> {
    use mtp_rs::mtp::{ByteRange, MtpDevice};
    use std::io::Write;

    // The phone side failed, so the advice is about the phone.
    let phone = |e: mtp_rs::Error| {
        format!("{e}. Check the cable and that the phone is unlocked, then press Try again.")
    };
    let device = MtpDevice::open_first().await.map_err(phone)?;
    let info = device.device_info();
    let name = info.model.clone();
    let root = dest.join(safe_name(&format!("{}_{}", info.model, info.serial_number)));
    let (mut files, mut copied, mut errors) = (0, Vec::new(), Vec::new());
    let mut kept = Vec::new();
    // Windows ignores letter case, so IMG.jpg and img.jpg would overwrite each other.
    let mut seen = HashSet::new();

    for (n, storage) in device.storages().await.map_err(phone)?.iter().enumerate() {
        let desc = &storage.info().description;
        // First storage is the phone's internal storage: same folder an ADB backup uses.
        let base = if n == 0 {
            root.join("files")
        } else {
            // Two cards with the same name would share one folder and skip each other's files.
            unclashed(
                &mut seen,
                &root.join(format!("storage_{}", safe_name(desc))),
            )
        };
        // The folder's path inside the storage goes along, for the folder and type choices.
        let mut stack = vec![(None, base, String::new())];
        while let Some((parent, dir, rel)) = stack.pop() {
            if hold() {
                // Copied files may replace older ones whose sums no longer match.
                let _ = update_sums(&root, &copied, &[], report);
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
                // Sorted, so the same file gets the same " (2)" name on every run.
                Ok(mut o) => {
                    o.sort_by(|a, b| a.filename.cmp(&b.filename));
                    o
                }
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
                let local = unclashed(&mut seen, &local);
                if is_junk(&format!("/sdcard/{path}")) || !pick.wants(&path, obj.size) {
                    continue;
                }
                files += 1;
                if let Ok(metadata) = fs::metadata(&local) {
                    let backup_size = metadata.len();
                    let (phone_time, backup_time) = if backup_size == obj.size {
                        let phone_time = local_phone_time_seconds(obj.modified.as_ref());
                        let backup_time = metadata
                            .modified()
                            .ok()
                            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                            .and_then(|time| i64::try_from(time.as_secs()).ok());
                        (phone_time, backup_time)
                    } else {
                        (None, None)
                    };
                    if !should_copy(obj.size, backup_size, phone_time, backup_time) {
                        kept.push(local);
                        continue;
                    }
                }
                // Pause waits between files, never inside a download.
                if hold() {
                    let _ = update_sums(&root, &copied, &[], report);
                    return Err(STOPPED.into());
                }
                // The whole name plus a suffix, so it never matches another file's name.
                let mut part = local.clone().into_os_string();
                part.push(".part-download");
                let part = PathBuf::from(part);
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
    let notes = match update_sums(&root, &copied, &kept, report) {
        Ok(()) => Vec::new(),
        Err(e) => vec![super::adb::sums_lost(&e)],
    };
    let s = Summary {
        name,
        root,
        files,
        copied: copied.len(),
        errors,
        data: Vec::new(),
        notes,
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

fn should_copy(
    phone_size: u64,
    backup_size: u64,
    phone_time: Option<i64>,
    backup_time: Option<i64>,
) -> bool {
    phone_size != backup_size
        || matches!((phone_time, backup_time), (Some(phone), Some(backup)) if phone > backup)
}

fn phone_time_seconds(
    modified: Option<&mtp_rs::mtp::DateTime>,
    offset: mtp_rs::mtp::UtcOffset,
) -> Option<i64> {
    modified?.to_unix_seconds_with_fallback(offset)
}

fn local_phone_time_seconds(modified: Option<&mtp_rs::mtp::DateTime>) -> Option<i64> {
    use mtp_rs::mtp::UtcOffset;

    let utc_seconds = phone_time_seconds(modified, UtcOffset::UTC)?;
    let offset = pc_utc_offset_minutes_at(utc_seconds)?;
    phone_time_seconds(modified, UtcOffset::from_minutes(offset)?)
}

#[cfg(windows)]
fn pc_utc_offset_minutes_at(utc_seconds: i64) -> Option<i16> {
    use mtp_rs::mtp::{DateTime, UtcOffset};
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

    let ticks = u64::try_from(
        utc_seconds
            .checked_add(11_644_473_600)?
            .checked_mul(10_000_000)?,
    )
    .ok()?;
    let file_time = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: both calls only read and fill the structs they are given; null means the current time zone.
    let local = unsafe {
        let mut utc = std::mem::zeroed();
        if FileTimeToSystemTime(&file_time, &mut utc) == 0 {
            return None;
        }
        let mut local = utc;
        if SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0 {
            return None;
        }
        local
    };
    let local_as_utc = DateTime::new(
        local.wYear,
        u8::try_from(local.wMonth).ok()?,
        u8::try_from(local.wDay).ok()?,
        u8::try_from(local.wHour).ok()?,
        u8::try_from(local.wMinute).ok()?,
        u8::try_from(local.wSecond).ok()?,
    )?
    .to_unix_seconds_with_fallback(UtcOffset::UTC)?;
    let offset_seconds = local_as_utc.checked_sub(utc_seconds)?;
    i16::try_from(offset_seconds / 60).ok()
}

#[cfg(not(windows))]
fn pc_utc_offset_minutes_at(_: i64) -> Option<i16> {
    None
}

#[cfg(test)]
mod tests {
    use super::{phone_time_seconds, should_copy};
    use mtp_rs::mtp::{DateTime, UtcOffset};

    #[cfg(windows)]
    use super::pc_utc_offset_minutes_at;

    #[test]
    fn same_size_older_phone_time_is_skipped() {
        assert!(!should_copy(10, 10, Some(100), Some(101)));
    }

    #[test]
    fn same_size_equal_phone_time_is_skipped() {
        assert!(!should_copy(10, 10, Some(100), Some(100)));
    }

    #[test]
    fn same_size_newer_phone_time_is_copied() {
        assert!(should_copy(10, 10, Some(101), Some(100)));
    }

    #[test]
    fn different_sizes_are_copied_with_or_without_times() {
        assert!(should_copy(11, 10, Some(100), Some(200)));
        assert!(should_copy(11, 10, None, None));
    }

    #[test]
    fn missing_phone_time_uses_size_only() {
        assert!(!should_copy(10, 10, None, Some(100)));
        assert!(should_copy(11, 10, None, Some(100)));
    }

    #[test]
    fn missing_backup_time_uses_size_only() {
        assert!(!should_copy(10, 10, Some(200), None));
        assert!(should_copy(11, 10, Some(100), None));
    }

    #[test]
    fn zoneless_phone_time_uses_the_supplied_offset() {
        let phone_time = DateTime::new(2026, 10, 7, 14, 30, 0).unwrap();
        assert_eq!(
            phone_time_seconds(Some(&phone_time), UtcOffset::from_minutes(120).unwrap()),
            Some(1_791_376_200)
        );
    }

    #[cfg(windows)]
    #[test]
    fn pc_offset_for_fixed_date_is_within_fourteen_hours() {
        let minutes = pc_utc_offset_minutes_at(1_791_376_200).unwrap();
        assert!((-14 * 60..=14 * 60).contains(&minutes));
    }
}

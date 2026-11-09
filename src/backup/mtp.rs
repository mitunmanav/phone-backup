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
                // shortcut: MTP compares size only (no file times); a same-size edit is missed.
                if fs::metadata(&local).is_ok_and(|m| m.len() == obj.size) {
                    kept.push(local);
                    continue;
                }
                // Pause waits between files, never inside a download.
                if hold() {
                    update_sums(&root, &copied, &[], report);
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
    update_sums(&root, &copied, &kept, report);
    let s = Summary {
        name,
        root,
        files,
        copied: copied.len(),
        errors,
        data: Vec::new(),
        notes: Vec::new(),
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

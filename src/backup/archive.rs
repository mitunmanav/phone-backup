//! A backup as one `.zip` file, to move it to another drive or hand it to someone, and back.
//! The archive holds the backup's files as ordinary files (sealed ones opened), with a fresh
//! `checksums.sha256` for them, so Windows or 7-Zip open it without this app. With a password
//! each file is encrypted with WinZip AES-256, which 7-Zip reads. Photos, videos and other
//! files that are already compressed are stored as they are; text is compressed.

use super::*;

/// Says what the archive was made from, so opening it can protect the backup again.
const MARK: &str = "phone-backup-archive.json";

/// Already compressed: zipping them again only costs time.
const STORED: [&str; 24] = [
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif", "mp4", "mov", "3gp", "mkv",
    "webm", "avi", "mp3", "m4a", "aac", "ogg", "opus", "flac", "apk", "zip", "7z", "rar",
];

/// A writer that hashes what goes through it.
struct Hashing<W: std::io::Write> {
    w: W,
    h: sha2::Sha256,
}

impl<W: std::io::Write> std::io::Write for Hashing<W> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let n = self.w.write(b)?;
        sha2::Digest::update(&mut self.h, &b[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.w.flush()
    }
}

/// The files that go into an archive: not exports, side files, temporary folders or the
/// backup's own checksums and password file, which do not describe the archive's files.
fn archive_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let top = dir == root;
            if name.starts_with('.')
                || name.ends_with(".part")
                || (top && [crate::export::EXPORTS, CHECKSUMS, LOCK].contains(&name.as_str()))
            {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Unix seconds as a zip time (local time is not known here, so UTC).
fn zip_time(secs: i64) -> zip::DateTime {
    let s = stamp(secs);
    let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse().ok()).unwrap_or(0);
    zip::DateTime::from_date_and_time(
        num(0, 4),
        num(5, 7) as u8,
        num(8, 10) as u8,
        num(11, 13) as u8,
        num(13, 15) as u8,
        num(15, 17) as u8,
    )
    .unwrap_or_default()
}

/// Writes backup `root` into the zip file `to`, encrypted with `password` when given. A
/// backup with a password must be given one. `stop` ends it early; then no file is left.
pub fn save_archive(
    root: &Path,
    to: &Path,
    password: Option<&str>,
    say: &dyn Fn(String),
    stop: &dyn Fn() -> bool,
) -> Result<String, String> {
    let lock = lock_of(root)?;
    if lock.is_some() && password.is_none_or(str::is_empty) {
        return Err("This backup has a password, so its archive needs one too.".into());
    }
    let key = match &lock {
        Some(_) => Some(key(root).ok_or(LOCKED)?),
        None => None,
    };
    let list = archive_files(root);
    let mut part = to.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let made = (|| -> Result<u64, String> {
        let file =
            fs::File::create(&part).map_err(|e| format!("Cannot create {}: {e}", to.display()))?;
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let mut sums = String::new();
        let mut total = 0;
        for (n, f) in list.iter().enumerate() {
            if stop() {
                return Err("You stopped it. No archive was saved.".into());
            }
            let plain = plain_path(f);
            let name = rel(root, &plain);
            say(format!(
                "Adding {} of {}: {name}",
                count(n + 1),
                files(list.len())
            ));
            let meta = fs::metadata(f).map_err(|e| format!("Cannot read {name}: {e}"))?;
            let size = plain_size(f, meta.len()).unwrap_or(meta.len());
            let ext = plain
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let method = if STORED.contains(&ext.as_str()) {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            };
            let when = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            let mut opts = zip::write::SimpleFileOptions::default()
                .compression_method(method)
                .last_modified_time(zip_time(when))
                .large_file(size >= u32::MAX as u64 / 2);
            if let Some(pw) = password {
                opts = opts.with_aes_encryption(zip::AesMode::Aes256, pw);
            }
            zip.start_file(name.as_str(), opts)
                .map_err(|e| format!("Cannot add {name}: {e}"))?;
            let mut w = Hashing {
                w: &mut zip,
                h: sha2::Digest::new(),
            };
            let mut r = std::io::BufReader::new(
                fs::File::open(f).map_err(|e| format!("Cannot read {name}: {e}"))?,
            );
            match (is_sealed(f), &key) {
                (true, Some(k)) => crypt::decrypt(k, &mut r, &mut w).map(|_| ()),
                (true, None) => Err(std::io::Error::other(LOCKED)),
                (false, _) => std::io::copy(&mut r, &mut w).map(|_| ()),
            }
            .map_err(|e| format!("Cannot add {name}: {e}"))?;
            let sum: String = sha2::Digest::finalize(w.h)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            sums += &format!("{sum}  {name}\n");
            total += size;
        }
        let mut opts = zip::write::SimpleFileOptions::default();
        if let Some(pw) = password {
            opts = opts.with_aes_encryption(zip::AesMode::Aes256, pw);
        }
        let mark = serde_json::json!({
            "version": 1,
            "scope": lock.as_ref().map(|l| match l.scope {
                Scope::Data => "data",
                Scope::All => "all",
            }),
        });
        for (name, text) in [(CHECKSUMS, sums), (MARK, mark.to_string())] {
            zip.start_file(name, opts)
                .and_then(|()| {
                    std::io::Write::write_all(&mut zip, text.as_bytes()).map_err(Into::into)
                })
                .map_err(|e| format!("Cannot add {name}: {e}"))?;
        }
        let w = zip
            .finish()
            .map_err(|e| format!("Cannot finish the archive: {e}"))?;
        w.into_inner()
            .map_err(|e| format!("Cannot finish the archive: {e}"))?
            .sync_all()
            .map_err(|e| format!("Cannot finish the archive: {e}"))?;
        fs::rename(&part, to).map_err(|e| format!("Cannot save {}: {e}", to.display()))?;
        Ok(total)
    })();
    match made {
        Ok(total) => {
            let mut said = format!(
                "Saved {} ({}) as {}.",
                files(list.len()),
                size(total),
                to.display()
            );
            said += if password.is_some() {
                " Windows cannot open an archive with a password; 7-Zip can, or open it here."
            } else {
                " Anyone with the file can open it."
            };
            Ok(said)
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

/// True when the zip file holds files with a password.
pub fn archive_has_password(zip_file: &Path) -> Result<bool, String> {
    let mut a = open_zip(zip_file)?;
    for i in 0..a.len() {
        if let Ok(f) = a.by_index_raw(i)
            && f.encrypted()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn open_zip(zip_file: &Path) -> Result<zip::ZipArchive<std::io::BufReader<fs::File>>, String> {
    let f =
        fs::File::open(zip_file).map_err(|e| format!("Cannot open {}: {e}", zip_file.display()))?;
    zip::ZipArchive::new(std::io::BufReader::new(f))
        .map_err(|_| "This file is not a zip archive, or it is damaged.".to_string())
}

/// What `open_archive` made: the new backup, what it says, and its recovery key when it was
/// protected again with the archive's password.
pub struct Opened {
    pub folder: PathBuf,
    pub said: String,
    pub recovery: Option<String>,
}

/// Unpacks a Phone Backup zip into a new folder in `dest`, named after the file. A backup that
/// had a password gets the archive's password again, protecting as much as before.
pub fn open_archive(
    zip_file: &Path,
    dest: &Path,
    password: Option<&str>,
    say: &dyn Fn(String),
    stop: &dyn Fn() -> bool,
) -> Result<Opened, String> {
    let mut a = open_zip(zip_file)?;
    let names: Vec<String> = a
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .collect();
    let ours = names.iter().any(|n| {
        n == CHECKSUMS
            || n == MARK
            || ["files/", "apps/", "data/", "sdcard_"]
                .iter()
                .any(|p| n.starts_with(p))
    });
    if !ours {
        return Err("This archive does not hold a phone backup.".into());
    }
    let stem = safe_name(&zip_file.file_stem().unwrap_or_default().to_string_lossy());
    let mut folder = dest.join(&stem);
    let mut k = 1;
    while folder.exists() {
        k += 1;
        folder = dest.join(format!("{stem} ({k})"));
    }
    let pw = password.unwrap_or_default().as_bytes();
    let mut mark = None;
    let unpacked = (|| -> Result<usize, String> {
        let mut n = 0;
        for i in 0..a.len() {
            if stop() {
                return Err("You stopped it. The part unpacked so far was removed.".into());
            }
            let encrypted = a.by_index_raw(i).map(|f| f.encrypted()).unwrap_or(false);
            let mut f = if encrypted {
                if pw.is_empty() {
                    return Err("This archive has a password. Type it to open it.".into());
                }
                a.by_index_decrypt(i, pw)
            } else {
                a.by_index(i)
            }
            .map_err(|_| "That password does not open this archive.".to_string())?;
            // Names that climb out of the folder are never written.
            let Some(name) = f.enclosed_name() else {
                continue;
            };
            if f.is_dir() {
                continue;
            }
            let name_s = name.to_string_lossy().replace('\\', "/");
            if name_s == MARK {
                let mut text = String::new();
                let _ = std::io::Read::read_to_string(&mut f, &mut text);
                mark = serde_json::from_str::<serde_json::Value>(&text).ok();
                continue;
            }
            say(format!(
                "Unpacking {} of {}: {name_s}",
                count(i + 1),
                count(names.len())
            ));
            let to = folder.join(&name);
            if let Some(dir) = to.parent() {
                fs::create_dir_all(dir)
                    .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
            }
            let mut part = to.as_os_str().to_owned();
            part.push(".part");
            let part = PathBuf::from(part);
            let when = f.last_modified();
            let written = (|| -> std::io::Result<()> {
                let mut out = std::io::BufWriter::new(fs::File::create(&part)?);
                // AES archives are checked at the end of each file, so a wrong byte fails here.
                std::io::copy(&mut f, &mut out)?;
                let out = out.into_inner().map_err(|e| e.into_error())?;
                if let Some(t) = when.and_then(|t| {
                    let s = format!(
                        "{:04}-{:02}-{:02}_{:02}{:02}{:02}",
                        t.year(),
                        t.month(),
                        t.day(),
                        t.hour(),
                        t.minute(),
                        t.second()
                    );
                    secs_of(&s)
                }) {
                    let _ = out.set_modified(UNIX_EPOCH + Duration::from_secs(t));
                }
                drop(out);
                fs::rename(&part, &to)
            })();
            if let Err(e) = written {
                let _ = fs::remove_file(&part);
                return Err(if encrypted {
                    "That password does not open this archive, or it is damaged.".into()
                } else {
                    format!("Cannot unpack {name_s}: {e}")
                });
            }
            n += 1;
        }
        Ok(n)
    })();
    let n = match unpacked {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_dir_all(&folder);
            return Err(e);
        }
    };
    // The archive's files are plain; the lists still name sealed copies.
    super::protect::relist(&folder);
    let mut said = format!("Unpacked {} into {}.", files(n), folder.display());
    let scope = match mark.as_ref().and_then(|m| m["scope"].as_str()) {
        Some("data") => Some(Scope::Data),
        Some("all") => Some(Scope::All),
        _ => None,
    };
    let mut recovery = None;
    if let (Some(scope), Some(pw)) = (scope, password.filter(|p| !p.is_empty())) {
        set_password(&folder, pw, scope)?;
        super::protect::protect(&folder, &|_| {})?;
        recovery = make_recovery(&folder).ok();
        said += " It has the archive's password, as the backup it came from had one.";
    }
    Ok(Opened {
        folder,
        said,
        recovery,
    })
}

/// "2026-11-10_143205" (UTC) as Unix seconds.
fn secs_of(s: &str) -> Option<u64> {
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, se) = (num(11, 13)?, num(13, 15)?, num(15, 17)?);
    // Howard Hinnant's days-from-civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + se).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pb-zip-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn backup(root: &Path) {
        fs::create_dir_all(root.join("files/DCIM")).unwrap();
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("files/DCIM/a.jpg"), "photo").unwrap();
        fs::write(root.join("data/sms.txt"), "Row: 0 body=hello\n").unwrap();
        fs::write(root.join(REPORT), "report").unwrap();
        let all = [root.join("files/DCIM/a.jpg"), root.join("data/sms.txt")];
        update_sums(root, &all, &all, &|_| {}).unwrap();
    }

    #[test]
    fn times_round_trip() {
        assert_eq!(secs_of(&stamp(1_794_407_525)), Some(1_794_407_525));
        assert_eq!(secs_of("1970-01-01_000000"), Some(0));
        let t = zip_time(1_794_407_525);
        assert_eq!((t.year(), t.month(), t.day(), t.hour()), (2026, 11, 11, 14));
    }

    #[test]
    fn plain_archive_round_trips_into_a_new_backup() {
        let (root, out) = (temp("plain-src"), temp("plain-out"));
        backup(&root);
        let zip = out.join("Pixel.zip");
        let said = save_archive(&root, &zip, None, &|_| {}, &|| false).unwrap();
        assert!(said.starts_with("Saved 3 files"), "{said}");
        assert!(!archive_has_password(&zip).unwrap());
        let opened = open_archive(&zip, &out, None, &|_| {}, &|| false).unwrap();
        assert_eq!(opened.folder, out.join("Pixel"));
        assert!(opened.recovery.is_none());
        assert_eq!(
            fs::read_to_string(out.join("Pixel/files/DCIM/a.jpg")).unwrap(),
            "photo"
        );
        assert!(verify(&opened.folder, &|_| {}).is_ok());
        assert!(!out.join("Pixel").join(MARK).exists());
        // A second copy gets its own folder.
        let again = open_archive(&zip, &out, None, &|_| {}, &|| false).unwrap();
        assert_eq!(again.folder, out.join("Pixel (2)"));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn protected_backup_archives_with_a_password_and_comes_back_protected() {
        let (root, out) = (temp("sealed-src"), temp("sealed-out"));
        backup(&root);
        set_password(&root, "backup pw", Scope::Data).unwrap();
        super::super::protect::protect(&root, &|_| {}).unwrap();
        let zip = out.join("Locked.zip");
        assert!(save_archive(&root, &zip, None, &|_| {}, &|| false).is_err());
        assert!(!zip.exists());
        // Stopping leaves no file.
        assert!(save_archive(&root, &zip, Some("zip pw"), &|_| {}, &|| true).is_err());
        assert!(!zip.exists() && !out.join("Locked.zip.part").exists());
        save_archive(&root, &zip, Some("zip pw"), &|_| {}, &|| false).unwrap();
        assert!(archive_has_password(&zip).unwrap());

        // Inside, the files are plain and encrypted by the zip's own AES.
        let mut a = open_zip(&zip).unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(
            &mut a.by_name_decrypt("data/sms.txt", b"zip pw").unwrap(),
            &mut text,
        )
        .unwrap();
        assert_eq!(text, "Row: 0 body=hello\n");
        assert!(a.by_name_decrypt("data/sms.txt", b"wrong").is_err());

        assert!(open_archive(&zip, &out, None, &|_| {}, &|| false).is_err());
        assert!(open_archive(&zip, &out, Some("wrong"), &|_| {}, &|| false).is_err());
        assert!(!out.join("Locked").exists());
        let opened = open_archive(&zip, &out, Some("zip pw"), &|_| {}, &|| false).unwrap();
        assert!(opened.recovery.is_some());
        assert!(opened.folder.join("data/sms.txt.pbenc").exists());
        assert_eq!(
            read_text(&opened.folder.join("data/sms.txt")).unwrap(),
            "Row: 0 body=hello\n"
        );
        assert!(verify(&opened.folder, &|_| {}).is_ok());
        forget(&opened.folder);
        unlock(&opened.folder, "zip pw").unwrap();
        forget(&opened.folder);
        forget(&root);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn other_zips_are_refused() {
        let out = temp("other");
        let zip = out.join("photos.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&zip).unwrap());
        w.start_file("holiday.jpg", zip::write::SimpleFileOptions::default())
            .unwrap();
        w.finish().unwrap();
        let err = open_archive(&zip, &out, None, &|_| {}, &|| false)
            .err()
            .unwrap();
        assert_eq!(err, "This archive does not hold a phone backup.");
        let not_zip = out.join("x.zip");
        fs::write(&not_zip, "no").unwrap();
        assert!(open_archive(&not_zip, &out, None, &|_| {}, &|| false).is_err());
        let _ = fs::remove_dir_all(&out);
    }
}

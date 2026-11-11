//! The Tools page's jobs on one backup: duplicate files, HEIC photos to JPG, merging duplicate
//! contacts, and deleting files from the phone that are safely in the backup.

use crate::backup::{self, files, size};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the Tools page's Stop button.
pub static STOP: AtomicBool = AtomicBool::new(false);

/// Groups of identical files, each sorted by path, biggest waste first.
pub type Groups = Vec<Vec<(PathBuf, u64)>>;

/// A file to free on the phone: its phone path, size, and SHA-256 in the backup ("" when the
/// backup has none, which never matches).
pub type OnPhone = (String, u64, String);

/// What a tool sends back to the window.
pub enum Done {
    /// A line saying how far it is.
    Status(String),
    Dupes(Result<Groups, String>),
    /// The phone checked, its files whose backup copy matches, and how many chosen files did not.
    Matched(String, Result<(Vec<OnPhone>, usize), String>),
    Text(Result<String, String>),
    /// A backup read for the Read page.
    Book(Result<crate::export::Book, String>),
    /// An archive unpacked into a new backup.
    Opened(Result<backup::Opened, String>),
}

/// Identical files in the backup's photos and files: same size first, then the same SHA-256.
pub fn duplicates(folder: &Path, say: &dyn Fn(String)) -> Result<Vec<Vec<(PathBuf, u64)>>, String> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (path, n, _) in backup::backup_files(folder, None)? {
        if n > 0 {
            by_size.entry(n).or_default().push(path);
        }
    }
    let same: Vec<(u64, Vec<PathBuf>)> = by_size.into_iter().filter(|(_, p)| p.len() > 1).collect();
    let total: usize = same.iter().map(|(_, p)| p.len()).sum();
    let (mut groups, mut done) = (Vec::new(), 0);
    for (n, paths) in same {
        let mut by_hash: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if STOP.load(Ordering::Relaxed) {
                return Err("You stopped the search. Nothing was changed.".into());
            }
            say(format!(
                "Comparing {} of {}…",
                backup::count(done),
                files(total)
            ));
            done += 1;
            if let Ok(h) = backup::plain_sha256(&path) {
                by_hash.entry(h).or_default().push(path);
            }
        }
        groups.extend(by_hash.into_values().filter(|g| g.len() > 1).map(|mut g| {
            g.sort();
            g.into_iter().map(|p| (p, n)).collect::<Vec<_>>()
        }));
    }
    let waste = |g: &Vec<(PathBuf, u64)>| g[0].1 * (g.len() as u64 - 1);
    groups.sort_by(|a, b| waste(b).cmp(&waste(a)).then_with(|| a[0].0.cmp(&b[0].0)));
    Ok(groups)
}

/// Deletes the ticked copies from the backup on this computer, and their checksums, so
/// Verify does not call them missing.
pub fn delete_copies(folder: &Path, paths: &[(PathBuf, u64)]) -> Result<String, String> {
    let (mut gone, mut bytes, mut failed) = (0, 0, 0);
    for (path, n) in paths {
        match fs::remove_file(path) {
            Ok(()) => {
                gone += 1;
                bytes += n;
            }
            Err(_) => failed += 1,
        }
    }
    // Drops the sums of files no longer in the folder; hashes nothing.
    let sums = backup::update_sums(folder, &[], &[], &|_| {});
    let mut msg = format!(
        "Deleted {} from the backup, freeing {}.",
        copies(gone),
        size(bytes)
    );
    if failed > 0 {
        msg += &format!(
            " {} could not be deleted; close any app that has them open.",
            copies(failed)
        );
    }
    if let Err(e) = sums {
        msg += &format!(
            " The checksum file could not be updated ({e}), so Verify may call the deleted copies missing."
        );
    }
    Ok(msg)
}

fn copies(n: usize) -> String {
    format!(
        "{} {}",
        backup::count(n),
        if n == 1 { "copy" } else { "copies" }
    )
}

/// Saves a JPG of every HEIC photo in the backup under its `jpg` folder, keeping the photo's
/// date and location (its Exif) and the file's time. JPGs already there are kept.
pub fn heic_to_jpg(folder: &Path, say: &dyn Fn(String)) -> Result<String, String> {
    let heic: Vec<PathBuf> = backup::backup_files(folder, None)?
        .into_iter()
        .map(|(p, ..)| p)
        .filter(|p| {
            backup::plain_path(p)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("heic") || e.eq_ignore_ascii_case("heif"))
        })
        .collect();
    if heic.is_empty() {
        return Ok("This backup has no HEIC photos.".into());
    }
    let out_root = folder.join("jpg");
    let (mut made, mut had, mut failed) = (0, 0, Vec::new());
    for (i, src) in heic.iter().enumerate() {
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        say(format!(
            "Converting {} of {}…",
            backup::count(i + 1),
            files(heic.len())
        ));
        let plain = backup::plain_path(src);
        let dst = out_root
            .join(plain.strip_prefix(folder).unwrap_or(&plain))
            .with_extension("jpg");
        if backup::stored(&dst).is_some() {
            had += 1;
            continue;
        }
        let _ = fs::create_dir_all(dst.parent().unwrap_or(&out_root));
        // With scope "all" the JPG is sealed like the photo it came from.
        let made_one = to_jpg(src, &dst).and_then(|()| match backup::seal_in(folder, &dst) {
            Ok(None) => Ok(()),
            Ok(Some(k)) => backup::copy_plain(&dst, &dst, Some(&k))
                .and_then(|_| fs::remove_file(&dst))
                .map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        });
        match made_one {
            Ok(()) => made += 1,
            Err(_) => {
                let _ = fs::remove_file(&dst);
                failed.push(
                    src.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut msg = format!("Saved {} as JPG in the backup's jpg folder.", files(made));
    if had > 0 {
        msg += &format!(" {} {} converted before.", files(had), backup::were(had));
    }
    if !failed.is_empty() {
        msg += &format!(
            " {} could not be read, for example {}. This converter reads 8-bit HEIC photos only.",
            files(failed.len()),
            failed[..failed.len().min(3)].join(", ")
        );
    }
    if STOP.load(Ordering::Relaxed) {
        msg = format!("You stopped converting. {msg}");
    }
    Ok(msg)
}

fn to_jpg(src: &Path, dst: &Path) -> Result<(), String> {
    use image::ImageEncoder;
    use zune_heic::zune_core::bytestream::ZCursor;
    let data = backup::read_file(src).map_err(|e| e.to_string())?;
    let mut d = zune_heic::HeifDecoder::new(ZCursor::new(&data));
    let pixels = d.decode().map_err(|e| format!("{e:?}"))?;
    let (Some(w), Some(h), Some(cs)) = (d.width(), d.height(), d.colorspace()) else {
        return Err("no size".into());
    };
    let (w, h) = (w as u32, h as u32);
    let img: Option<image::DynamicImage> = match cs.num_components() {
        1 => image::GrayImage::from_raw(w, h, pixels).map(Into::into),
        2 => image::GrayAlphaImage::from_raw(w, h, pixels).map(Into::into),
        3 => image::RgbImage::from_raw(w, h, pixels).map(Into::into),
        4 => image::RgbaImage::from_raw(w, h, pixels).map(Into::into),
        _ => None,
    };
    let rgb = img.ok_or("unexpected pixel layout")?.to_rgb8();
    let out = fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut enc =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(&out), 90);
    if let Some(exif) = d.exif_data() {
        // The decoder has already turned the picture upright.
        let _ = enc.set_exif_metadata(upright(exif.clone()));
    }
    enc.write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    if let Ok(t) = fs::metadata(src).and_then(|m| m.modified()) {
        let _ = out.set_modified(t);
    }
    Ok(())
}

/// Sets the Exif orientation (tag 0x0112 in the first IFD) to 1, "as stored", so viewers do not
/// turn an already upright picture a second time.
fn upright(mut exif: Vec<u8>) -> Vec<u8> {
    let le = exif.starts_with(b"II");
    let u16_at = |b: &[u8], i: usize| {
        let x = [*b.get(i)?, *b.get(i + 1)?];
        Some(if le {
            u16::from_le_bytes(x)
        } else {
            u16::from_be_bytes(x)
        })
    };
    let u32_at = |b: &[u8], i: usize| {
        let x = [*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?];
        Some(if le {
            u32::from_le_bytes(x)
        } else {
            u32::from_be_bytes(x)
        })
    };
    let Some(ifd) = u32_at(&exif, 4).map(|o| o as usize) else {
        return exif;
    };
    for k in 0..u16_at(&exif, ifd).unwrap_or(0) as usize {
        let entry = ifd + 2 + 12 * k;
        if u16_at(&exif, entry) == Some(0x0112) && entry + 10 <= exif.len() {
            let one = if le {
                1u16.to_le_bytes()
            } else {
                1u16.to_be_bytes()
            };
            exif[entry + 8..entry + 10].copy_from_slice(&one);
        }
    }
    exif
}

/// Writes `data/contacts-merged.vcf` with contacts of the same name, phone number or email
/// merged into one. `contacts.vcf` is left as it is, so deleting the merged file undoes it.
pub fn merge_contacts(folder: &Path) -> Result<String, String> {
    let src = folder.join("data").join("contacts.vcf");
    let text = match backup::read_file(&src) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Err(e.to_string()),
        Err(_) => {
            return Err(
                "This backup has no contacts.vcf. Back up with contacts ticked first.".into(),
            );
        }
    };
    let (merged, before, after) = merge_vcards(&text);
    let out = folder.join("data").join(MERGED);
    // A copy left from before the backup had a password goes, so only one is there.
    let _ = fs::remove_file(crypt_or_plain_other(&out));
    backup::write_file(folder, &out, merged.as_bytes())
        .map_err(|e| format!("Cannot save {MERGED}: {e}"))?;
    Ok(if before == after {
        format!(
            "No duplicates among {} contacts. {MERGED} is a plain copy.",
            backup::count(before)
        )
    } else {
        format!(
            "Merged {} contacts into {}, saved as {MERGED} next to contacts.vcf. The original is unchanged.",
            backup::count(before),
            backup::count(after)
        )
    })
}

pub const MERGED: &str = "contacts-merged.vcf";

/// The merged contacts file of the other kind: the sealed copy if the backup now writes plain
/// files, else the plain one. Both are removed by Undo.
fn crypt_or_plain_other(out: &Path) -> PathBuf {
    match backup::stored(out) {
        Some(p) if backup::is_sealed(&p) => out.to_path_buf(),
        _ => backup::crypt::sealed_path(out),
    }
}

/// Removes the merged contacts file, plain or sealed.
pub fn unmerge_contacts(folder: &Path) -> Result<String, String> {
    let out = folder.join("data").join(MERGED);
    let at = backup::stored(&out).ok_or_else(|| format!("{MERGED} is already gone."))?;
    fs::remove_file(&at).map_err(|e| format!("Cannot remove {MERGED}: {e}"))?;
    Ok(format!("Removed {MERGED}. contacts.vcf was never changed."))
}

/// Merges vCards that share a name, a phone number (last 10 digits) or an email. Returns the
/// merged file and the number of cards before and after.
fn merge_vcards(text: &str) -> (String, usize, usize) {
    // Unfold continuation lines, then split into cards of property lines.
    let mut cards: Vec<Vec<String>> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        match raw.strip_prefix([' ', '\t']) {
            Some(more) if !lines.is_empty() => lines.last_mut().unwrap().push_str(more),
            _ => lines.push(raw.to_string()),
        }
    }
    for line in lines {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VCARD" {
            cards.push(Vec::new());
        } else if upper != "END:VCARD"
            && !upper.starts_with("VERSION:")
            && let Some(card) = cards.last_mut()
        {
            card.push(line);
        }
    }
    // What makes two lines the same: phone numbers by their digits, emails and names in any case.
    let key = |line: &str| -> Option<String> {
        let (prop, value) = line.split_once(':')?;
        let prop = prop.split(';').next()?.to_ascii_uppercase();
        let value = value.trim();
        match prop.as_str() {
            "TEL" => {
                let d: String = value.chars().filter(char::is_ascii_digit).collect();
                // Short codes like 911 are shared by unrelated contacts.
                (d.len() >= 7).then(|| format!("TEL:{}", &d[d.len().saturating_sub(10)..]))
            }
            "EMAIL" if !value.is_empty() => Some(format!("EMAIL:{}", value.to_lowercase())),
            "FN" if !value.is_empty() => Some(format!("FN:{}", value.to_lowercase())),
            _ => None,
        }
    };
    // Union-find over cards that share any key.
    let mut parent: Vec<usize> = (0..cards.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    let mut owner: HashMap<String, usize> = HashMap::new();
    for (i, card) in cards.iter().enumerate() {
        for k in card.iter().filter_map(|l| key(l)) {
            let j = *owner.entry(k).or_insert(i);
            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
            parent[a.max(b)] = a.min(b);
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for i in 0..cards.len() {
        let r = root(&mut parent, i);
        let g = *slot.entry(r).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[g].push(i);
    }
    let mut out = String::new();
    for group in &groups {
        let mut seen = std::collections::HashSet::new();
        let mut has = |prop: &str| !seen.insert(prop.to_string());
        out += "BEGIN:VCARD\r\nVERSION:3.0\r\n";
        for line in group.iter().flat_map(|&i| &cards[i]) {
            let prop = line
                .split([':', ';'])
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            // One name per card: the first card's.
            let dup = match prop.as_str() {
                "FN" | "N" => has(&prop),
                _ => has(&key(line).unwrap_or_else(|| line.clone())),
            };
            if !dup {
                out += line;
                out += "\r\n";
            }
        }
        out += "END:VCARD\r\n";
    }
    (out, cards.len(), groups.len())
}

/// Phone paths, backup sizes and backup checksums of the chosen files of a backup.
pub fn chosen_on_phone(
    folder: &Path,
    chosen: &std::collections::HashSet<PathBuf>,
) -> Result<Vec<OnPhone>, String> {
    let sums = backup::read_sums(folder);
    Ok(backup::saved_files(folder, None)?
        .0
        .into_iter()
        .filter(|j| chosen.contains(&j.local))
        .map(|j| {
            // The phone holds the file itself, so a sealed copy is opened and hashed again.
            let sum = if backup::is_sealed(&j.local) {
                backup::plain_sha256(&j.local)
                    .map(|h| h.iter().map(|b| format!("{b:02x}")).collect())
                    .unwrap_or_default()
            } else {
                sums.get(&backup::rel(folder, &j.local))
                    .cloned()
                    .unwrap_or_default()
            };
            (j.remote, j.size, sum)
        })
        .collect())
}

/// The wanted phone files that the phone has at exactly the backup's size, from
/// `stat -c '%s %n'` output. Paths outside shared storage are never matched.
fn same_size(wanted: &[OnPhone], stat: &str) -> Vec<OnPhone> {
    let on_phone: HashMap<&str, u64> = stat
        .lines()
        .filter_map(|l| {
            let (n, p) = l.split_once(' ')?;
            Some((p, n.parse().ok()?))
        })
        .collect();
    wanted
        .iter()
        .filter(|(p, n, _)| safe_to_delete(p) && on_phone.get(p.as_str()) == Some(n))
        .cloned()
        .collect()
}

/// The wanted files whose SHA-256 on the phone, from `sha256sum` output, equals the backup's.
/// A file with no sum in the backup never matches, nor does a name `sha256sum` escapes.
fn same_sum(wanted: &[OnPhone], out: &str) -> Vec<OnPhone> {
    let on_phone: HashMap<&str, &str> = out
        .lines()
        .filter_map(|l| l.split_once("  "))
        .map(|(sum, p)| (p, sum))
        .collect();
    wanted
        .iter()
        .filter(|(p, _, sum)| !sum.is_empty() && on_phone.get(p.as_str()) == Some(&sum.as_str()))
        .cloned()
        .collect()
}

/// Only files in shared storage or an SD card, never a path that climbs out with `..`, and
/// never one with a line break, which would split the `stat` output.
fn safe_to_delete(path: &str) -> bool {
    (path.starts_with("/sdcard/") || path.starts_with("/storage/"))
        && !path.split('/').any(|p| p == "..")
        && !path.contains('\n')
}

/// Of the wanted files, those the phone has with exactly the backup's contents (same size,
/// then same SHA-256), and how many it does not.
pub fn match_on_phone(serial: &str, wanted: &[OnPhone]) -> Result<(Vec<OnPhone>, usize), String> {
    if !backup::connected(serial) {
        return Err("The phone was disconnected. Plug it back in and try again.".into());
    }
    let paths: Vec<&str> = wanted
        .iter()
        .map(|(p, ..)| p.as_str())
        .filter(|p| safe_to_delete(p))
        .collect();
    let sized = same_size(
        wanted,
        &backup::shell_each(serial, "stat -c '%s %n'", &paths),
    );
    let paths: Vec<&str> = sized.iter().map(|(p, ..)| p.as_str()).collect();
    let found = same_sum(&sized, &backup::shell_each(serial, "sha256sum", &paths));
    let left = wanted.len() - found.len();
    Ok((found, left))
}

/// Deletes the confirmed files from the phone, checking each one's contents again right
/// before, then checks that they are gone and asks the phone to refresh its Gallery.
pub fn delete_from_phone(serial: &str, confirmed: &[OnPhone]) -> Result<String, String> {
    let (matched, _) = match_on_phone(serial, confirmed)?;
    let paths: Vec<&str> = matched.iter().map(|(p, ..)| p.as_str()).collect();
    backup::shell_each(serial, "rm -f", &paths);
    let still: std::collections::HashSet<String> =
        backup::shell_each(serial, "stat -c '%n'", &paths)
            .lines()
            .map(String::from)
            .collect();
    // A gone phone answers `stat` with nothing, which would look like every file was deleted.
    if !backup::connected(serial) {
        return Err("The phone was disconnected while deleting. Plug it back in and choose the files again to see which are left.".into());
    }
    let gone: Vec<&OnPhone> = matched
        .iter()
        .filter(|(p, ..)| !still.contains(p))
        .collect();
    if !gone.is_empty() {
        backup::shell_each(
            serial,
            "content call --uri content://media --method scan_volume --arg",
            &["external_primary"],
        );
    }
    let bytes: u64 = gone.iter().map(|(_, n, _)| n).sum();
    let mut msg = format!(
        "Deleted {} from the phone, freeing {}.",
        files(gone.len()),
        size(bytes)
    );
    let kept = confirmed.len() - gone.len();
    if kept > 0 {
        msg += &format!(
            " {} stayed on the phone because they changed or could not be deleted.",
            files(kept)
        );
    }
    Ok(msg)
}

/// scrcpy.exe next to this app (or in a `scrcpy` folder there), then on PATH.
pub fn scrcpy_path() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    beside
        .iter()
        .flat_map(|d| [d.clone(), d.join("scrcpy")])
        .chain(on_path)
        .map(|d| {
            d.join(if cfg!(windows) {
                "scrcpy.exe"
            } else {
                "scrcpy"
            })
        })
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletes_only_same_size_files_in_shared_storage() {
        let wanted: Vec<OnPhone> = [
            ("/sdcard/DCIM/a b.jpg", 10),
            ("/sdcard/DCIM/changed.jpg", 10),
            ("/sdcard/DCIM/gone.jpg", 10),
            ("/sdcard/../data/x", 5),
            ("/data/local/tmp/x", 5),
            ("/sdcard/it's $x.txt", 0),
        ]
        .map(|(p, n)| (p.to_string(), n, "ab".to_string()))
        .into();
        let stat = "10 /sdcard/DCIM/a b.jpg\n11 /sdcard/DCIM/changed.jpg\n5 /sdcard/../data/x\n5 /data/local/tmp/x\n0 /sdcard/it's $x.txt\n";
        let found: Vec<String> = same_size(&wanted, stat)
            .into_iter()
            .map(|(p, ..)| p)
            .collect();
        assert_eq!(found, ["/sdcard/DCIM/a b.jpg", "/sdcard/it's $x.txt"]);
        assert!(!safe_to_delete("/sdcard/a\nb"));
        assert!(safe_to_delete("/storage/3A1F-0C2B/DCIM/x..y.jpg"));
    }

    #[test]
    fn deletes_only_files_with_the_backups_checksum() {
        let wanted: Vec<OnPhone> = [
            ("/sdcard/same.jpg", "aa"),
            ("/sdcard/edited.jpg", "bb"),
            ("/sdcard/no sum.jpg", ""),
            ("/sdcard/back\\slash.jpg", "cc"),
            ("/sdcard/not listed.jpg", "dd"),
        ]
        .map(|(p, s)| (p.to_string(), 1, s.to_string()))
        .into();
        // sha256sum escapes a name with a backslash and starts its line with one.
        let out = "aa  /sdcard/same.jpg\nff  /sdcard/edited.jpg\n  /sdcard/no sum.jpg\n\\cc  /sdcard/back\\\\slash.jpg\n";
        let found: Vec<String> = same_sum(&wanted, out)
            .into_iter()
            .map(|(p, ..)| p)
            .collect();
        assert_eq!(found, ["/sdcard/same.jpg"]);
    }

    #[test]
    fn finds_identical_files_only() {
        let dir = std::env::temp_dir().join(format!("pb-dupes-{}", std::process::id()));
        let f = dir.join("files");
        fs::create_dir_all(f.join("b")).unwrap();
        fs::write(f.join("a.jpg"), "same").unwrap();
        fs::write(f.join("b").join("a copy.jpg"), "same").unwrap();
        fs::write(f.join("other.jpg"), "diff").unwrap(); // same size, other bytes
        fs::write(f.join("empty1"), "").unwrap();
        fs::write(f.join("empty2"), "").unwrap();
        let groups = duplicates(&dir, &|_| {}).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        assert!(groups[0][0].0.ends_with("a.jpg"));
    }

    #[test]
    fn merges_contacts_of_a_protected_backup_sealed() {
        let f = std::env::temp_dir().join(format!("pb-merge-vcf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&f);
        fs::create_dir_all(f.join("data")).unwrap();
        backup::set_password(&f, "pw", backup::Scope::Data).unwrap();
        let card = "BEGIN:VCARD\nVERSION:3.0\nFN:Ann\nTEL:+15550100\nEND:VCARD\n";
        backup::write_file(&f, &f.join("data/contacts.vcf"), card.repeat(2).as_bytes()).unwrap();
        let said = merge_contacts(&f).unwrap();
        assert!(said.starts_with("Merged 2 contacts into 1"), "{said}");
        let out = f.join("data").join(MERGED);
        assert!(!out.exists());
        assert!(backup::read_text(&out).unwrap().contains("FN:Ann"));
        assert!(unmerge_contacts(&f).unwrap().starts_with("Removed"));
        assert!(backup::stored(&out).is_none());
        backup::forget(&f);
        assert!(merge_contacts(&f).unwrap_err().contains("password"));
        let _ = fs::remove_dir_all(&f);
    }

    #[test]
    fn merges_contacts_by_name_number_or_email() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ann Lee\r\nN:;Ann Lee;;;\r\nTEL:+1 555 010 0100\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:3.0\r\nFN:ann lee\r\nTEL:5550100100\r\nEMAIL:ann@x.com\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Annie\r\nEMAIL:ANN@x.com\r\nTEL:911\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Bob\r\nTEL:911\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Cat\r\nEMAIL:\r\nEND:VCARD\r\n\
BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Dan\r\nEMAIL: \r\nEND:VCARD\r\n";
        let (out, before, after) = merge_vcards(vcf);
        // An empty email joins nobody.
        assert_eq!((before, after), (6, 4));
        assert_eq!(out.matches("BEGIN:VCARD").count(), 4);
        // One name and one copy of each number and email in the merged card.
        let ann = out.split("END:VCARD").next().unwrap();
        assert_eq!(ann.matches("FN:").count(), 1);
        assert!(ann.contains("FN:Ann Lee"));
        assert_eq!(ann.matches("TEL:").count(), 2, "{ann}"); // the long number once, 911 once
        assert_eq!(ann.matches("EMAIL:").count(), 1);
    }

    /// Puts files with awkward names on a real phone or emulator, then deletes them as the
    /// Tools page does. Run with `PB_E2E_SERIAL=emulator-5554 cargo test e2e_free -- --ignored`.
    /// Touches only /sdcard/E2E/free.
    #[test]
    #[ignore]
    fn e2e_free_up_space() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let dir = std::env::temp_dir().join(format!("pb-free-{}", std::process::id()));
        let free = dir.join("files").join("E2E").join("free");
        fs::create_dir_all(free.join("sp ace dir")).unwrap();
        let names = [
            "it's $x.txt",
            "ünïcødé 日本.txt",
            " leading space.txt",
            "sp ace dir/x y.txt",
            "-dash.txt",
        ];
        for name in names {
            fs::write(
                free.join(name),
                "same
",
            )
            .unwrap();
        }
        backup::shell_each(&serial, "rm -rf", &["/sdcard/E2E/free"]);
        let pushed = backup::adb()
            .args(["-s", &serial, "push"])
            .arg(&free)
            .arg("/sdcard/E2E/")
            .output()
            .unwrap();
        assert!(pushed.status.success(), "{pushed:?}");
        // A backup copy of the same size but other contents: that phone file must stay.
        fs::write(free.join("-dash.txt"), "SAME\n").unwrap();
        let chosen: std::collections::HashSet<PathBuf> = backup::backup_files(&dir, None)
            .unwrap()
            .into_iter()
            .map(|(p, ..)| p)
            .collect();
        let all: Vec<PathBuf> = chosen.iter().cloned().collect();
        backup::update_sums(&dir, &[], &all, &|_| {}).unwrap();
        let wanted = chosen_on_phone(&dir, &chosen).unwrap();
        let (matched, left) = match_on_phone(&serial, &wanted).unwrap();
        assert_eq!((matched.len(), left), (4, 1), "{matched:?}");
        let msg = delete_from_phone(&serial, &matched).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(msg.starts_with("Deleted 4 files"), "{msg}");
        let on_phone = backup::shell_each(&serial, "find", &["/sdcard/E2E/free", "-type", "f"]);
        backup::shell_each(&serial, "rm -rf", &["/sdcard/E2E/free"]);
        assert_eq!(on_phone.trim(), "/sdcard/E2E/free/-dash.txt");
    }

    #[test]
    fn exif_orientation_becomes_upright() {
        // Little-endian TIFF, IFD0 at 8 with one entry: orientation (SHORT) = 6.
        let mut le = b"II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0".to_vec();
        le.extend([0, 0, 0, 0]);
        assert_eq!(upright(le)[18..20], [1, 0]);
        let be = b"MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0".to_vec();
        assert_eq!(upright(be)[18..20], [0, 1]);
        assert_eq!(upright(b"junk".to_vec()), b"junk");
    }
}

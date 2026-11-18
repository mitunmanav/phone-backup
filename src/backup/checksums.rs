use super::*;

/// Lists files whose name was changed to be legal on Windows: "files/Docs/_CON.txt<TAB>/sdcard/Docs/CON.txt".
pub(super) const RENAMED: &str = "renamed.txt";

/// The renamed files of a backup folder: path inside it (with `/`) to the phone path.
pub(super) fn renamed(root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(root.join(RENAMED))
        .unwrap_or_default()
        .lines()
        // safe_name turns tabs into `_`, so the first tab always ends the local path.
        .filter_map(|l| {
            l.split_once('\t')
                .map(|(a, b)| (a.to_string(), b.to_string()))
        })
        .collect()
}

/// SHA-256 of every file in a backup, in `sha256sum` format so `sha256sum -c` can check it too.
pub const CHECKSUMS: &str = "checksums.sha256";

/// The sums of a backup folder: path inside it (with `/`) to the hex SHA-256.
pub(crate) fn read_sums(root: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(root.join(CHECKSUMS))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            l.split_once("  ")
                .map(|(sum, path)| (path.to_string(), sum.to_string()))
        })
        .collect()
}

/// `path` inside `root`, with `/` as `sha256sum` writes it.
pub(crate) fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn sha256(path: &Path) -> std::io::Result<String> {
    Ok(sha256_bytes(path)?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// SHA-256 of a file, read in chunks so large videos never sit in memory.
pub fn sha256_bytes(path: &Path) -> std::io::Result<[u8; 32]> {
    use sha2::Digest;
    let (mut file, mut hasher) = (fs::File::open(path)?, sha2::Sha256::new());
    let mut buf = vec![0; 1 << 16];
    loop {
        match file.read(&mut buf)? {
            0 => return Ok(hasher.finalize().into()),
            n => hasher.update(&buf[..n]),
        }
    }
}

/// The most threads hashing uses: 4, or the cores this machine has if fewer.
fn hash_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(4)
}

/// Hashes `paths` on `threads` threads. Returns one entry per path in path order, or None for a
/// file that was not hashed because the job was stopped. `got` runs on the calling thread as
/// each result arrives, in whatever order the workers finish.
fn hash_all(
    paths: &[PathBuf],
    threads: usize,
    mut got: impl FnMut(usize, &std::io::Result<String>),
) -> Vec<Option<std::io::Result<String>>> {
    let mut results: Vec<Option<std::io::Result<String>>> = paths.iter().map(|_| None).collect();
    let threads = threads.clamp(1, paths.len().max(1));
    let (tx, rx) = std::sync::mpsc::channel();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let tx = tx.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    if stopped() {
                        break;
                    }
                    if tx.send((i, sha256(path))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (i, result) in rx {
            got(i, &result);
            results[i] = Some(result);
        }
    });
    results
}

/// Drops the sums of `fresh` files (just rewritten) and of files gone from the folder, then
/// hashes each `known` file that has no sum. Old sums of untouched files are kept, so a later
/// backup hashes only what it copied. A failed write leaves the old checksum file whole.
pub(crate) fn update_sums(
    root: &Path,
    fresh: &[PathBuf],
    known: &[PathBuf],
    report: &dyn Fn(Msg),
) -> std::io::Result<()> {
    let old = read_sums(root);
    let mut sums = old.clone();
    sums.retain(|path, _| root.join(path).is_file());
    for p in fresh {
        sums.remove(&rel(root, p));
    }
    let todo: Vec<PathBuf> = known
        .iter()
        .filter(|p| !sums.contains_key(&rel(root, p)))
        .cloned()
        .collect();
    if !todo.is_empty() {
        report(Msg::Status(format!(
            "Saving checksums: {} of {}…",
            count(0),
            files(todo.len())
        )));
    }
    let mut done = 0;
    let hashed = hash_all(&todo, hash_threads(), |_, _| {
        done += 1;
        if done % 100 == 0 {
            report(Msg::Status(format!(
                "Saving checksums: {} of {}…",
                count(done),
                files(todo.len())
            )));
        }
    });
    for (p, hashed) in todo.iter().zip(hashed) {
        // A file that failed to copy is not there to hash; it gets its sum next time.
        if let Some(Ok(sum)) = hashed {
            sums.insert(rel(root, p), sum);
        }
    }
    if sums == old {
        return Ok(());
    }
    let part = root.join(format!("{CHECKSUMS}.part"));
    let text: String = sums
        .iter()
        .map(|(path, sum)| format!("{sum}  {path}\n"))
        .collect();
    let written = fs::write(&part, text).and_then(|()| fs::rename(&part, root.join(CHECKSUMS)));
    if written.is_err() {
        let _ = fs::remove_file(&part);
    }
    written
}

/// Re-hashes every file listed in a backup's checksums and says which changed or went missing.
pub fn verify(root: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    let sums = read_sums(root);
    if sums.is_empty() {
        return Err("This backup has no checksums yet. Back it up again to add them.".into());
    }
    let items: Vec<(&String, &String)> = sums.iter().collect();
    let paths: Vec<PathBuf> = items.iter().map(|(path, _)| root.join(path)).collect();
    let mut done = 0;
    let hashed = hash_all(&paths, hash_threads(), |i, _| {
        report(Msg::Progress {
            done,
            total: sums.len(),
            now: items[i].0.clone(),
        });
        done += 1;
    });
    if stopped() {
        return Err("You stopped the check. The backup was not changed.".into());
    }
    let (mut changed, mut missing) = (Vec::new(), Vec::new());
    for ((path, sum), hashed) in items.iter().zip(hashed) {
        match hashed {
            Some(Ok(s)) if s == **sum => {}
            Some(Ok(_)) => changed.push(path.as_str()),
            Some(Err(_)) => missing.push(path.as_str()),
            None => {}
        }
    }
    if changed.is_empty() && missing.is_empty() {
        return Ok(format!(
            "All {} match. Nothing has changed since they were backed up.",
            files(sums.len())
        ));
    }
    let mut msg = String::new();
    for (list, what) in [(&changed, "changed"), (&missing, "missing")] {
        if !list.is_empty() {
            msg += &format!(
                "{} {} {what}, for example {}.\n",
                files(list.len()),
                if list.len() == 1 { "is" } else { "are" },
                list.iter().take(3).copied().collect::<Vec<_>>().join(", ")
            );
        }
    }
    Err(msg + "Back up again to copy them from the phone, if it still has them.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn parallel_hash_matches_serial() {
        let root =
            std::env::temp_dir().join(format!("phone-backup-hash-all-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut paths = Vec::new();
        for (n, size) in [0, 1, 4096, 1 << 20].into_iter().enumerate() {
            let path = root.join(format!("file-{n}.bin"));
            fs::write(&path, vec![(n as u8).wrapping_mul(37); size]).unwrap();
            paths.push(path);
        }
        let gone = root.join("gone.bin");
        paths.push(gone);
        let quiet = |_: usize, _: &std::io::Result<String>| {};
        let one = hash_all(&paths, 1, quiet);
        let four = hash_all(&paths, 4, quiet);
        assert_eq!(one.len(), four.len());
        for (i, path) in paths.iter().enumerate() {
            match (&one[i], &four[i]) {
                (Some(Ok(a)), Some(Ok(b))) => {
                    assert_eq!(a, b, "{}", path.display());
                    assert_eq!(b, &sha256(path).unwrap(), "{}", path.display());
                }
                (Some(Err(_)), Some(Err(_))) => {
                    assert_eq!(i, paths.len() - 1, "only the gone file fails");
                }
                _ => panic!("hash_all differs for {}", path.display()),
            }
        }
        assert!(four.last().unwrap().as_ref().unwrap().is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    #[ignore]
    fn checksum_speed() {
        let root = std::env::temp_dir().join(format!(
            "phone-backup-checksum-speed-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let block = vec![0x5a; 32 << 20];
        let paths: Vec<PathBuf> = (0..64)
            .map(|n| {
                let path = root.join(format!("file-{n}.bin"));
                fs::write(&path, &block).unwrap();
                path
            })
            .collect();
        // Read every file once so the disk cache holds them before the timed runs.
        let quiet = |_: usize, _: &std::io::Result<String>| {};
        hash_all(&paths, 4, quiet);
        let one = Instant::now();
        hash_all(&paths, 1, quiet);
        let one = one.elapsed();
        let four = Instant::now();
        hash_all(&paths, 4, quiet);
        let four = four.elapsed();
        println!("1 thread: {one:?}");
        println!("4 threads: {four:?}");
        println!("speed-up: {:.2}x", one.as_secs_f64() / four.as_secs_f64());
        fs::remove_dir_all(&root).unwrap();
    }
}

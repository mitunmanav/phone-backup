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

/// Adds `more` (path inside the backup with `/`, phone path) to the backup's renamed files.
/// Written whole or not at all: restore reads this list to put the real names back.
pub(super) fn save_renamed(
    root: &Path,
    more: impl IntoIterator<Item = (String, String)>,
) -> std::io::Result<()> {
    let mut map = renamed(root);
    let before = map.len();
    map.extend(more);
    if map.len() == before && root.join(RENAMED).exists() {
        return Ok(());
    }
    let text: String = map
        .iter()
        .map(|(local, remote)| format!("{local}\t{remote}\n"))
        .collect();
    fs::create_dir_all(root)?;
    write_whole(&root.join(RENAMED), text.as_bytes())
}

/// Writes `path` through a temporary file, so a crash or a full disk leaves the old one whole.
pub(crate) fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = fs::File::create(&part)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
        .and_then(|()| fs::rename(&part, path));
    if written.is_err() {
        let _ = fs::remove_file(&part);
    }
    written
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
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(64);
    for b in sha256_bytes(path)? {
        hex.push(HEX[usize::from(b >> 4)] as char);
        hex.push(HEX[usize::from(b & 15)] as char);
    }
    Ok(hex)
}

/// SHA-256 of a file, read in chunks so large videos never sit in memory.
pub fn sha256_bytes(path: &Path) -> std::io::Result<[u8; 32]> {
    use sha2::Digest;
    let (mut file, mut hasher) = (fs::File::open(path)?, sha2::Sha256::new());
    // 256 KB reads: fewer system calls than small ones, and still small enough to stay in cache.
    let mut buf = vec![0; 1 << 18];
    loop {
        match file.read(&mut buf)? {
            0 => return Ok(hasher.finalize().into()),
            n => hasher.update(&buf[..n]),
        }
    }
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
    update_sums_with(root, fresh, known, HashMap::new(), report)
}

/// `update_sums`, with sums already made for some of the `known` files, such as by a `Hasher`
/// while they were copied. Those files are not read again.
pub(crate) fn update_sums_with(
    root: &Path,
    fresh: &[PathBuf],
    known: &[PathBuf],
    mut ready: HashMap<PathBuf, String>,
    report: &dyn Fn(Msg),
) -> std::io::Result<()> {
    let old = read_sums(root);
    let mut sums = old.clone();
    // One walk of the folder says which files are there; only names it misses (another letter
    // case, an unreadable folder) are asked about one by one.
    let there: HashSet<String> = super::adb::stamps(root)
        .keys()
        .map(|p| rel(root, p))
        .collect();
    sums.retain(|path, _| there.contains(path) || root.join(path).is_file());
    for p in fresh {
        sums.remove(&rel(root, p));
    }
    let mut todo: Vec<&PathBuf> = Vec::new();
    for p in known {
        let key = rel(root, p);
        if sums.contains_key(&key) {
            continue;
        }
        match ready.remove(p) {
            Some(sum) => {
                sums.insert(key, sum);
            }
            None => todo.push(p),
        }
    }
    let mut said = usize::MAX;
    let hashed = par_map(
        &todo,
        // A file that failed to copy is not there to hash; it gets its sum next time.
        |p| (!stopped()).then(|| sha256(p).ok()).flatten(),
        &mut |n| {
            // Many files: say how far it got, a few times a second at most.
            if todo.len() > 100 && n / 100 != said / 100 {
                said = n;
                report(Msg::Status(format!(
                    "Saving checksums: {} of {}…",
                    count(n),
                    files(todo.len())
                )));
            }
        },
    );
    for (p, sum) in todo.iter().zip(hashed) {
        if let Some(sum) = sum {
            sums.insert(rel(root, p), sum);
        }
    }
    if sums == old {
        return Ok(());
    }
    let text: String = sums
        .iter()
        .map(|(path, sum)| format!("{sum}  {path}\n"))
        .collect();
    write_whole(&root.join(CHECKSUMS), text.as_bytes())
}

/// Hashes files on a few threads while the backup goes on copying, so each file is read while
/// it is still in memory and the copy does not wait for it.
pub(crate) struct Hasher {
    tx: Option<std::sync::mpsc::Sender<PathBuf>>,
    workers: Vec<std::thread::JoinHandle<Vec<(PathBuf, String)>>>,
}

impl Hasher {
    pub(crate) fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        let workers = (0..2)
            .map(|_| {
                let rx = rx.clone();
                std::thread::spawn(move || {
                    let mut done = Vec::new();
                    loop {
                        let next = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok(path) = next else { break };
                        if stopped() {
                            break;
                        }
                        if let Ok(sum) = sha256(&path) {
                            done.push((path, sum));
                        }
                    }
                    done
                })
            })
            .collect();
        Hasher {
            tx: Some(tx),
            workers,
        }
    }

    /// Queues a file that has arrived whole.
    pub(crate) fn add(&self, path: PathBuf) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(path);
        }
    }

    /// Waits for the queued files and returns their sums.
    pub(crate) fn finish(mut self) -> HashMap<PathBuf, String> {
        drop(self.tx.take());
        std::mem::take(&mut self.workers)
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    }
}

/// Re-hashes every file listed in a backup's checksums and says which changed or went missing.
pub fn verify(root: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    let sums = read_sums(root);
    if sums.is_empty() {
        return Err("This backup has no checksums yet. Back it up again to add them.".into());
    }
    let list: Vec<(&String, &String)> = sums.iter().collect();
    let mut shown = usize::MAX;
    // What is wrong with each file: (changed, missing, unreadable), or None when it matches.
    // Stop leaves the rest unread.
    let found = par_map(
        &list,
        |(path, sum)| match (!stopped()).then(|| sha256(&root.join(path))) {
            None => None,
            Some(Ok(s)) if s == **sum => None,
            Some(Ok(_)) => Some(0),
            Some(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Some(1),
            // Open in another program, or a failing drive: there, but not checked.
            Some(Err(_)) => Some(2),
        },
        &mut |n| {
            if n != shown {
                shown = n;
                report(Msg::Progress {
                    done: n,
                    total: list.len(),
                    now: list.get(n).map(|(p, _)| p.to_string()).unwrap_or_default(),
                });
            }
        },
    );
    if stopped() {
        return Err("You stopped the check. The backup was not changed.".into());
    }
    let mut bad: [Vec<&str>; 3] = Default::default();
    for ((path, _), what) in list.iter().zip(found) {
        if let Some(what) = what {
            bad[what].push(path.as_str());
        }
    }
    if bad.iter().all(Vec::is_empty) {
        return Ok(format!(
            "All {} match. Nothing has changed since they were backed up.",
            files(sums.len())
        ));
    }
    let mut msg = String::new();
    for (list, what) in bad.iter().zip(["changed", "missing", "could not be read"]) {
        if !list.is_empty() {
            let verb = match list.len() {
                _ if what == "could not be read" => "",
                1 => "is ",
                _ => "are ",
            };
            msg += &format!(
                "{} {verb}{what}, for example {}.\n",
                files(list.len()),
                list.iter().take(3).copied().collect::<Vec<_>>().join(", ")
            );
        }
    }
    if !bad[2].is_empty() {
        msg += "Close any program using them, or check the drive, then verify again.\n";
    }
    if bad[0].is_empty() && bad[1].is_empty() {
        return Err(msg.trim_end().into());
    }
    Err(msg + "Back up again to copy them from the phone, if it still has them.")
}

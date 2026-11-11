use crate::*;

/// What the file picker's worker sends back.
pub(crate) enum Found {
    List(Listed),
    Thumb(PathBuf, Option<egui::ColorImage>),
}

/// The open "Choose files" dialog. Dropping it ends its worker thread.
pub(crate) struct Picker {
    pub(crate) folder: PathBuf,
    /// None while the worker reads the backup.
    pub(crate) files: Option<Listed>,
    /// `backup::flags` of each file.
    pub(crate) flags: Vec<u8>,
    pub(crate) chosen: HashSet<PathBuf>,
    /// None: not a picture this app can show.
    pub(crate) thumbs: HashMap<PathBuf, Option<egui::TextureHandle>>,
    /// The rows last asked for, so the worker hears only of changes.
    pub(crate) asked: Vec<PathBuf>,
    pub(crate) ask: mpsc::Sender<Vec<PathBuf>>,
    pub(crate) rx: mpsc::Receiver<Found>,
    /// Choosing files to delete from the phone, not to restore.
    pub(crate) delete: bool,
}

/// An 80 px square preview of a picture, turned the way the camera held it.
pub(crate) fn thumbnail(path: &Path) -> Option<egui::ColorImage> {
    if !backup::is_sealed(path) {
        return small(image::ImageReader::open(path).ok()?);
    }
    // Only pictures are opened: a sealed video would be read whole for nothing.
    let picture = backup::plain_path(path).extension().is_some_and(|e| {
        ["jpg", "jpeg", "png", "webp", "gif"]
            .iter()
            .any(|p| e.eq_ignore_ascii_case(p))
    });
    if !picture || !fs::metadata(path).is_ok_and(|m| m.len() < 64 << 20) {
        return None;
    }
    let bytes = backup::read_file(path).ok()?;
    small(image::ImageReader::new(std::io::Cursor::new(bytes)))
}

/// An 80 by 80 preview of the picture `reader` holds, turned upright.
fn small<R: std::io::BufRead + std::io::Seek>(
    reader: image::ImageReader<R>,
) -> Option<egui::ColorImage> {
    use image::ImageDecoder;
    let mut decoder = reader.with_guessed_format().ok()?.into_decoder().ok()?;
    let turn = decoder.orientation().ok()?;
    let mut img = image::DynamicImage::from_decoder(decoder)
        .ok()?
        .resize_to_fill(80, 80, image::imageops::FilterType::Triangle);
    img.apply_orientation(turn);
    let rgba = img.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        &rgba,
    ))
}

impl App {
    /// Opens the file picker; a worker lists the backup, then makes previews of the rows in view.
    pub(crate) fn open_picker(&mut self, ctx: &egui::Context, folder: PathBuf, delete: bool) {
        let (ask, asks) = mpsc::channel::<Vec<PathBuf>>();
        let (tx, rx) = mpsc::channel();
        let (ctx, dir) = (ctx.clone(), folder.clone());
        let version = self.version_of(&folder);
        thread::spawn(move || {
            let send = |f: Found| {
                ctx.request_repaint();
                tx.send(f).is_ok()
            };
            if !send(Found::List(backup::backup_files(&dir, version.as_deref()))) {
                return;
            }
            let mut todo = Vec::new();
            loop {
                // Only the rows in view now: rows scrolled past are never decoded.
                while let Ok(rows) = asks.try_recv() {
                    todo = rows;
                }
                if todo.is_empty() {
                    match asks.recv() {
                        Ok(rows) => todo = rows,
                        Err(_) => return,
                    }
                    continue;
                }
                let path = todo.remove(0);
                let img = thumbnail(&path);
                if !send(Found::Thumb(path, img)) {
                    return;
                }
            }
        });
        self.picker = Some(Picker {
            folder,
            files: None,
            flags: Vec::new(),
            chosen: HashSet::new(),
            thumbs: HashMap::new(),
            asked: Vec::new(),
            ask,
            rx,
            delete,
        });
    }

    pub(crate) fn picker_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(p) = &mut self.picker else { return };
        while let Ok(found) = p.rx.try_recv() {
            match found {
                Found::List(list) => {
                    if let Ok(list) = &list {
                        p.chosen = match (&self.only, &self.free_pick) {
                            // Nothing is chosen for deleting until the user ticks it.
                            (_, Some((folder, chosen))) if p.delete && *folder == p.folder => {
                                chosen.clone()
                            }
                            _ if p.delete => HashSet::new(),
                            (Some((folder, chosen, _)), _) if *folder == p.folder => {
                                (**chosen).clone()
                            }
                            _ => list.iter().map(|(f, ..)| f.clone()).collect(),
                        };
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs() as i64);
                        p.flags = backup::flags(list, now);
                    }
                    p.files = Some(list);
                }
                Found::Thumb(path, img) => {
                    let tex = img.map(|img| {
                        ctx.load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR)
                    });
                    p.thumbs.insert(path, tex);
                }
            }
        }
        let mut wanted = Vec::new();
        let close = dialog(
            ctx,
            "picker",
            if p.delete {
                "Choose files to delete from the phone"
            } else if self.tab == Tab::Transfer {
                "Choose files to copy"
            } else {
                "Choose files to restore"
            },
            520.0,
            "Done",
            |ui| {
                match &p.files {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reading the backup…");
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(t.error));
                    }
                    Some(Ok(list)) => {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Select all").clicked() {
                                p.chosen = list.iter().map(|(f, ..)| f.clone()).collect();
                            }
                            if ui.button("Select none").clicked() {
                                p.chosen.clear();
                            }
                            // Kept whole: on a narrow window it moves under the buttons instead.
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{} of {} chosen",
                                        count(p.chosen.len()),
                                        files(list.len())
                                    ))
                                    .color(t.muted),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                        // Files that may not be worth restoring, one button per reason.
                        let flagged: Vec<_> = FLAGS
                            .iter()
                            .map(|&(bit, word)| {
                                // Only ticked ones: once unticked, the button goes away.
                                let n = list
                                    .iter()
                                    .zip(&p.flags)
                                    .filter(|((path, ..), f)| {
                                        *f & bit != 0 && p.chosen.contains(path)
                                    })
                                    .count();
                                (bit, word, n)
                            })
                            .filter(|f| f.2 > 0)
                            .collect();
                        if !flagged.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for (bit, word, n) in flagged {
                                    let label = match (word, n) {
                                        ("large", _) => format!("Untick {} over 1 GB", files(n)),
                                        ("duplicate", 1) => "Untick 1 duplicate".into(),
                                        ("duplicate", _) => {
                                            format!("Untick {} duplicates", count(n))
                                        }
                                        ("empty", 1) => "Untick 1 empty file".into(),
                                        ("empty", _) => format!("Untick {} empty files", count(n)),
                                        (_, _) => format!("Untick {} {word}", files(n)),
                                    };
                                    if ui.button(label).clicked() {
                                        for ((path, ..), f) in list.iter().zip(&p.flags) {
                                            if f & bit != 0 {
                                                p.chosen.remove(path);
                                            }
                                        }
                                    }
                                }
                            });
                        }
                        ui.add_space(4.0);
                        let files_root = p.folder.join("files");
                        // The room left in the dialog's body, so only the list scrolls, not the body too.
                        let height = ui.available_height().clamp(160.0, 480.0);
                        // Only the rows in view are built, so a backup of 100,000 files stays quick.
                        // Nested in the dialog's scroll area: without the minimum it would shrink.
                        egui::ScrollArea::vertical()
                            .max_height(height)
                            .min_scrolled_height(height.min(40.0 * list.len() as f32))
                            .auto_shrink([false, true])
                            .show_rows(ui, 40.0, list.len(), |ui, rows| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                for ((path, bytes, _), flag) in
                                    list[rows.clone()].iter().zip(&p.flags[rows])
                                {
                                    ui.horizontal(|ui| {
                                        let (tile, _) = ui.allocate_exact_size(
                                            vec2(40.0, 40.0),
                                            egui::Sense::hover(),
                                        );
                                        match p.thumbs.get(path) {
                                            Some(Some(tex)) => {
                                                egui::Image::new(tex)
                                                    .corner_radius(4)
                                                    .paint_at(ui, tile);
                                            }
                                            known => {
                                                if known.is_none() {
                                                    wanted.push(path.clone());
                                                }
                                                // Not a picture, or not ready yet: its type instead.
                                                ui.painter().rect(
                                                    tile,
                                                    4,
                                                    t.bg,
                                                    Stroke::new(1.0, t.line),
                                                    StrokeKind::Inside,
                                                );
                                                let ext: String = path
                                                    .extension()
                                                    .map(|e| e.to_string_lossy().to_uppercase())
                                                    .unwrap_or_default()
                                                    .chars()
                                                    .take(4)
                                                    .collect();
                                                ui.painter().text(
                                                    tile.center(),
                                                    Align2::CENTER_CENTER,
                                                    ext,
                                                    FontId::proportional(12.0),
                                                    t.muted,
                                                );
                                            }
                                        }
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().interact_size.y = 20.0;
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            let name = path
                                                .file_name()
                                                .map(|n| n.to_string_lossy().into_owned())
                                                .unwrap_or_default();
                                            let mut on = p.chosen.contains(path);
                                            if ui.checkbox(&mut on, name).changed() {
                                                if on {
                                                    p.chosen.insert(path.clone());
                                                } else {
                                                    p.chosen.remove(path);
                                                }
                                            }
                                            let dir = path.parent().unwrap_or(path);
                                            let dir = dir
                                                .strip_prefix(&files_root)
                                                .or_else(|_| dir.strip_prefix(&p.folder))
                                                .unwrap_or(dir);
                                            ui.horizontal(|ui| {
                                                ui.add_space(
                                                    ui.spacing().icon_width
                                                        + ui.spacing().icon_spacing,
                                                );
                                                // Why it may not be worth restoring, before the folder so it is never cut.
                                                let why: Vec<_> = FLAGS
                                                    .iter()
                                                    .filter(|(bit, _)| flag & bit != 0)
                                                    .map(|(_, word)| *word)
                                                    .collect();
                                                if !why.is_empty() {
                                                    let why = why.join(", ");
                                                    let mut why =
                                                        why[..1].to_uppercase() + &why[1..];
                                                    why.push('.');
                                                    ui.label(RichText::new(why).size(12.0));
                                                }
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{} in {}",
                                                        size(*bytes),
                                                        dir.display()
                                                    ))
                                                    .color(t.muted)
                                                    .size(12.0),
                                                );
                                            });
                                        });
                                    });
                                }
                            });
                    }
                }
            },
        );
        if wanted != p.asked {
            let _ = p.ask.send(wanted.clone());
            p.asked = wanted;
        }
        // Esc or a click outside keeps the choice too: nothing has changed on the phone yet.
        if close
            && let Some(p) = self.picker.take()
            && let Some(Ok(list)) = p.files
        {
            if p.delete {
                self.free_pick = Some((p.folder, p.chosen));
                return;
            }
            self.only =
                (p.chosen.len() < list.len()).then(|| (p.folder, Arc::new(p.chosen), list.len()));
        }
    }

    /// Chooses a backup. Results about the one chosen before must not show for, or act on, this one.
    pub(crate) fn choose(&mut self, path: Option<std::path::PathBuf>) {
        if self.chosen != path {
            self.chosen = path;
            self.dupes = None;
            self.tool_msg = None;
            self.export_msg = None;
            self.restore_msg = None;
        }
    }

    /// Re-reads the backups in `folder` and keeps the choice when it is still there.
    pub(crate) fn scan(&mut self, folder: &std::path::Path) {
        if self.fixture != Fixture::Real {
            return;
        }
        self.backups = backup::saved_backups(folder);
        self.scanned = folder.to_path_buf();
        let (tx, paths): (_, Vec<PathBuf>) = (
            self.tx.clone(),
            self.backups.iter().map(|b| b.path.clone()).collect(),
        );
        // Adding up 100,000 files takes seconds, so off the window's thread. The phone check
        // repaints every 2 seconds, which shows the sizes.
        thread::spawn(move || {
            let sizes = paths
                .into_iter()
                .map(|p| {
                    let n = backup::folder_size(&p);
                    (p, n)
                })
                .collect();
            let _ = tx.send(Msg::FolderSizes(sizes));
        });
        if !self
            .backups
            .iter()
            .any(|b| Some(&b.path) == self.chosen.as_ref())
        {
            self.choose(self.backups.first().map(|b| b.path.clone()));
        }
    }
}

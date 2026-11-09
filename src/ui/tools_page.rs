use crate::*;
use egui::pos2;

struct Tool {
    /// What the tool's jobs and results are filed under.
    id: &'static str,
    short: &'static str,
    name: &'static str,
    note: &'static str,
    glyph: &'static str,
    safe: &'static str,
    input: (&'static str, &'static str),
    output: (&'static str, &'static str),
}

const ICON_PHONE: &str = "\u{E8EA}";

const TOOLS: [Tool; 6] = [
    Tool {
        id: "read",
        short: "Read",
        name: "Read this backup",
        note: "See its messages, contacts, call history and calendar here, without a phone.",
        glyph: "\u{E736}",
        safe: "",
        input: ("\u{E8B7}", "Your backup"),
        output: ("\u{E736}", "Read it here"),
    },
    Tool {
        id: "dupes",
        short: "Find copies",
        name: "Duplicate files",
        note: "Finds photos and files saved more than once in a backup, such as pictures shared in several chats.",
        glyph: "\u{E8C8}",
        safe: "Nothing is deleted until you confirm.",
        input: ("\u{E8C8}", "Files in your backup"),
        output: ("\u{E9D5}", "A list of the copies"),
    },
    Tool {
        id: "heic",
        short: "HEIC to JPG",
        name: "HEIC photos to JPG",
        note: "Makes a JPG copy of each photo, with its date and location, for Windows apps that cannot open HEIC.",
        glyph: "\u{E8B9}",
        safe: "Your HEIC photos stay as they are.",
        input: ("\u{E8B9}", "HEIC photos"),
        output: ("\u{EB9F}", "JPG copies"),
    },
    Tool {
        id: "contacts",
        short: "Fix contacts",
        name: "Duplicate contacts",
        note: "Merges contacts with the same name, number or email into a new file.",
        glyph: "\u{E77B}",
        safe: "contacts.vcf stays as it is.",
        input: ("\u{E77B}", "contacts.vcf"),
        output: ("\u{E716}", "One card per person"),
    },
    Tool {
        id: "phone",
        short: "Free up space",
        name: "Free up space on the phone",
        note: "Deletes files from the phone that are safe in a backup: only those whose backup copy has exactly the same contents.",
        glyph: ICON_PHONE,
        safe: "This changes the phone. Nothing goes until you confirm.",
        input: (ICON_PHONE, "Files on the phone"),
        output: ("\u{E74D}", "Space back"),
    },
    Tool {
        id: "scrcpy",
        short: "See screen",
        name: "See the phone's screen",
        note: "Shows and controls the phone from this PC, using scrcpy, a free app by Genymobile.",
        glyph: "\u{E7F4}",
        safe: "",
        input: (ICON_PHONE, "The phone's screen"),
        output: ("\u{E7F4}", "A window on this PC"),
    },
];

/// Tools are separate small jobs, not a copy, so no cable: picture buttons across the top, then
/// what goes in and what comes out.
impl App {
    pub(crate) fn adb_serial(&self) -> Option<String> {
        self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, .. } => Some(serial.clone()),
            _ => None,
        })
    }

    /// Deleting must never pick one of several phones by guesswork.
    fn many_phones(&self) -> bool {
        self.phones
            .iter()
            .flatten()
            .filter(|p| matches!(p, Phone::Adb { .. }))
            .count()
            > 1
    }

    /// Runs a tool on a worker thread; `which` is the panel that shows its progress and result.
    pub(crate) fn run_tool(
        &mut self,
        ctx: &egui::Context,
        which: &'static str,
        job: impl FnOnce(&dyn Fn(String)) -> tools::Done + Send + 'static,
    ) {
        tools::STOP.store(false, Ordering::Relaxed);
        self.tool_busy = Some(which);
        self.tool_status = "Working…".into();
        self.tool_msg = None;
        let (tx, ctx) = (self.tool_tx.clone(), ctx.clone());
        thread::spawn(move || {
            let say = |s: String| {
                let _ = tx.send(tools::Done::Status(s));
                ctx.request_repaint();
            };
            let done =
                crate::jobs::caught(|| job(&say)).unwrap_or_else(|e| tools::Done::Text(Err(e)));
            let _ = tx.send(done);
            ctx.request_repaint();
        });
    }

    /// Lists the groups found; every copy but the first of each group starts ticked.
    pub(crate) fn show_dupes(&mut self, groups: Vec<Vec<(PathBuf, u64)>>) {
        self.ticked = groups
            .iter()
            .flat_map(|g| g.iter().skip(1).map(|(p, _)| p.clone()))
            .collect();
        let rows = groups
            .iter()
            .enumerate()
            .flat_map(|(i, g)| {
                std::iter::once((i, None)).chain((0..g.len()).map(move |f| (i, Some(f))))
            })
            .collect();
        self.dupes = Some((groups, rows));
    }

    /// What this tool's last job said, while it stands for the chosen backup.
    fn tool_result(&self, id: &str) -> Option<&Result<String, String>> {
        self.tool_msg
            .as_ref()
            .filter(|(w, _)| *w == id)
            .map(|(_, r)| r)
    }

    pub(crate) fn tools_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let width = ui.available_width();
        let locked = self.tool_busy.is_some();
        let cols = if width >= 672.0 { 6.0 } else { 3.0 };
        let each = (width - 8.0 * (cols - 1.0)) / cols - 0.5;
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for (i, k) in TOOLS.iter().enumerate() {
                if tool_tile(ui, t, k, self.tool == i, each, locked) {
                    self.tool = i;
                }
            }
        });
        ui.add_space(16.0);
        let top = ui.cursor().min;
        let height = (ui.clip_rect().bottom() - top.y - 14.0).max(240.0);
        let area = Rect::from_min_size(top, vec2(width, height));
        ui.allocate_rect(area, egui::Sense::hover());
        let (stage, detail) = if width >= 640.0 {
            let w = (width * 0.42).clamp(300.0, 640.0);
            (
                Rect::from_min_size(area.min, vec2(w, height.min(w * 0.72))),
                Rect::from_min_max(pos2(area.left() + w + 36.0, area.top()), area.max),
            )
        } else {
            // In a short window the words and the button matter more than the picture.
            let h = if height < 320.0 { 0.0 } else { 150.0 };
            (
                Rect::from_min_size(area.min, vec2(width, (h - 14.0f32).max(0.0))),
                Rect::from_min_max(pos2(area.left(), area.top() + h), area.max),
            )
        };
        if stage.height() > 0.0 {
            self.tool_stage(ui, t, stage);
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(detail), |ui| {
            own_scroll(ui, "tool-detail", detail.height(), |ui| {
                self.tool_detail(ui, t)
            });
        });
    }

    /// What goes in and what comes out, in pictures, with work moving between them while it runs.
    fn tool_stage(&self, ui: &egui::Ui, t: &Tokens, r: Rect) {
        let k = &TOOLS[self.tool];
        let ctx = ui.ctx();
        // Fades in when you pick another tool, so the change shows.
        let secs = if self.motion { 0.25 } else { 0.0 };
        let shown = ctx.animate_value_with_time(egui::Id::unique("stage"), self.tool as f32, secs);
        let a = 1.0 - (shown - self.tool as f32).abs().min(1.0);
        let p = ui.painter();
        p.rect(
            r,
            RADIUS,
            t.surface,
            Stroke::new(1.0, t.line),
            StrokeKind::Inside,
        );
        let size = (r.height() * 0.3).clamp(28.0, 60.0);
        let y = r.center().y - 12.0;
        let (lx, rx) = (r.left() + r.width() * 0.24, r.right() - r.width() * 0.24);
        let done = matches!(self.tool_result(k.id), Some(Ok(_)))
            || (k.id == "dupes" && self.dupes.as_ref().is_some_and(|d| !d.0.is_empty()));
        let seal = ctx.animate_bool_with_easing(
            egui::Id::unique("stage-seal"),
            done,
            egui::emath::easing::cubic_out,
        );
        icon(ui, pos2(lx, y), k.input.0, size, t.ink.gamma_multiply(a));
        let out = mix(t.muted, t.accent, seal).gamma_multiply(a);
        icon(ui, pos2(rx, y), k.output.0, size, out);
        let label_w = r.width() * 0.44;
        for (x, word) in [(lx, k.input.1), (rx, k.output.1)] {
            put(
                ui,
                pos2(x, y + size * 0.5 + 12.0),
                Align2::CENTER_TOP,
                txt(word, 12.0, 400.0, t.ink.gamma_multiply(a)),
                label_w,
            );
        }
        let (x0, x1) = (lx + size * 0.62, rx - size * 0.62);
        p.hline(x0..=x1, y, Stroke::new(2.0, t.line));
        if self.tool_busy == Some(k.id) && self.motion {
            let time = ctx.input(|i| i.time) as f32;
            let gap = (x1 - x0) / 3.0;
            for n in 0..3 {
                let x = (time * 70.0 + n as f32 * gap).rem_euclid(x1 - x0);
                p.rect_filled(
                    Rect::from_center_size(pos2(x0 + x, y), vec2(8.0, 10.0)),
                    2,
                    t.accent,
                );
            }
            ctx.request_repaint();
        } else {
            icon(ui, pos2((x0 + x1) / 2.0, y), "\u{E76C}", 14.0, out);
        }
        badge(ui, t, pos2(rx + size * 0.45, y + size * 0.4), seal);
    }

    fn tool_detail(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let k = &TOOLS[self.tool];
        let wrapped = |ui: &mut egui::Ui, text: RichText| {
            ui.add(egui::Label::new(text).wrap());
        };
        let running = self.tool_busy.is_some();
        let full = ui.available_width().min(680.0);
        ui.set_max_width(full);
        wrapped(ui, txt(k.name, 18.0, 600.0, t.ink));
        ui.add_space(4.0);
        wrapped(ui, txt(k.note, 13.0, 400.0, t.ink));
        if !k.safe.is_empty() {
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 20.0), egui::Sense::hover());
                icon(
                    ui,
                    r.left_center() + vec2(8.0, 0.0),
                    "\u{EA18}",
                    15.0,
                    t.accent,
                );
                ui.scope(|ui| wrapped(ui, txt(k.safe, 12.0, 400.0, t.muted)));
            });
        }
        ui.add_space(14.0);

        let serial = self.adb_serial();
        let (row, _) = ui.allocate_exact_size(vec2(full, HIT), egui::Sense::hover());
        let y = row.center().y;
        let word = if k.id == "scrcpy" { "Phone" } else { "Backup" };
        put(
            ui,
            pos2(row.left(), y),
            Align2::LEFT_CENTER,
            txt(word, 12.0, 400.0, t.muted),
            64.0,
        );
        let at = pos2(row.left() + 64.0, y - 10.0);
        if k.id == "scrcpy" {
            let name = self.adb_phone().map_or("No phone yet".into(), |p| p.1);
            put(
                ui,
                at,
                Align2::LEFT_TOP,
                txt(&name, 13.0, 600.0, t.ink),
                full - 64.0,
            );
        } else {
            let options: Vec<String> = self
                .backups
                .iter()
                .map(|b| {
                    format!(
                        "{}, {}",
                        Self::label(&self.backups, b),
                        days_ago(b.age_days)
                    )
                })
                .collect();
            let name = self
                .backups
                .iter()
                .position(|b| Some(&b.path) == self.chosen.as_ref())
                .map_or("Choose a backup".to_string(), |i| options[i].clone());
            // A narrow page gives the name the whole row, with Find another under it.
            let beside = full >= 480.0;
            let name_w = full - 64.0 - if beside { 132.0 } else { 0.0 };
            let room = (name_w, !running && !options.is_empty());
            if let Some(n) = chooser(
                ui,
                t,
                at,
                &name,
                (14.0, room.0),
                room.1,
                "tool-backup",
                &options,
            ) {
                self.choose(Some(self.backups[n].path.clone()));
            }
            let end = Rect::from_min_max(pos2(row.right() - 124.0, row.top()), row.max);
            let find = if beside {
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(end)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| quiet(ui, t, "Find another…", !running),
                )
                .inner
            } else {
                quiet(ui, t, "Find another…", !running)
            };
            if find.clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_directory(&self.dest)
                    .pick_folder()
            {
                self.choose(Some(dir.clone()));
                self.scan(&dir);
            }
            if self.backups.is_empty() {
                wrapped(
                    ui,
                    txt(
                        &format!("No backups in {} yet.", self.scanned.display()),
                        12.0,
                        400.0,
                        t.muted,
                    ),
                );
            }
        }
        ui.add_space(10.0);

        let folder = self.chosen.clone().filter(|_| !self.backups.is_empty());
        let can = folder.is_some() && !running;
        let many = self.many_phones();
        let hint = |ui: &mut egui::Ui, title: &str, note: &str| {
            if !title.is_empty() {
                wrapped(ui, txt(title, 13.0, 600.0, t.ink));
            }
            if !note.is_empty() {
                wrapped(ui, txt(note, 12.0, 400.0, t.muted));
            }
        };
        let chosen = self
            .free_pick
            .as_ref()
            .filter(|(f, _)| Some(f) == folder.as_ref())
            .map(|(_, c)| c.clone())
            .unwrap_or_default();
        if self.tool_busy == Some(k.id) {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.add(egui::Label::new(txt(&self.tool_status, 13.0, 600.0, t.ink)).wrap());
            });
        } else if let Some(r) = self.tool_result(k.id) {
            match r {
                // Its first sentence is the news; the rest explains.
                Ok(m) => {
                    let (head, rest) = m.split_once(". ").unwrap_or((m, ""));
                    wrapped(ui, txt(head.trim_end_matches('.'), 18.0, 600.0, t.accent));
                    wrapped(ui, txt(rest, 13.0, 400.0, t.ink));
                }
                Err(e) => wrapped(ui, txt(e, 13.0, 400.0, t.error)),
            }
        } else {
            match k.id {
                "scrcpy" if self.scrcpy.is_none() => hint(
                    ui,
                    "scrcpy is not on this computer",
                    "Unzip it next to this app to use it from here.",
                ),
                "scrcpy" if serial.is_none() => {
                    hint(ui, "Connect the phone with USB debugging on", "")
                }
                "scrcpy" => hint(ui, "Opens in its own window", ""),
                _ if folder.is_none() => hint(ui, "Choose a backup first", ""),
                "phone" if chosen.is_empty() => hint(ui, "Choose what to delete", ""),
                "phone" if serial.is_none() => hint(
                    ui,
                    &format!("{} chosen", files(chosen.len())),
                    "Connect the phone with USB debugging on.",
                ),
                "phone" if many => hint(
                    ui,
                    &format!("{} chosen", files(chosen.len())),
                    "Unplug the other phones first, so only the phone to free is connected.",
                ),
                "phone" => hint(
                    ui,
                    &format!("{} chosen", files(chosen.len())),
                    "Their copies stay in the backup.",
                ),
                _ => {}
            }
        }
        if k.id == "dupes" {
            self.dupes_list(ui, t, folder.as_deref(), running);
        }
        ui.add_space(12.0);

        ui.horizontal_wrapped(|ui| {
            if self.tool_busy == Some(k.id) {
                let stopping = tools::STOP.load(Ordering::Relaxed);
                let label = if stopping { "Stopping…" } else { "Stop" };
                // Only these two look at tools::STOP; the others end on their own.
                if quiet(ui, t, label, !stopping && matches!(k.id, "dupes" | "heic")).clicked() {
                    self.stop_tool = true;
                }
                return;
            }
            match k.id {
                "read" => {
                    if primary(ui, t, "Open", can)
                        && let Some(f) = folder.clone()
                    {
                        self.run_tool(ui.ctx(), "read", move |say| {
                            say("Reading the backup…".into());
                            tools::Done::Book(export::book(&f))
                        });
                    }
                }
                "dupes" => self.dupes_buttons(ui, t, folder.clone(), can),
                "heic" => {
                    let done = matches!(self.tool_result("heic"), Some(Ok(_)));
                    if done
                        && let Some(f) = &folder
                        && primary(ui, t, "Open jpg folder", true)
                    {
                        let _ = backup::explorer().arg(f.join("jpg")).spawn();
                    }
                    let start = if done {
                        quiet(ui, t, "Convert to JPG", can).clicked()
                    } else {
                        primary(ui, t, "Convert to JPG", can)
                    };
                    if start && let Some(f) = folder.clone() {
                        self.run_tool(ui.ctx(), "heic", move |say| {
                            tools::Done::Text(tools::heic_to_jpg(&f, say))
                        });
                    }
                }
                "contacts" => {
                    // Undoing leaves a message too, and there is then nothing to undo.
                    let merged = matches!(
                        self.tool_result("contacts"),
                        Some(Ok(m)) if !m.starts_with("Removed")
                    );
                    if merged && let Some(f) = &folder {
                        let data = f.join("data");
                        if primary(ui, t, "Undo", true) {
                            self.tool_msg = Some((
                                "contacts",
                                match fs::remove_file(data.join(tools::MERGED)) {
                                    Ok(()) => Ok(format!(
                                        "Removed {}. contacts.vcf was never changed.",
                                        tools::MERGED
                                    )),
                                    Err(e) => Err(format!("Cannot remove {}: {e}", tools::MERGED)),
                                },
                            ));
                        }
                        if quiet(ui, t, "Open folder", true).clicked() {
                            let _ = backup::explorer().arg(&data).spawn();
                        }
                    } else if primary(ui, t, "Merge duplicates", can)
                        && let Some(f) = folder.clone()
                    {
                        self.run_tool(ui.ctx(), "contacts", move |_| {
                            tools::Done::Text(tools::merge_contacts(&f))
                        });
                    }
                }
                "phone" => {
                    let ready = can && serial.is_some() && !many && !chosen.is_empty();
                    if primary(ui, t, "Delete from phone…", ready)
                        && let (Some(f), Some(serial)) = (folder.clone(), serial.clone())
                    {
                        self.run_tool(ui.ctx(), "phone", move |say| {
                            say("Checking the files on the phone…".into());
                            tools::Done::Matched(
                                serial.clone(),
                                tools::chosen_on_phone(&f, &chosen)
                                    .and_then(|wanted| tools::match_on_phone(&serial, &wanted)),
                            )
                        });
                    }
                    if quiet(ui, t, "Choose files…", can).clicked()
                        && let Some(f) = folder.clone()
                    {
                        self.open_picker(ui.ctx(), f, true);
                    }
                }
                _ => match &self.scrcpy {
                    Some(exe) => {
                        if primary(ui, t, "Open scrcpy", serial.is_some()) {
                            let mut c = Command::new(exe);
                            c.args(["-s", serial.as_deref().unwrap_or_default()]);
                            // One adb for both apps: two different versions stop each other's server.
                            if let Some(adb) = backup::adb_path() {
                                c.env("ADB", adb);
                            }
                            #[cfg(windows)]
                            std::os::windows::process::CommandExt::creation_flags(
                                &mut c,
                                0x0800_0000,
                            );
                            self.tool_msg = c
                                .spawn()
                                .err()
                                .map(|e| ("scrcpy", Err(format!("scrcpy did not start: {e}"))));
                        }
                    }
                    None => {
                        ui.hyperlink_to(
                            "Get scrcpy on GitHub",
                            "https://github.com/Genymobile/scrcpy",
                        );
                    }
                },
            }
        });
    }

    /// The copies found, with a tick on each one to delete.
    fn dupes_list(&mut self, ui: &mut egui::Ui, t: &Tokens, folder: Option<&Path>, running: bool) {
        let Some((groups, rows)) = &self.dupes else {
            return;
        };
        let wrapped = |ui: &mut egui::Ui, text: RichText| {
            ui.add(egui::Label::new(text).wrap());
        };
        if groups.is_empty() {
            wrapped(ui, txt("No duplicates found", 18.0, 600.0, t.accent));
            return;
        }
        let (n, bytes) = groups
            .iter()
            .flatten()
            .filter(|(p, _)| self.ticked.contains(p))
            .fold((0, 0), |(n, b), (_, s)| (n + 1, b + s));
        wrapped(
            ui,
            txt(
                &format!("{} files saved more than once", count(groups.len())),
                18.0,
                600.0,
                t.accent,
            ),
        );
        wrapped(
            ui,
            txt(
                &format!(
                    "{} ticked to delete ({}); one copy of each stays.",
                    count(n),
                    size(bytes)
                ),
                13.0,
                400.0,
                t.ink,
            ),
        );
        // Rows of 22 px with 4 px between; show_rows takes the height without the gap.
        const ROW: f32 = 26.0;
        let height = (rows.len() as f32 * ROW - 4.0).min(320.0);
        egui::ScrollArea::vertical()
            .id_salt("dupes")
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, true])
            .show_rows(ui, ROW - 4.0, rows.len(), |ui, range| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.spacing_mut().interact_size.y = ROW - 4.0;
                ui.spacing_mut().item_spacing.y = 4.0;
                for &(g, f) in &rows[range] {
                    let group = &groups[g];
                    match f {
                        None => {
                            ui.label(
                                RichText::new(format!(
                                    "{} copies, {} each",
                                    group.len(),
                                    size(group[0].1)
                                ))
                                .font(semibold(14.0)),
                            );
                        }
                        Some(f) => {
                            let path = &group[f].0;
                            let shown = path.strip_prefix(folder.unwrap_or(path)).unwrap_or(path);
                            let mut on = self.ticked.contains(path);
                            if ui
                                .add_enabled(
                                    !running,
                                    egui::Checkbox::new(&mut on, shown.display().to_string()),
                                )
                                .on_hover_text(path.display().to_string())
                                .changed()
                            {
                                if on {
                                    self.ticked.insert(path.clone());
                                } else {
                                    self.ticked.remove(path);
                                }
                            }
                        }
                    }
                }
            });
        // Every copy of a file ticked would delete it from the backup altogether.
        if groups
            .iter()
            .any(|g| g.iter().all(|(p, _)| self.ticked.contains(p)))
        {
            wrapped(
                ui,
                txt(
                    "Leave at least one copy of each file unticked.",
                    12.0,
                    400.0,
                    t.error,
                ),
            );
        }
    }

    fn dupes_buttons(&mut self, ui: &mut egui::Ui, t: &Tokens, folder: Option<PathBuf>, can: bool) {
        let found: Vec<(PathBuf, u64)> = self
            .dupes
            .iter()
            .flat_map(|(groups, _)| groups.iter().flatten())
            .filter(|(p, _)| self.ticked.contains(p))
            .cloned()
            .collect();
        let any = self.dupes.as_ref().is_some_and(|d| !d.0.is_empty());
        let start = if any {
            let all_gone = self.dupes.iter().any(|(groups, _)| {
                groups
                    .iter()
                    .any(|g| g.iter().all(|(p, _)| self.ticked.contains(p)))
            });
            if primary(
                ui,
                t,
                "Delete ticked copies…",
                can && !found.is_empty() && !all_gone,
            ) {
                self.confirm = Some(Confirm::Copies(found));
            }
            quiet(ui, t, "Find again", can).clicked()
        } else {
            primary(ui, t, "Find duplicates", can)
        };
        if start && let Some(f) = folder {
            self.dupes = None;
            self.run_tool(ui.ctx(), "dupes", move |say| {
                tools::Done::Dupes(tools::duplicates(&f, say))
            });
        }
    }

    /// Asks before a tool deletes anything; Esc or a click outside keeps the files.
    /// Asks before a stray click on Stop ends a running tool.
    pub(crate) fn stop_tool_dialog(&mut self, ctx: &egui::Context) {
        let (title, body) = match self.tool_busy {
            _ if !self.stop_tool => return,
            Some("dupes") => (
                "Stop looking for copies?",
                "Nothing in the backup has changed.",
            ),
            Some("heic") => (
                "Stop making JPG copies?",
                "Copies made so far are kept. The HEIC photos are not changed.",
            ),
            // The tool ended while this was being asked: nothing is left to stop.
            _ => {
                self.stop_tool = false;
                return;
            }
        };
        if let Some(yes) = confirm(ctx, title, "Stop", true, |ui| {
            ui.add(egui::Label::new(body).wrap());
        }) {
            if yes {
                tools::STOP.store(true, Ordering::Relaxed);
            }
            self.stop_tool = false;
        }
    }

    pub(crate) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let (title, body, yes) = match c {
            Confirm::Copies(list) => (
                format!("Delete {} from the backup?", files(list.len())),
                format!(
                    "This frees {} on this computer. One copy of each file stays in the backup. While the phone still has these files, the next backup copies them again.",
                    size(list.iter().map(|(_, n)| n).sum())
                ),
                format!("Delete {}", files(list.len())),
            ),
            Confirm::Phone(_, list, left) => {
                let mut body = format!(
                    "This frees {} on the phone. Their copies in the backup are exactly the same and stay on this computer. Deleted files cannot be brought back on the phone except by restoring them.",
                    size(list.iter().map(|(_, n, _)| n).sum())
                );
                if *left > 0 {
                    body += &format!(
                        " {} chosen are left alone: they are not on the phone or differ from the backup.",
                        files(*left)
                    );
                }
                (
                    format!("Delete {} from the phone?", files(list.len())),
                    body,
                    format!("Delete {}", files(list.len())),
                )
            }
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm_delete"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width().min(360.0));
                ui.label(RichText::new(title).font(semibold(16.0)));
                ui.label(body);
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(yes).clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Keep them").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if modal.should_close() {
            answer = Some(false);
        }
        let Some(yes) = answer else { return };
        match self.confirm.take() {
            Some(Confirm::Copies(list)) if yes => {
                self.dupes = None;
                // Copies are only listed for the chosen backup.
                let folder = self.chosen.clone().unwrap_or_default();
                self.run_tool(ctx, "dupes", move |_| {
                    tools::Done::Text(tools::delete_copies(&folder, &list))
                });
            }
            Some(Confirm::Phone(..)) if yes && self.many_phones() => {
                self.tool_msg = Some((
                    "phone",
                    Err("Another phone was connected, so nothing was deleted. Unplug the other phones and try again.".into()),
                ));
            }
            Some(Confirm::Phone(serial, list, _)) if yes => {
                self.free_pick = None;
                self.run_tool(ctx, "phone", move |say| {
                    say("Deleting from the phone…".into());
                    tools::Done::Text(tools::delete_from_phone(&serial, &list))
                });
            }
            _ => {}
        }
    }
}

/// One tool as a picture button.
fn tool_tile(ui: &mut egui::Ui, t: &Tokens, k: &Tool, on: bool, w: f32, locked: bool) -> bool {
    let sense = if locked {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (rect, r) = ui.allocate_exact_size(vec2(w, 64.0), sense);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, !locked, on, k.name));
    let edge = if on {
        Stroke::new(2.0, t.accent)
    } else if r.hovered() && !locked {
        Stroke::new(1.0, t.border)
    } else {
        Stroke::new(1.0, t.line)
    };
    ui.painter()
        .rect(rect, RADIUS, t.surface, edge, StrokeKind::Inside);
    let ink = if on { t.accent } else { t.ink };
    icon(
        ui,
        pos2(rect.center().x, rect.top() + 23.0),
        k.glyph,
        20.0,
        ink,
    );
    let weight = if on { 600.0 } else { 400.0 };
    put(
        ui,
        pos2(rect.center().x, rect.top() + 40.0),
        Align2::CENTER_TOP,
        txt(k.short, 12.0, weight, t.ink),
        w - 8.0,
    );
    ring(ui, &r, rect, RADIUS, t);
    r.clicked()
}

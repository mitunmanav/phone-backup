use super::backup_tab::{Cord, Phase, finished_with};
use crate::*;

/// "Today", "3 days ago": when a backup was made, to start a line.
fn saved_when(days: Option<u64>) -> String {
    let when = days_ago(days);
    let mut letters = when.chars();
    letters.next().map_or(String::new(), |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// One saved backup, drawn as a folder. The chosen one opens to show what you can do with it, so
/// nothing hides in a menu. Returns if it was pressed, the action pressed, and where its middle is.
#[allow(clippy::too_many_arguments)]
fn backup_card(
    ui: &mut egui::Ui,
    t: &Tokens,
    b: &backup::Saved,
    label: &str,
    on: bool,
    locked: bool,
    many: bool,
    motion: bool,
) -> (bool, Option<usize>, f32) {
    const ACTIONS: [(&str, &str, &str); 7] = [
        ("\u{E838}", "Open", "Opens the backup's folder."),
        (
            "\u{E8AC}",
            "Rename",
            "Gives the backup's folder a new name.",
        ),
        (
            "\u{E9D5}",
            "Verify",
            "Checks every file against its checksum. Nothing changes.",
        ),
        (
            "\u{EDE1}",
            "Export",
            "Saves contacts, calls and messages as files you can open, print or import, and sorts photos into year and month folders. The backup itself is not changed.",
        ),
        (
            "\u{E8C8}",
            "Copy",
            "Copies this backup's files into another backup.",
        ),
        (
            "\u{E72E}",
            "Lock",
            "Protects the backup with a password, or changes or removes its password.",
        ),
        ("\u{E74D}", "Delete", "Moves the backup to the Recycle Bin."),
    ];
    let w = ui.available_width();
    let id = egui::Id::unique(("backup", &b.path));
    let open = if motion {
        ui.ctx()
            .animate_bool_with_easing(id.with("open"), on, egui::emath::easing::cubic_out)
    } else {
        on as u8 as f32
    };
    let (rect, _) = ui.allocate_exact_size(vec2(w, 56.0 + 46.0 * open), egui::Sense::hover());
    let top = Rect::from_min_size(rect.min, vec2(w, 56.0));
    let sense = if locked {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let r = ui.interact(top, id, sense);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, !locked, on, label));
    let edge = if on {
        Stroke::new(2.0, t.accent)
    } else if r.hovered() && !locked {
        Stroke::new(1.0, t.border)
    } else {
        Stroke::new(1.0, t.line)
    };
    ui.painter()
        .rect(rect, 6, t.surface, edge, StrokeKind::Inside);
    ring(ui, &r, rect, 6, t);
    let folder_ink = if on { t.accent } else { t.muted };
    icon(
        ui,
        egui::pos2(rect.left() + 26.0, top.center().y),
        ICON_FOLDER,
        24.0,
        folder_ink,
    );
    let size_at = egui::pos2(rect.right() - 14.0, top.top() + 11.0);
    let bytes = b.bytes.map_or(String::new(), size);
    let shown = put(
        ui,
        size_at,
        Align2::RIGHT_TOP,
        txt(&bytes, 13.0, 400.0, t.ink),
        80.0,
    );
    let tx = rect.left() + 50.0;
    let name_at = egui::pos2(tx, top.top() + 11.0);
    put(
        ui,
        name_at,
        Align2::LEFT_TOP,
        txt(label, 13.0, 600.0, t.ink),
        shown.left() - tx - 10.0,
    );
    let holds = match (b.files, b.apps) {
        (true, true) => "Photos, files and apps",
        (true, false) => "Photos and files",
        _ => "Apps",
    };
    let lock = if b.password { " · Password" } else { "" };
    let line = format!("{}: {holds}{lock}", saved_when(b.age_days));
    let line_at = egui::pos2(tx, top.top() + 30.0);
    put(
        ui,
        line_at,
        Align2::LEFT_TOP,
        txt(&line, 12.0, 400.0, t.muted),
        rect.right() - tx - 14.0,
    );
    let mut action = None;
    if open > 0.5 {
        let fade = (open - 0.5) * 2.0;
        let cell_w = (w - 12.0) / ACTIONS.len() as f32;
        for (k, (glyph, name, hint)) in ACTIONS.iter().enumerate() {
            // Copying needs another backup to copy into.
            let live = !locked && (k != 4 || many);
            let cell = Rect::from_min_size(
                rect.min + vec2(6.0 + k as f32 * cell_w, 54.0),
                vec2(cell_w, 46.0),
            );
            let sense = if live {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            };
            let a = ui.interact(cell, id.with(k), sense).on_hover_text(*hint);
            a.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, live, *name));
            if a.hovered() && live {
                ui.painter().rect_filled(cell, 6, t.hover);
            }
            ring(ui, &a, cell, 6, t);
            let ink = if live { t.ink } else { t.muted }.gamma_multiply(fade);
            icon(ui, cell.center_top() + vec2(0.0, 15.0), glyph, 14.0, ink);
            // Narrow windows get smaller words, so none is cut short.
            let words = if cell_w < 50.0 { 11.0 } else { 12.0 };
            put(
                ui,
                cell.center_top() + vec2(0.0, 25.0),
                Align2::CENTER_TOP,
                txt(name, words, 400.0, ink),
                cell_w - 2.0,
            );
            if a.clicked() {
                action = Some(k);
            }
        }
    }
    (r.clicked(), action, top.center().y)
}

const ICON_FOLDER: &str = "\u{E8B7}";

/// The Versions dialog: the latest and every older run, newest first, each with what it changed.
/// A run that kept replaced copies offers to show them.
fn history_list(
    ui: &mut egui::Ui,
    t: &Tokens,
    root: &Path,
    runs: &[backup::Run],
    pick: &mut Option<String>,
) {
    let muted = |s: &str| RichText::new(s).color(t.muted);
    ui.add(
        egui::Label::new(muted(
            "Each backup keeps the copies it replaced, so you can restore or read the backup as it was then. Files the phone already has are still never replaced.",
        ))
        .wrap(),
    );
    ui.add_space(6.0);
    if ui
        .radio(pick.is_none(), "Latest: everything in the backup now")
        .clicked()
    {
        *pick = None;
    }
    if runs.is_empty() {
        ui.label(muted(
            "No older versions yet. Each backup from now on adds one.",
        ));
    }
    // The newest run is the latest itself, so it is listed for what it did but not picked.
    for (k, run) in runs.iter().rev().enumerate() {
        ui.add_space(4.0);
        let label = format!("{}, {}", run.title(), run.how);
        if k == 0 {
            ui.label(RichText::new(format!("{label} (latest)")).font(semibold(14.0)));
        } else if ui
            .radio(pick.as_deref() == Some(run.name.as_str()), label)
            .clicked()
        {
            *pick = Some(run.name.clone());
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_space(24.0);
            ui.label(muted(&run.said()));
            if !run.replaced.is_empty() && ui.link("Show replaced copies").clicked() {
                let dir = root.join(backup::HISTORY).join(&run.name);
                let _ = backup::explorer().arg(dir).spawn();
            }
        });
    }
}

impl App {
    /// How the running or last restore stands. A check is not a restore.
    pub(crate) fn restore_phase(&self) -> Phase {
        match &self.restore_msg {
            _ if self.busy.load(Ordering::Relaxed) => {
                if self.job == Job::Restore && self.checking.is_none() {
                    Phase::Running
                } else {
                    Phase::Idle
                }
            }
            None => Phase::Idle,
            Some(Ok((_, failed))) => Phase::Done(*failed),
            Some(Err(_)) => Phase::Stopped,
        }
    }

    /// What the main button would restore, while there is something to restore.
    fn restore_plan(&self) -> Option<Plan> {
        let phone = self.adb_phone()?;
        let chosen = self
            .backups
            .iter()
            .find(|b| Some(&b.path) == self.chosen.as_ref())?;
        let only = self
            .only
            .as_ref()
            .filter(|(folder, ..)| *folder == chosen.path)
            .map(|(_, picked, _)| picked.clone());
        // A choice the chosen backup has nothing for is ignored, not run.
        let files =
            self.restore_files && chosen.files && only.as_ref().is_none_or(|o| !o.is_empty());
        let apps = self.restore_apps && chosen.apps;
        (files || apps).then(|| {
            (
                phone.0,
                chosen.path.clone(),
                files,
                apps,
                only.filter(|_| files),
            )
        })
    }

    /// The page: the backups beside the phone when there is room, the cable across the top when not.
    pub(crate) fn restore_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let plan = self.restore_plan();
        let width = ui.available_width();
        if width >= 730.0 {
            self.restore_columns(ui, t, plan.as_ref());
        } else {
            self.restore_band(ui, t, plan.as_ref());
        }
        self.restore_cmd(plan)
    }

    fn restore_columns(&mut self, ui: &mut egui::Ui, t: &Tokens, plan: Option<&Plan>) {
        let top = ui.cursor().min;
        let height = (ui.clip_rect().bottom() - top.y - 14.0).max(240.0);
        let area = Rect::from_min_size(top, vec2(ui.available_width(), height));
        ui.allocate_rect(area, egui::Sense::hover());
        let gutter = (area.width() * 0.16).clamp(130.0, 280.0);
        let col = (area.width() - gutter) / 2.0;
        let list = Rect::from_min_size(area.min, vec2(col, height));
        let right = Rect::from_min_max(egui::pos2(area.right() - col, area.top()), area.max);
        let mut row = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            own_scroll(ui, "backups", height, |ui| row = self.backup_list(ui, t));
        });

        let phase = self.restore_phase();
        let phone = egui::pos2(right.left() + 11.0, area.top() + 22.0);
        let b = egui::pos2(right.left() - 16.0, phone.y);
        let connected = self.adb_phone().is_some();
        let (mut plug, mut seal) = (connected as u8 as f32, 0.0);
        if let Some(y) = row {
            let target = y.clamp(list.top() + 20.0, list.bottom() - 20.0);
            // The plug glides to the new folder, and the cable swings slack while it travels.
            let now = if self.motion {
                spring(ui.ctx(), egui::Id::unique("restore-plug"), target, 0.35)
            } else {
                target
            };
            let a = egui::pos2(list.right() + 16.0, now);
            let loose = egui::pos2(a.x + (b.x - a.x) * 0.58, b.y + 44.0);
            (plug, seal) = self.cable(
                ui,
                t,
                Cord {
                    id: "restore",
                    a,
                    b,
                    loose,
                    sag: 12.0 + (target - now).abs() * 0.5,
                    phase,
                    connected,
                    phone_at_b: true,
                    coil: None,
                },
            );
        }
        let ink = mix(mix(t.muted, t.ink, plug), t.accent, seal);
        let shown = self.adb_phone().map(|p| p.0);
        let (glyph, size) = self.device_icon(shown.as_deref(), 36.0);
        icon(ui, phone, glyph, size, ink);
        badge(ui, t, phone + vec2(12.0, 17.0), seal);
        let tx = right.left() + 36.0;
        // Leaves room for the percent or outcome at the right edge, when there is one.
        let max = right.width() - if phase == Phase::Idle { 36.0 } else { 136.0 };
        let (name, note) = self.restore_phone_words();
        let at = |dy: f32| egui::pos2(tx, area.top() + dy);
        put(
            ui,
            at(4.0),
            Align2::LEFT_TOP,
            txt(&name, 15.0, 600.0, t.ink),
            max,
        );
        put(
            ui,
            at(25.0),
            Align2::LEFT_TOP,
            txt(&note, 12.0, 400.0, t.muted),
            max,
        );
        let words = (
            egui::pos2(right.right(), area.top() + 2.0),
            Some(egui::pos2(right.right(), area.top() + 25.0)),
            Align2::RIGHT_TOP,
        );
        self.mid_words(ui, t, words, 18.0, phase, "Restored", !connected);

        let rows = Rect::from_min_max(egui::pos2(right.left(), area.top() + 54.0), right.max);
        let room = rows.height() - 2.0 * HIT - 16.0;
        ui.scope_builder(egui::UiBuilder::new().max_rect(rows), |ui| {
            own_scroll(ui, "restore-rows", rows.height(), |ui| {
                self.restore_rows(ui, t, room);
                self.restore_result(ui, t, plan);
            });
        });
    }

    fn restore_band(&mut self, ui: &mut egui::Ui, t: &Tokens, plan: Option<&Plan>) {
        let w = ui.available_width();
        let narrow = w < 560.0;
        let (cols, each) = tile_grid(w, 0.0, 4);
        let block = 4usize.div_ceil(cols) as f32 * (each.y + TILE_GAP);
        let rest = ui.clip_rect().bottom() - ui.cursor().top() - 14.0 - 8.0 - HIT;
        let hero = (rest - block).clamp(if narrow { 136.0 } else { 148.0 }, 300.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, hero), egui::Sense::hover());
        let (lx, rx) = (rect.left(), rect.right());
        let by = rect.bottom() - if narrow { 42.0 } else { 46.0 };
        let y = rect.top() + (by - rect.top() - 4.0) * 0.5;
        let (folder, phone) = (egui::pos2(lx + 18.0, y), egui::pos2(rx - 11.0, y));
        let a = egui::pos2(folder.x + 34.0, y);
        let b = egui::pos2(phone.x - 27.0, y);
        let loose = egui::pos2(a.x + (b.x - a.x) * 0.42, y + (by - y - 22.0).max(16.0));
        let phase = self.restore_phase();
        let connected = self.adb_phone().is_some();
        let sag = if narrow { 16.0 } else { 30.0 };
        let (plug, seal) = self.cable(
            ui,
            t,
            Cord {
                id: "restore",
                a,
                b,
                loose,
                sag,
                phase,
                connected,
                phone_at_b: true,
                coil: None,
            },
        );
        icon(ui, folder, ICON_FOLDER, 36.0, t.ink);
        let ink = mix(mix(t.muted, t.ink, plug), t.accent, seal);
        let shown = self.adb_phone().map(|p| p.0);
        let (glyph, size) = self.device_icon(shown.as_deref(), 36.0);
        icon(ui, phone, glyph, size, ink);
        badge(ui, t, phone + vec2(8.0, 17.0), seal);

        let chosen = self
            .backups
            .iter()
            .find(|b| Some(&b.path) == self.chosen.as_ref());
        let name = chosen.map_or("No backup chosen".to_string(), |b| {
            Self::label(&self.backups, b)
        });
        let when = chosen.map_or(String::new(), |b| saved_when(b.age_days));
        let (phone_name, note) = self.restore_phone_words();
        let sub_y = by + if narrow { 22.0 } else { 26.0 };
        let (full, left) = (rx - lx, (rx - lx) / 2.0 - 80.0);
        let title = |s: &str| txt(s, 15.0, 600.0, t.ink);
        let sub = |s: &str| txt(s, 12.0, 400.0, t.muted);
        if narrow {
            let max = full - 118.0;
            put(
                ui,
                egui::pos2(lx, by + 2.0),
                Align2::LEFT_TOP,
                title(&name),
                max,
            );
            let to = if phase == Phase::Stopped && !connected {
                format!("{phone_name} unplugged")
            } else {
                format!("To {phone_name}")
            };
            put(ui, egui::pos2(lx, sub_y), Align2::LEFT_TOP, sub(&to), full);
            let words = (egui::pos2(rx, by), None, Align2::RIGHT_TOP);
            self.mid_words(ui, t, words, 15.0, phase, "Restored", !connected);
        } else {
            put(
                ui,
                egui::pos2(lx, by + 2.0),
                Align2::LEFT_TOP,
                title(&name),
                left,
            );
            put(
                ui,
                egui::pos2(lx, sub_y),
                Align2::LEFT_TOP,
                sub(&when),
                left,
            );
            put(
                ui,
                egui::pos2(rx, by + 2.0),
                Align2::RIGHT_TOP,
                title(&phone_name),
                left,
            );
            put(
                ui,
                egui::pos2(rx, sub_y),
                Align2::RIGHT_TOP,
                sub(&note),
                left,
            );
            let x = rect.center().x;
            let words = (
                egui::pos2(x, by),
                Some(egui::pos2(x, sub_y)),
                Align2::CENTER_TOP,
            );
            self.mid_words(ui, t, words, 18.0, phase, "Restored", !connected);
        }
        ui.add_space(8.0);
        // What to restore comes first so it shows at once, unless a backup must be chosen first.
        if chosen.is_none() {
            self.backup_list(ui, t);
            ui.add_space(18.0);
            self.restore_rows(ui, t, 0.0);
        } else {
            self.restore_rows(ui, t, 0.0);
            ui.add_space(18.0);
            self.backup_list(ui, t);
        }
        self.restore_result(ui, t, plan);
    }

    /// The phone's name and state, or what to do without one.
    fn restore_phone_words(&self) -> (String, String) {
        match self.adb_phone() {
            Some((serial, model)) => (model, self.phone_note(&serial)),
            None => (
                "No phone yet".to_string(),
                "Plug it in with USB debugging on".to_string(),
            ),
        }
    }

    /// What can come back from the chosen backup.
    fn restore_rows(&mut self, ui: &mut egui::Ui, t: &Tokens, room: f32) {
        let phase = self.restore_phase();
        let locked = self.busy.load(Ordering::Relaxed) || phase != Phase::Idle;
        self.battery_warnings(ui, t);
        if phase == Phase::Idle && self.adb_phone().is_none() {
            self.connect_help(ui, t, !self.busy.load(Ordering::Relaxed));
        }
        let chosen = self
            .backups
            .iter()
            .find(|b| Some(&b.path) == self.chosen.as_ref());
        let (has_files, has_apps) = chosen.map_or((false, false), |b| (b.files, b.apps));
        let path = chosen.map(|b| b.path.clone());
        let folder = path.clone();
        let (files_line, label) = self.picked_words(path.as_ref());

        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, HIT), egui::Sense::hover());
        let at = egui::pos2(r.left(), r.center().y);
        put(
            ui,
            at,
            Align2::LEFT_CENTER,
            txt("What to restore", 13.0, 600.0, t.ink),
            w,
        );

        let has = (has_files, has_apps);
        self.onto_tiles(ui, t, room, &files_line, has, locked, phase);
        ui.add_space(6.0);
        let can = !locked && has_files && self.restore_files;
        let why = if locked {
            "Wait for the job to finish."
        } else if !has_files {
            "This backup has no files to choose from."
        } else {
            "Turn on Photos, files above first."
        };
        if quiet(ui, t, &label, can)
            .on_disabled_hover_text(why)
            .clicked()
            && let Some(path) = path
        {
            self.open_picker(ui.ctx(), path, false);
        }
        if let Some(b) = folder {
            let version = self.version_of(&b);
            let words = match &version {
                Some(name) => format!(
                    "Version: {}…",
                    backup::Run {
                        name: name.clone(),
                        ..Default::default()
                    }
                    .title()
                ),
                None => "Version: latest…".to_string(),
            };
            if quiet(ui, t, &words, !locked)
                .on_hover_text("Restore or read the backup as it was after an earlier backup.")
                .on_disabled_hover_text("Wait for the job to finish.")
                .clicked()
            {
                let mut runs = backup::runs(&b);
                runs.retain(|r| r.finished || !r.replaced.is_empty());
                self.ask = Some(Ask::History(b, runs, version));
            }
        }
        ui.add(
            egui::Label::new(txt(
                "Files the phone already has are skipped, never replaced. Apps come back without their logins and data.",
                12.0,
                400.0,
                t.muted,
            ))
            .wrap(),
        );
    }

    /// The files tile's line and the Choose button's words, with any files picked in `folder`.
    pub(crate) fn picked_words(&self, folder: Option<&PathBuf>) -> (String, String) {
        match self.only.as_ref().filter(|(f, ..)| Some(f) == folder) {
            Some((_, picked, total)) => (
                format!("{} of {}", count(picked.len()), files(*total)),
                format!(
                    "Choose files ({} of {})…",
                    count(picked.len()),
                    count(*total)
                ),
            ),
            None => ("All files".into(), "Choose files…".into()),
        }
    }

    /// What can go onto a phone, as tiles. `has` says if there are (files, apps) to copy at all.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn onto_tiles(
        &mut self,
        ui: &mut egui::Ui,
        t: &Tokens,
        room: f32,
        files_line: &str,
        (has_files, has_apps): (bool, bool),
        locked: bool,
        phase: Phase,
    ) {
        const NOT_IN: &str = "Not in backup";
        const NOT_YET: &str = "Not yet";
        let line = |has: bool, on: bool, line: &'static str| match (has, phase) {
            (false, _) => NOT_IN,
            (_, Phase::Done(0)) if on => "Copied",
            _ => line,
        };
        let (files_on, apps_on) = (
            self.restore_files && has_files,
            self.restore_apps && has_apps,
        );
        let copied = line(has_files, files_on, "");
        let items = [
            (
                "\u{E91B}",
                "Photos, files",
                if copied.is_empty() {
                    files_line
                } else {
                    copied
                },
                files_on,
                !has_files,
            ),
            (
                "\u{E71D}",
                "Apps",
                line(has_apps, apps_on, "Without logins"),
                apps_on,
                !has_apps,
            ),
            ("\u{E77B}", "Contacts", NOT_YET, false, true),
            ("\u{E8BD}", "Messages", NOT_YET, false, true),
        ];
        let (cols, each) = tile_grid(ui.available_width(), room, items.len());
        for (row, chunk) in items.chunks(cols).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = TILE_GAP;
                for (k, (glyph, name, line, on, dim)) in chunk.iter().enumerate() {
                    if tile(ui, t, (glyph, name, line), *on, locked, *dim, each) {
                        match row * cols + k {
                            0 => self.restore_files = !self.restore_files,
                            _ => self.restore_apps = !self.restore_apps,
                        }
                    }
                }
            });
            ui.add_space(TILE_GAP - ui.spacing().item_spacing.y);
        }
    }

    /// The backups in the folder. Returns where the chosen one sits, so the cable can leave from it.
    fn backup_list(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Option<f32> {
        let locked = self.busy.load(Ordering::Relaxed) || self.restore_phase() != Phase::Idle;
        let w = ui.available_width();
        let mut find = false;
        ui.allocate_ui_with_layout(vec2(w, HIT), Layout::left_to_right(Align::Center), |ui| {
            ui.label(txt("Your backups", 13.0, 600.0, t.ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                find = quiet(ui, t, "Find another…", !locked)
                    .on_disabled_hover_text("Wait for the job to finish.")
                    .clicked();
            });
        });
        if find
            && let Some(dir) = rfd::FileDialog::new()
                .set_directory(&self.dest)
                .pick_folder()
        {
            self.choose(Some(dir.clone()));
            self.scan(&dir);
        }
        let (line, _) = ui.allocate_exact_size(vec2(w, 22.0), egui::Sense::hover());
        let y = line.center().y;
        icon(
            ui,
            egui::pos2(line.left() + 9.0, y),
            ICON_FOLDER,
            14.0,
            t.muted,
        );
        let full = self.scanned.display().to_string();
        let shown = put(
            ui,
            egui::pos2(line.left() + 24.0, y),
            Align2::LEFT_CENTER,
            txt(&short_path(ui, &full), 12.0, 400.0, t.muted),
            w - 24.0,
        );
        ui.interact(shown, ui.scope_id().with("scanned"), egui::Sense::hover())
            .on_hover_text(&full);
        ui.add_space(4.0);
        if self.backups.is_empty() {
            ui.add(
                egui::Label::new(txt(
                    "No backups in this folder yet. Back up a phone first, or find the folder that holds your backups.",
                    12.0,
                    400.0,
                    t.muted,
                ))
                .wrap(),
            );
        }
        let gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 4.0);
        let mut row = None;
        let mut action = None;
        let mut pressed = None;
        for b in &self.backups {
            let on = self.chosen.as_ref() == Some(&b.path);
            let label = Self::label(&self.backups, b);
            let many = self.backups.len() > 1;
            let (press, act, y) = backup_card(ui, t, b, &label, on, locked, many, self.motion);
            if press {
                pressed = Some(b.path.clone());
            }
            if on {
                row = Some(y);
            }
            if let Some(a) = act {
                action = Some((a, b.path.clone()));
            }
        }
        ui.spacing_mut().item_spacing.y = gap;
        if pressed.is_some() {
            self.choose(pressed);
        }
        match action {
            Some((0, path)) => {
                let _ = backup::explorer().arg(path).spawn();
            }
            Some((1, path)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.ask = Some(Ask::Rename(path, name, None));
            }
            Some((2, path)) => self.verify(ui.ctx(), path),
            Some((3, path)) => self.export(ui.ctx(), path),
            Some((4, path)) => self.ask = Some(Ask::Copy(path, None)),
            Some((5, path)) => {
                if !backup::has_password(&path) {
                    self.ask = Some(Ask::Password(path, PwStep::new()));
                } else if !self.ask_password(&path, false) {
                    self.ask = Some(Ask::Password(path, PwStep::Menu));
                }
            }
            Some((_, path)) => {
                let older = backup::runs(&path).len();
                self.ask = Some(Ask::Delete(path, older));
            }
            None => {}
        }
        row
    }

    /// What the command bar says and offers.
    fn restore_cmd(&self, plan: Option<Plan>) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
        let phase = self.restore_phase();
        let checked = matches!(self.preview_for(plan.as_ref()), Some(Ok(_)));
        let phone = self.adb_phone().is_some();
        let percent = self
            .fraction
            .map_or(String::new(), |f| format!(" {:.0}%", (f * 100.0).floor()));
        let (title, note): (String, String) = match phase {
            Phase::Running => (
                self.status.clone(),
                match self.now.rsplit(['/', '\\']).next() {
                    Some(file) if !file.is_empty() => file.to_string(),
                    _ => "Keep the phone plugged in and unlocked.".into(),
                },
            ),
            _ if busy => (
                match self.job {
                    Job::Verify => format!("Checking the backup…{percent}"),
                    Job::Copy => format!("Copying between backups…{percent}"),
                    Job::Delete => "Deleting the backup…".into(),
                    Job::Export => format!("Exporting…{percent}"),
                    Job::Protect => format!("Changing the backup's protection…{percent}"),
                    _ => "Checking…".into(),
                },
                self.status.clone(),
            ),
            Phase::Done(0) => ("Restore finished".into(), "No errors. Report saved.".into()),
            Phase::Done(n) => (finished_with(n), "The report says what failed.".into()),
            Phase::Stopped => (
                self.stopped_at.map_or("Restore stopped".into(), |f| {
                    format!("Stopped at {:.0}%", (f * 100.0).floor())
                }),
                if phone {
                    "Copied files are safe.".into()
                } else {
                    "Connect a phone to try again.".into()
                },
            ),
            Phase::Idle if !phone => (
                "Connect a phone to restore".into(),
                "With USB debugging on.".into(),
            ),
            Phase::Idle if self.backups.is_empty() => {
                ("No backups to restore yet".into(), String::new())
            }
            Phase::Idle if self.chosen.is_none() => {
                ("Choose a backup to restore".into(), String::new())
            }
            Phase::Idle if plan.is_none() => ("Pick something to restore".into(), String::new()),
            Phase::Idle if checked => ("Ready to restore".into(), "Nothing changes yet.".into()),
            Phase::Idle => (
                "Ready to check".into(),
                "Changes nothing on the phone.".into(),
            ),
        };
        let open = match phase {
            Phase::Done(_) => self.restore_report.clone(),
            _ => None,
        };
        Cmd {
            title,
            note,
            label: match phase {
                Phase::Done(_) => "Open report",
                Phase::Stopped => "Try again",
                _ if checked => "Restore now",
                _ => "Check restore",
            },
            enabled: open.is_some() || (plan.is_some() && !matches!(phase, Phase::Done(_))),
            plan: plan.map(|p| (p, !checked)),
            open,
            again: matches!(phase, Phase::Done(_) | Phase::Stopped),
        }
    }

    /// What the last check found, while the choices still match it.
    pub(crate) fn preview_for(&self, plan: Option<&Plan>) -> Option<&Result<String, String>> {
        self.preview
            .as_ref()
            .filter(|(p, _)| Some(p) == plan)
            .map(|(_, r)| r)
    }

    /// What the last check or restore found.
    pub(crate) fn restore_result(&mut self, ui: &mut egui::Ui, t: &Tokens, plan: Option<&Plan>) {
        if let Some((_, r)) = self
            .verified
            .as_ref()
            .filter(|(folder, _)| Some(folder) == self.chosen.as_ref())
            .filter(|_| !self.busy.load(Ordering::Relaxed))
        {
            ui.add_space(16.0);
            result_box(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (title, color, text) = match r {
                    Ok(m) => ("Backup verified", t.ink, m),
                    Err(e) => ("Backup not verified", t.error, e),
                };
                ui.label(RichText::new(title).font(semibold(16.0)).color(color));
                ui.label(text.as_str());
            });
        }
        if self.tab == Tab::Restore
            && let Some(r) = &self.tidy_msg
            && !self.busy.load(Ordering::Relaxed)
        {
            ui.add_space(16.0);
            result_box(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (title, color, text) = match (r, self.job) {
                    (Ok(m), Job::Protect) => ("Password changed", t.ink, m),
                    (Err(e), Job::Protect) => ("Not finished", t.error, e),
                    (Ok(m), Job::Delete) => ("Backup deleted", t.ink, m),
                    (Ok(m), _) => ("Copy finished", t.ink, m),
                    (Err(e), Job::Delete) => ("Backup not deleted", t.error, e),
                    (Err(e), _) => ("Copy stopped", t.error, e),
                };
                ui.label(RichText::new(title).font(semibold(16.0)).color(color));
                ui.label(text.as_str());
            });
        }
        if let Some(r) = self
            .preview_for(plan)
            .filter(|_| !self.busy.load(Ordering::Relaxed))
        {
            ui.add_space(16.0);
            result_box(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (title, color, text) = match r {
                    Ok(m) => ("What will happen", t.ink, m),
                    Err(e) if self.tab == Tab::Transfer => ("Cannot transfer", t.error, e),
                    Err(e) => ("Cannot restore", t.error, e),
                };
                ui.label(RichText::new(title).font(semibold(16.0)).color(color));
                ui.label(text.as_str());
            });
        }
        if let Some((folder, r)) = &self.export_msg {
            ui.add_space(16.0);
            result_box(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (title, color, text) = match r {
                    Ok(m) => ("Export finished", t.ink, m),
                    Err(e) => ("Export stopped", t.error, e),
                };
                ui.label(RichText::new(title).font(semibold(16.0)).color(color));
                ui.label(text.as_str());
                ui.add_space(4.0);
                if ui.button("Open folder").clicked() {
                    let _ = backup::explorer().arg(folder).spawn();
                }
            });
        }
        match &self.restore_msg {
            Some(Ok((m, _))) => {
                ui.add_space(16.0);
                result_box(t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(if self.tab == Tab::Transfer {
                            "Transfer finished"
                        } else {
                            "Restore finished"
                        })
                        .font(semibold(16.0)),
                    );
                    ui.label(m);
                });
            }
            Some(Err(e)) => {
                ui.add_space(16.0);
                result_box(t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(if self.tab == Tab::Transfer {
                            "Transfer stopped"
                        } else {
                            "Restore stopped"
                        })
                        .font(semibold(16.0))
                        .color(t.error),
                    );
                    ui.label(e);
                    self.open_restore_report(ui);
                });
            }
            None => {}
        }
    }

    /// A backup's name in lists. Two phones of the same model: told apart by the end of the ID
    /// in "<model>_<id>".
    pub(crate) fn label(all: &[backup::Saved], b: &backup::Saved) -> String {
        let twin = all.iter().filter(|o| o.name == b.name).count() > 1;
        let id = b
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit_once('_'))
            .map(|(_, id)| id)
            .filter(|_| twin);
        match id {
            Some(id) => format!(
                "{} (ID {})",
                b.name,
                id.char_indices().rev().nth(3).map_or(id, |(i, _)| &id[i..])
            ),
            None => b.name.clone(),
        }
    }

    /// The dialog about the chosen backup: delete, rename or copy into another.
    pub(crate) fn ask_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(ask) = &mut self.ask else { return };
        let label_of = |path: &Path| {
            self.backups
                .iter()
                .find(|b| b.path == path)
                .map(|b| (Self::label(&self.backups, b), b.bytes))
                .unwrap_or_else(|| (path.display().to_string(), None))
        };
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let answer = match ask {
            Ask::Delete(path, older) => {
                let older = *older;
                let (name, bytes) = label_of(path);
                let full = match bytes {
                    Some(n) => format!("{} ({})", path.display(), size(n)),
                    None => path.display().to_string(),
                };
                confirm(
                    ctx,
                    &format!("Delete the backup of {name}?"),
                    "Delete",
                    "Cancel",
                    true,
                    |ui| {
                        ui.label("This moves the whole folder to the Recycle Bin:");
                        ui.label(RichText::new(full).font(semibold(14.0)));
                        if older > 0 {
                            ui.label(format!(
                                "Its {} older {} go with it.",
                                count(older),
                                if older == 1 { "version" } else { "versions" }
                            ));
                        }
                        ui.label(muted("You can bring it back from the Recycle Bin. If the folder is too big for it, Windows asks before deleting it for good."));
                    },
                )
            }
            Ask::Rename(_, name, err) => {
                confirm(ctx, "Rename backup", "Rename", "Cancel", true, |ui| {
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    r.request_focus();
                    if let Some(e) = err {
                        ui.label(RichText::new(e.as_str()).color(t.error));
                    }
                    ui.label(muted(
                        "The next backup of this phone starts a new folder with its usual name.",
                    ));
                })
            }
            Ask::Unlock(path, password, err, _, recovery) => {
                let (name, _) = label_of(path);
                let ready = !password.is_empty();
                let title = if *recovery {
                    format!("Recovery key for {name}")
                } else {
                    format!("Password for {name}")
                };
                let can_recover = backup::has_recovery(path);
                confirm(ctx, &title, "Unlock", "Cancel", ready, |ui| {
                    ui.label(if *recovery {
                        "Type the recovery key you kept when the password was set."
                    } else {
                        "This backup is protected with a password."
                    });
                    let edit = egui::TextEdit::singleline(&mut **password)
                        .password(!*recovery)
                        .desired_width(f32::INFINITY);
                    ui.add(edit).request_focus();
                    if let Some(e) = err {
                        ui.label(RichText::new(e.as_str()).color(t.error));
                    }
                    let other = match (*recovery, can_recover) {
                        (true, _) => Some("Use the password instead"),
                        (false, true) => Some("Forgot the password? Use the recovery key"),
                        (false, false) => None,
                    };
                    if let Some(other) = other
                        && ui.link(other).clicked()
                    {
                        *recovery = !*recovery;
                        password.clear();
                        *err = None;
                    }
                    ui.label(muted("It stays unlocked until you close Phone Backup."));
                })
            }
            Ask::Password(path, step) => {
                let (name, _) = label_of(path);
                password_dialog(ctx, t, path, &name, step)
            }
            Ask::Recovery(path, key, kept) => {
                let (name, _) = label_of(path);
                recovery_dialog(ctx, t, &name, key, kept)
            }
            Ask::History(path, runs, pick) => {
                let (name, _) = label_of(path);
                confirm(
                    ctx,
                    &format!("Versions of {name}"),
                    "Use this version",
                    "Cancel",
                    true,
                    |ui| history_list(ui, t, path, runs, pick),
                )
            }
            Ask::Copy(from, into) => {
                let (name, _) = label_of(from);
                let others: Vec<_> = self
                    .backups
                    .iter()
                    .filter(|b| b.path != *from)
                    .map(|b| {
                        (
                            b.path.clone(),
                            Self::label(&self.backups, b),
                            days_ago(b.age_days),
                        )
                    })
                    .collect();
                confirm(
                    ctx,
                    "Copy files into another backup",
                    "Copy files",
                    "Cancel",
                    into.is_some(),
                    |ui| {
                        ui.label(format!(
                            "Copies every file of {name} into the backup you choose."
                        ));
                        ui.label(muted("Files already there are kept. A file with the same name but different contents is saved as name (2)."));
                        ui.add_space(4.0);
                        for (path, other, age) in others {
                            let on = into.as_ref() == Some(&path);
                            if ui.radio(on, format!("{other}, backed up {age}")).clicked() {
                                *into = Some(path);
                            }
                        }
                    },
                )
            }
        };
        match answer {
            Some(false) => self.ask = None,
            Some(true) => match self.ask.take() {
                Some(Ask::Delete(path, _)) => self.tidy(ctx, Job::Delete, path, None),
                Some(Ask::History(path, _, pick)) => self.use_version(path, pick),
                Some(Ask::Copy(from, into)) => self.tidy(ctx, Job::Copy, from, into),
                Some(Ask::Password(path, step)) => self.password_step(ctx, path, step),
                Some(Ask::Unlock(path, password, _, job, recovery)) => {
                    let opened = if recovery {
                        backup::unlock_with_recovery(&path, &password)
                    } else {
                        backup::unlock(&path, &password)
                    };
                    match opened {
                        Ok(()) => {
                            self.unlocked(ctx, job);
                            // The password was forgotten, so a new one is asked for.
                            if recovery {
                                let step = PwStep::Change {
                                    pw: Default::default(),
                                    again: Default::default(),
                                    err: None,
                                };
                                self.ask = Some(Ask::Password(path, step));
                            }
                        }
                        Err(e) => {
                            let ask = Ask::Unlock(path, password, Some(e), job, recovery);
                            self.ask = Some(ask);
                        }
                    }
                }
                Some(Ask::Recovery(..)) => {}
                Some(Ask::Rename(path, name, _)) => match backup::rename_backup(&path, &name) {
                    Ok(new) => {
                        self.choose(Some(new.clone()));
                        // A backup folder chosen on its own is listed under its new name.
                        let dir = if self.scanned == path {
                            new
                        } else {
                            self.scanned.clone()
                        };
                        self.scan(&dir);
                    }
                    Err(e) => self.ask = Some(Ask::Rename(path, name, Some(e))),
                },
                None => {}
            },
            None => {}
        }
    }

    /// Acts on the password dialog's main button.
    fn password_step(&mut self, ctx: &egui::Context, path: PathBuf, step: PwStep) {
        let check = |pw: &str, again: &str| {
            if pw.chars().count() < 8 {
                Err("Use at least 8 characters.".to_string())
            } else if pw != again {
                Err("The two passwords are not the same.".to_string())
            } else {
                Ok(())
            }
        };
        match step {
            PwStep::New {
                pw,
                again,
                all,
                sure,
                ..
            } => {
                let scope = if all {
                    backup::Scope::All
                } else {
                    backup::Scope::Data
                };
                match check(&pw, &again).and_then(|()| backup::set_password(&path, &pw, scope)) {
                    Ok(()) => {
                        let key = backup::make_recovery(&path);
                        self.protect(ctx, path.clone(), false);
                        if let Ok(key) = key {
                            self.ask = Some(Ask::Recovery(path, key.into(), false));
                        }
                    }
                    Err(e) => {
                        let err = Some(e);
                        let step = PwStep::New {
                            pw,
                            again,
                            all,
                            sure,
                            err,
                        };
                        self.ask = Some(Ask::Password(path, step));
                    }
                }
            }
            PwStep::Change { pw, again, .. } => {
                match check(&pw, &again).and_then(|()| backup::change_password(&path, &pw)) {
                    Ok(()) => {
                        self.job = Job::Protect;
                        self.tidy_msg = Some(Ok(
                            "The password is changed. Use the new one from now on.".into(),
                        ));
                    }
                    Err(e) => {
                        let step = PwStep::Change {
                            pw,
                            again,
                            err: Some(e),
                        };
                        self.ask = Some(Ask::Password(path, step));
                    }
                }
            }
            PwStep::Remove => self.protect(ctx, path, true),
            PwStep::Menu => {
                let step = PwStep::Change {
                    pw: Default::default(),
                    again: Default::default(),
                    err: None,
                };
                self.ask = Some(Ask::Password(path, step));
            }
            PwStep::Finish => {
                let remove = backup::lock_of(&path)
                    .is_ok_and(|l| l.is_some_and(|l| l.pending == Some(backup::Pending::Remove)));
                self.protect(ctx, path, remove);
            }
            PwStep::Scope(scope) => match backup::set_scope(&path, scope) {
                Ok(()) => self.protect(ctx, path, false),
                Err(e) => {
                    self.job = Job::Protect;
                    self.tidy_msg = Some(Err(e));
                }
            },
            PwStep::Lock => backup::forget(&path),
            PwStep::NewRecovery => match backup::make_recovery(&path) {
                Ok(key) => self.ask = Some(Ask::Recovery(path, key.into(), false)),
                Err(e) => {
                    self.job = Job::Protect;
                    self.tidy_msg = Some(Err(e));
                }
            },
        }
    }

    /// Runs the job that stopped to ask for a password again.
    fn unlocked(&mut self, ctx: &egui::Context, job: Option<Job>) {
        if job == Some(Job::Backup) {
            self.start(ctx);
        }
    }

    /// Asks for the backup's password when reading what the job needs takes one not yet
    /// entered: personal data, or with `files` files and apps too. True when it asked: the
    /// caller then waits for the next click.
    pub(crate) fn ask_password(&mut self, folder: &Path, files: bool) -> bool {
        let ask = backup::needs_password(folder, files);
        if ask {
            self.ask = Some(Ask::Unlock(
                folder.to_path_buf(),
                Default::default(),
                None,
                None,
                false,
            ));
        }
        ask
    }

    pub(crate) fn open_restore_report(&self, ui: &mut egui::Ui) {
        if let Some(path) = &self.restore_report {
            ui.add_space(4.0);
            if ui.button("Open report").clicked() {
                let _ = backup::explorer().arg(path).spawn();
            }
        }
    }
}

/// The password dialog: give the backup a password, or change, widen, narrow, lock or remove
/// it. Some(true) runs the step it ends on (`password_step`).
fn password_dialog(
    ctx: &egui::Context,
    t: &Tokens,
    path: &Path,
    name: &str,
    step: &mut PwStep,
) -> Option<bool> {
    let muted = |s: &str| RichText::new(s).color(t.muted);
    let typed = |ui: &mut egui::Ui, label: &str, text: &mut String, focus: bool| {
        ui.label(label);
        let r = ui.add(
            egui::TextEdit::singleline(text)
                .password(true)
                .desired_width(f32::INFINITY),
        );
        // The first field starts with the cursor in it; a click elsewhere is left alone.
        if focus && ui.memory(|m| m.focused().is_none()) {
            r.request_focus();
        }
    };
    let error = |ui: &mut egui::Ui, err: &Option<String>| {
        if let Some(e) = err {
            ui.label(RichText::new(e.as_str()).color(t.error));
        }
    };
    match step {
        PwStep::New {
            pw,
            again,
            all,
            sure,
            err,
        } => {
            let ready = *sure && !pw.is_empty() && !again.is_empty();
            let title = format!("Protect {name} with a password");
            confirm(ctx, &title, "Protect", "Cancel", ready, |ui| {
                typed(ui, "Password", pw, true);
                typed(ui, "Type it again", again, false);
                error(ui, err);
                ui.add_space(6.0);
                ui.label("What it protects:");
                ui.radio_value(all, false, "Contacts, calls, messages and calendar");
                ui.label(muted(
                    "Photos, videos and files stay ordinary files that open in Explorer.",
                ));
                ui.radio_value(all, true, "Everything copied from the phone");
                ui.label(muted("Photos, videos and files then open only through Phone Backup, not with a double-click in Explorer."));
                ui.add_space(6.0);
                ui.checkbox(sure, "I understand that without the password or the recovery key shown next, nobody can open this backup, not even Phone Backup.");
                ui.label(muted("File names stay visible. Exports are ordinary files and are not protected. Changing a large backup takes a while; you can stop and finish later."));
            })
        }
        PwStep::Change { pw, again, err } => {
            let ready = !pw.is_empty() && !again.is_empty();
            confirm(
                ctx,
                "New password",
                "Change password",
                "Cancel",
                ready,
                |ui| {
                    typed(ui, "New password", pw, true);
                    typed(ui, "Type it again", again, false);
                    error(ui, err);
                    ui.label(muted(
                        "Only the password changes; the files stay as they are, so this is quick.",
                    ));
                },
            )
        }
        PwStep::Remove => confirm(
            ctx,
            &format!("Remove the password of {name}?"),
            "Remove password",
            "Cancel",
            true,
            |ui| {
                ui.label("Every protected file is opened and saved as an ordinary file again.");
                ui.label(muted(
                    "Anyone who can open the backup folder can then read it.",
                ));
            },
        ),
        _ => {
            let lock = backup::lock_of(path).ok().flatten();
            let all = lock.as_ref().is_some_and(|l| l.scope == backup::Scope::All);
            let pending = lock.as_ref().and_then(|l| l.pending);
            let mut next = None;
            let answer = confirm(
                ctx,
                &format!("Password of {name}"),
                "Change password…",
                "Close",
                true,
                |ui| {
                    ui.label(if all {
                        "The password protects everything copied from the phone."
                    } else {
                        "The password protects contacts, calls, messages and calendar."
                    });
                    if let Some(p) = pending {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(match p {
                                backup::Pending::Protect => {
                                    "Protecting the files was stopped part way."
                                }
                                backup::Pending::Remove => {
                                    "Removing the password was stopped part way."
                                }
                            })
                            .color(t.error),
                        );
                        if primary(ui, t, "Finish now", true) {
                            next = Some(PwStep::Finish);
                        }
                    }
                    ui.add_space(8.0);
                    let (widen, words) = if all {
                        (backup::Scope::Data, "Protect only personal data")
                    } else {
                        (backup::Scope::All, "Protect everything")
                    };
                    if quiet(ui, t, words, pending.is_none()).clicked() {
                        next = Some(PwStep::Scope(widen));
                    }
                    let (words, hint) = if backup::has_recovery(path) {
                        (
                            "New recovery key…",
                            "Makes a new recovery key. The old one stops working.",
                        )
                    } else {
                        (
                            "Make a recovery key…",
                            "A key that opens the backup if the password is forgotten.",
                        )
                    };
                    if quiet(ui, t, words, true).on_hover_text(hint).clicked() {
                        next = Some(PwStep::NewRecovery);
                    }
                    if quiet(ui, t, "Lock now", true)
                        .on_hover_text("Asks for the password again before the backup is read.")
                        .clicked()
                    {
                        next = Some(PwStep::Lock);
                    }
                    if quiet(ui, t, "Remove password…", true).clicked() {
                        next = Some(PwStep::Remove);
                    }
                },
            );
            match next {
                // Removing asks first, on the next page.
                Some(PwStep::Remove) => {
                    *step = PwStep::Remove;
                    None
                }
                Some(go) => {
                    *step = go;
                    Some(true)
                }
                None => answer,
            }
        }
    }
}

/// Shows a new recovery key once, with Copy and a printable page.
fn recovery_dialog(
    ctx: &egui::Context,
    t: &Tokens,
    name: &str,
    key: &str,
    kept: &mut bool,
) -> Option<bool> {
    let muted = |s: &str| RichText::new(s).color(t.muted);
    confirm(
        ctx,
        "Keep your recovery key",
        "Done",
        "Close",
        *kept,
        |ui| {
            ui.label("If the password is ever forgotten, this key opens the backup instead. It is shown only now.");
            ui.add_space(8.0);
            ui.label(RichText::new(key).monospace().size(18.0).color(t.ink));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if quiet(ui, t, "Copy", true).clicked() {
                    ui.ctx().copy_text(key.to_string());
                }
                if quiet(ui, t, "Save printable page…", true).clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_file_name(format!("Phone Backup recovery key - {name}.html"))
                        .add_filter("Web page", &["html"])
                        .save_file()
                    && fs::write(&file, recovery_page(name, key)).is_ok()
                {
                    let _ = backup::explorer().arg(&file).spawn();
                }
            });
            ui.label(muted("Keep it away from this computer: printed, or in a password manager. Anyone with this key can open the backup."));
            ui.add_space(6.0);
            ui.checkbox(kept, "I have kept the recovery key somewhere safe.");
        },
    )
}

/// A page to print with the backup's name and its recovery key.
fn recovery_page(name: &str, key: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>Recovery key</title>\n<style>body{{font:16px/1.5 system-ui,sans-serif;max-width:40em;margin:3em auto;padding:0 1em;color:#111}}code{{display:block;font:24px/1.4 monospace;letter-spacing:.05em;margin:1em 0;padding:.6em;border:2px solid #111;text-align:center}}</style></head>\n<body><h1>Phone Backup recovery key</h1>\n<p>Backup: <strong>{}</strong></p>\n<code>{}</code>\n<p>If the password of this backup is forgotten, choose <em>Use the recovery key</em> when Phone Backup asks for it, and type this key. Then choose a new password.</p>\n<p>Anyone with this key can open the backup. Keep this page somewhere safe, away from the computer that holds the backup, and delete the file once it is printed.</p>\n</body></html>\n",
        esc(name),
        esc(key)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn recovery_page_holds_the_key_and_escapes_the_name() {
        let page = super::recovery_page("Mum's <phone>", "ABCD-EFGH");
        assert!(page.contains("<code>ABCD-EFGH</code>"));
        assert!(page.contains("Mum's &lt;phone&gt;"));
        assert!(page.starts_with("<!doctype html>\n"));
    }
}

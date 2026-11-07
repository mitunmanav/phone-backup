use super::backup_tab::{Cord, Phase};
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
    const ACTIONS: [(&str, &str, &str); 6] = [
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
    let line = format!("{}: {holds}", saved_when(b.age_days));
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
            put(
                ui,
                cell.center_top() + vec2(0.0, 25.0),
                Align2::CENTER_TOP,
                txt(name, 12.0, 400.0, ink),
                cell_w - 2.0,
            );
            if a.clicked() {
                action = Some(k);
            }
        }
    }
    (r.clicked(), action, top.center().y)
}

const ICON_PHONE: &str = "\u{E8EA}";
const ICON_FOLDER: &str = "\u{E8B7}";

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
            Some(Ok(_)) => Phase::Done(0),
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
                spring(ui.ctx(), egui::Id::unique("restore-plug"), target, 0.55)
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
        icon(ui, phone, ICON_PHONE, 36.0, ink);
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
        icon(ui, phone, ICON_PHONE, 36.0, ink);
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
        if quiet(ui, t, &label, can).clicked()
            && let Some(path) = path
        {
            self.open_picker(ui.ctx(), path, false);
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
                find = quiet(ui, t, "Find another…", !locked).clicked();
            });
        });
        if find
            && let Some(dir) = rfd::FileDialog::new()
                .set_directory(&self.dest)
                .pick_folder()
        {
            self.chosen = Some(dir.clone());
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
            self.chosen = pressed;
        }
        match action {
            Some((0, path)) => {
                let _ = Command::new("explorer").arg(path).spawn();
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
            Some((_, path)) => self.ask = Some(Ask::Delete(path)),
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
                    _ => "Checking…".into(),
                },
                self.status.clone(),
            ),
            Phase::Done(_) => ("Restore finished".into(), "No errors. Report saved.".into()),
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
                    let _ = Command::new("explorer").arg(folder).spawn();
                }
            });
        }
        match &self.restore_msg {
            Some(Ok(m)) => {
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
            Ask::Delete(path) => {
                let (name, bytes) = label_of(path);
                let full = match bytes {
                    Some(n) => format!("{} ({})", path.display(), size(n)),
                    None => path.display().to_string(),
                };
                confirm(
                    ctx,
                    &format!("Delete the backup of {name}?"),
                    "Delete",
                    true,
                    |ui| {
                        ui.label("This moves the whole folder to the Recycle Bin:");
                        ui.label(RichText::new(full).font(semibold(14.0)));
                        ui.label(muted("You can bring it back from the Recycle Bin. If the folder is too big for it, Windows asks before deleting it for good."));
                    },
                )
            }
            Ask::Rename(_, name, err) => confirm(ctx, "Rename backup", "Rename", true, |ui| {
                let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                r.request_focus();
                if let Some(e) = err {
                    ui.label(RichText::new(e.as_str()).color(t.error));
                }
                ui.label(muted(
                    "The next backup of this phone starts a new folder with its usual name.",
                ));
            }),
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
                Some(Ask::Delete(path)) => self.tidy(ctx, Job::Delete, path, None),
                Some(Ask::Copy(from, into)) => self.tidy(ctx, Job::Copy, from, into),
                Some(Ask::Rename(path, name, _)) => match backup::rename_backup(&path, &name) {
                    Ok(new) => {
                        self.chosen = Some(new.clone());
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

    pub(crate) fn open_restore_report(&self, ui: &mut egui::Ui) {
        if let Some(path) = &self.restore_report {
            ui.add_space(4.0);
            if ui.button("Open report").clicked() {
                let _ = Command::new("explorer").arg(path).spawn();
            }
        }
    }
}

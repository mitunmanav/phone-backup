use crate::*;

impl App {
    fn change_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_directory(&self.dest)
            .pick_folder()
        {
            self.dest = dir.clone();
            self.scan(&dir);
        }
    }

    /// The backup folder in a read-only box, with Change… beside it.
    pub(crate) fn folder_row(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        if ui
            .add_enabled(!busy, egui::Button::new("Change…"))
            .on_disabled_hover_text("Wait for the job to finish")
            .clicked()
        {
            self.change_folder();
        }
        egui::Frame::new()
            .fill(t.field)
            .stroke(Stroke::new(1.0, t.line))
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(280.0));
                let full = self.dest.display().to_string();
                ui.add(egui::Label::new(short_path(ui, &full)).truncate())
                    .on_hover_text(&full);
            });
    }

    /// Things you set on the left; facts and links, which are not settings, on the right.
    pub(crate) fn settings_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let w = ui.available_width();
        if w >= 640.0 {
            let side = (w * 0.3).clamp(240.0, 380.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 28.0;
                ui.allocate_ui(vec2(w - side - 28.0, 0.0), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.vertical(|ui| self.settings_rows(ui, t));
                });
                ui.allocate_ui(vec2(side, 0.0), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.vertical(|ui| self.settings_side(ui, t));
                });
            });
        } else {
            self.settings_rows(ui, t);
            ui.add_space(12.0);
            self.settings_side(ui, t);
        }
    }

    fn settings_rows(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        if self.free.as_ref().is_none_or(|(d, _)| *d != self.dest) {
            self.free = Some((self.dest.clone(), backup::free_on(&self.dest)));
        }
        let free = self.free.as_ref().and_then(|f| f.1);
        let free = free.map_or(String::new(), |f| format!("{} free on this PC", size(f)));
        let full = self.dest.display().to_string();
        // Under 400 inside its frame a plate wraps its words, so only the wide one needs a cut.
        let room = ui.available_width() - 150.0;
        let title = if room < 278.0 {
            format!("Backups go to {}", short_path(ui, &full))
        } else {
            cut_middle(ui, &format!("Backups go to {full}"), 13.0, 400.0, room)
        };
        let mut change = false;
        plate(ui, t, "\u{E8B7}", &title, &free, |ui| {
            change = quiet(ui, t, "Change…", !busy).clicked();
        });
        if change {
            self.change_folder();
        }

        plate_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(txt("Look", 13.0, 600.0, t.ink));
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                let now = ui.ctx().options(|o| o.theme_preference);
                for (value, name, kinds) in [
                    (
                        egui::ThemePreference::System,
                        "Follow Windows",
                        &[false, true][..],
                    ),
                    (egui::ThemePreference::Light, "Light", &[false][..]),
                    (egui::ThemePreference::Dark, "Dark", &[true][..]),
                ] {
                    if theme_card(ui, t, name, kinds, now == value) {
                        // egui saves this with the window size, so it is remembered.
                        ui.ctx().set_theme(value);
                    }
                }
            });
            if ui.visuals().dark_mode {
                ui.add_space(10.0);
                let mut grey = GREY.load(Ordering::Relaxed);
                ui.horizontal(|ui| {
                    ui.label(txt("Darkness", 13.0, 400.0, t.muted));
                    ui.spacing_mut().slider_width = (ui.available_width() - 110.0).max(80.0);
                    let slider = egui::Slider::new(&mut grey, 0..=GREY_MAX)
                        .show_value(false)
                        .trailing_fill(true);
                    if ui.add(slider).changed() {
                        GREY.store(grey, Ordering::Relaxed);
                        ui.ctx().set_visuals_of(egui::Theme::Dark, visuals(true));
                    }
                    let word = if grey == 0 { "Pure black" } else { "Dark grey" };
                    ui.label(txt(word, 13.0, 400.0, t.ink));
                });
            }
        });
        ui.add_space(6.0);

        plate_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = "Sound when a job ends";
                    if switch(ui, t, self.sound, label) {
                        self.sound = !self.sound;
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.label(txt(label, 13.0, 600.0, t.ink));
                    });
                });
            });
            if self.sound {
                // Even columns, so the choices line up. A click plays it.
                let w = ui.available_width();
                let cols: usize = if w >= 736.0 {
                    6
                } else if w >= 364.0 {
                    3
                } else {
                    2
                };
                let cell = (w - 8.0 * (cols as f32 - 1.0)) / cols as f32;
                for (row, names) in backup::SOUNDS.chunks(cols).enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for (k, (name, _)) in names.iter().enumerate() {
                            let i = row * cols + k;
                            if chip(ui, t, "\u{E768}", name, self.sound_pick == i, cell) {
                                self.sound_pick = i;
                                backup::play_sound(i);
                            }
                        }
                    });
                }
            }
        });
        ui.add_space(6.0);

        plate_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(txt("Phone connection tool (adb)", 13.0, 600.0, t.ink));
            let note = if self.adb_ok {
                "Found. Apps, contacts and messages can be backed up over USB debugging."
            } else {
                "Not found. Photos and files still back up in File transfer mode. Apps, contacts and messages need it."
            };
            ui.add(egui::Label::new(txt(note, 12.0, 400.0, t.muted)).wrap());
            ui.add_space(8.0);
            if !self.adb_ok {
                self.adb_missing(ui, t);
                ui.add_space(4.0);
            }
            self.phone_actions(ui, t);
        });
    }

    /// The app itself, then where to get help.
    fn settings_side(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        ui.spacing_mut().item_spacing.y = 0.0;
        let muted = |ui: &mut egui::Ui, s: &str| {
            ui.add(egui::Label::new(txt(s, 12.0, 400.0, t.muted)).wrap());
        };
        let logo = self.logo_id(ui.ctx());
        plate_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(48.0), egui::Sense::hover());
                ui.painter().image(
                    logo,
                    r,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                ui.vertical(|ui| {
                    ui.label(txt("Phone Backup", 15.0, 600.0, t.ink));
                    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
                    ui.label(txt(&version, 12.0, 400.0, t.muted));
                });
            });
            ui.add_space(8.0);
            muted(
                ui,
                "Free software. No account, and none of your data is sent online.",
            );
            ui.add_space(6.0);
            if side_row(ui, t, "\u{E895}", "Check for updates") && !self.update_working {
                self.update_working = true;
                self.update = None;
                let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
                thread::spawn(move || {
                    let _ = tx.send(Msg::Update(backup::check_update()));
                    ctx.request_repaint();
                });
            }
            if self.update_working {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(16.0));
                    muted(ui, "Asking GitHub…");
                });
            }
            match &self.update {
                Some(Ok(None)) => muted(ui, "You have the newest version."),
                Some(Ok(Some(v))) => muted(ui, &format!("Version {v} is out.")),
                Some(Err(e)) => {
                    ui.add(egui::Label::new(txt(e, 12.0, 400.0, t.error)).wrap());
                }
                None => {}
            }
            if matches!(self.update, Some(Ok(Some(_)) | Err(_))) {
                ui.hyperlink_to(
                    "Go to the download page",
                    concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/latest"),
                );
            }
            if side_row(ui, t, "\u{EA18}", "Privacy") {
                self.go(Tab::Privacy);
            }
            if side_row(ui, t, "\u{E8A5}", "License") {
                self.go(Tab::License);
            }
            if side_row(ui, t, "\u{E943}", "Source code") {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab(env!("CARGO_PKG_REPOSITORY")));
            }
        });
        ui.add_space(12.0);
        plate_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(txt("Need help?", 13.0, 600.0, t.ink));
            ui.add_space(6.0);
            if side_row(ui, t, "\u{E88E}", "Turn on USB debugging") {
                self.help_from = Tab::Settings;
                self.go(Tab::Help);
            }
            if side_row(ui, t, "\u{E897}", "Show the intro") {
                self.intro = Some(0);
                self.tab_at = Instant::now();
            }
            if side_row(ui, t, "\u{E9D9}", "Save diagnostics…")
                && let Some(path) = rfd::FileDialog::new()
                    .set_file_name("phone-backup-diagnostics.txt")
                    .save_file()
            {
                let _ = fs::write(&path, backup::diagnostics(&self.dest));
                let _ = backup::explorer().arg(&path).spawn();
            }
            muted(
                ui,
                "Diagnostics are a text file of versions and connected phones. Read it before you share it.",
            );
        });
    }
}

/// A row in the side panel: picture, words, arrow, the whole width clickable.
fn side_row(ui: &mut egui::Ui, t: &Tokens, glyph: &str, label: &str) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), HIT), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, RADIUS, t.hover);
    }
    ring(ui, &r, rect, RADIUS, t);
    let y = rect.center().y;
    icon(ui, egui::pos2(rect.left() + 16.0, y), glyph, 16.0, t.accent);
    put(
        ui,
        egui::pos2(rect.left() + 36.0, y),
        Align2::LEFT_CENTER,
        txt(label, 13.0, 400.0, t.ink),
        rect.width() - 62.0,
    );
    icon(
        ui,
        egui::pos2(rect.right() - 14.0, y),
        "\u{E76C}",
        10.0,
        t.muted,
    );
    let clicked = r.clicked();
    r.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// A tiny copy of the app in that theme: side rail, cable, tiles. `kinds` lists the themes it
/// shows, side by side, each `true` being dark.
fn theme_card(ui: &mut egui::Ui, t: &Tokens, label: &str, kinds: &[bool], on: bool) -> bool {
    use egui::pos2;
    let (all, r) = ui.allocate_exact_size(vec2(148.0, 118.0), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, label));
    let card = Rect::from_min_size(all.min, vec2(148.0, 92.0));
    let each = card.width() / kinds.len() as f32;
    for (i, dark) in kinds.iter().enumerate() {
        let k = tokens(*dark);
        let clip = Rect::from_min_size(
            card.min + vec2(i as f32 * each, 0.0),
            vec2(each, card.height()),
        );
        let p = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
        p.rect_filled(card, RADIUS, k.bg);
        let rail = Rect::from_min_max(card.min, pos2(card.left() + 26.0, card.bottom()));
        let corners = egui::CornerRadius {
            nw: RADIUS,
            sw: RADIUS,
            ne: 0,
            se: 0,
        };
        p.rect_filled(rail, corners, k.surface);
        for n in 0..3 {
            let c = pos2(card.left() + 13.0, card.top() + 16.0 + n as f32 * 18.0);
            let fill = if n == 0 { k.accent } else { k.line };
            p.rect_filled(Rect::from_center_size(c, vec2(12.0, 9.0)), 2, fill);
        }
        let mid = card.top() + 22.0;
        let (x0, x1) = (card.left() + 46.0, card.right() - 22.0);
        let phone = Rect::from_center_size(pos2(card.left() + 38.0, mid), vec2(9.0, 16.0));
        p.rect_filled(phone, 2, k.ink);
        let pc = Rect::from_center_size(pos2(card.right() - 14.0, mid), vec2(14.0, 11.0));
        p.rect_filled(pc, 2, k.ink);
        let line: Vec<egui::Pos2> = (0..=16)
            .map(|i| {
                let s = i as f32 / 16.0;
                pos2(x0 + (x1 - x0) * s, mid + 12.0 * s * (1.0 - s) * 4.0)
            })
            .collect();
        p.add(egui::Shape::line(line, Stroke::new(2.0, k.accent)));
        for n in 0..3 {
            let at = pos2(card.left() + 32.0 + n as f32 * 36.0, card.bottom() - 32.0);
            let edge = if n == 0 { k.accent } else { k.line };
            p.rect(
                Rect::from_min_size(at, vec2(32.0, 24.0)),
                3,
                k.surface,
                Stroke::new(1.0, edge),
                StrokeKind::Inside,
            );
        }
    }
    let edge = if on {
        Stroke::new(2.0, t.accent)
    } else if r.hovered() {
        Stroke::new(1.0, t.border)
    } else {
        Stroke::new(1.0, t.line)
    };
    ui.painter()
        .rect_stroke(card, RADIUS, edge, StrokeKind::Outside);
    let weight = if on { 600.0 } else { 400.0 };
    put(
        ui,
        pos2(card.left(), card.bottom() + 6.0),
        Align2::LEFT_TOP,
        txt(label, 12.0, weight, t.ink),
        148.0,
    );
    ring(ui, &r, card, RADIUS, t);
    r.clicked()
}

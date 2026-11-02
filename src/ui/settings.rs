use crate::*;

impl App {
    /// The backup folder in a read-only box, with Change… beside it.
    pub(crate) fn folder_row(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        if ui
            .add_enabled(!busy, egui::Button::new("Change…"))
            .on_disabled_hover_text("Wait for the job to finish")
            .clicked()
            && let Some(dir) = rfd::FileDialog::new()
                .set_directory(&self.dest)
                .pick_folder()
        {
            self.dest = dir.clone();
            self.scan(&dir);
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

    pub(crate) fn settings_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        // egui sets this width outright, even past the window, so it is capped by the room left.
        ui.set_max_width(ui.available_width().min(640.0));
        ui.label(RichText::new("Where backups go").font(semibold(14.0)));
        row(
            ui,
            t,
            "Backup folder",
            "New backups are saved here. Restore lists the backups in it.",
            |ui| self.folder_row(ui, t),
        );

        section(ui, "Phone connection");
        let note = if self.adb_ok {
            "Found. Apps, contacts and messages can be backed up over USB debugging."
        } else {
            "Not found. Photos and files still back up in File transfer mode. Apps, contacts and messages need it."
        };
        row(ui, t, "adb", note, |ui| {
            if ui.button("Check again").clicked() {
                self.adb_ok = backup::adb_installed();
            }
        });
        if !self.adb_ok {
            ui.add_space(4.0);
            self.adb_missing(ui, t);
        }

        section(ui, "Look and sound");
        row(ui, t, "Theme", "", |ui| {
            let mut theme = ui.ctx().options(|o| o.theme_preference);
            // Right to left, so the last choice comes first.
            for (value, label) in [
                (egui::ThemePreference::Dark, "Dark"),
                (egui::ThemePreference::Light, "Light"),
                (egui::ThemePreference::System, "Follow Windows"),
            ] {
                ui.radio_value(&mut theme, value, label);
            }
            // egui saves this with the window size, so it is remembered.
            ui.ctx().set_theme(theme);
        });
        row(
            ui,
            t,
            "Sound",
            "When a backup or restore ends. The taskbar button also flashes when the window is in the background.",
            |ui| {
                ui.checkbox(&mut self.sound, "Play a sound");
            },
        );

        section(ui, "Help");
        row(
            ui,
            t,
            "Save diagnostics",
            "A text file with the app, Windows and adb versions and the phones adb sees. Read it before you share it.",
            |ui| {
                if ui.button("Save…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_file_name("phone-backup-diagnostics.txt")
                        .save_file()
                {
                    let _ = fs::write(&path, backup::diagnostics(&self.dest));
                    let _ = Command::new("explorer").arg(&path).spawn();
                }
            },
        );
        if link_row(
            ui,
            t,
            "Show the intro again",
            "The first-run steps: what the app does, adb, USB debugging and the backup folder.",
        ) {
            self.intro = Some(0);
            self.tab_at = Instant::now();
        }

        section(ui, "About");
        row(
            ui,
            t,
            &format!("Phone Backup {}", env!("CARGO_PKG_VERSION")),
            "Free software. No account, and none of your data is sent online.",
            |ui| {
                ui.hyperlink_to("Source code on GitHub", env!("CARGO_PKG_REPOSITORY"));
            },
        );
        let note = match &self.update {
            _ if self.update_working => "Asking GitHub…".to_string(),
            Some(Ok(None)) => "You have the newest version.".to_string(),
            Some(Ok(Some(v))) => format!("Version {v} is out."),
            _ => "Asks GitHub for the newest version, only when you click.".to_string(),
        };
        row(ui, t, "Updates", &note, |ui| {
            let check =
                ui.add_enabled(!self.update_working, egui::Button::new("Check for updates"));
            if check.clicked() {
                self.update_working = true;
                self.update = None;
                let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
                thread::spawn(move || {
                    let _ = tx.send(Msg::Update(backup::check_update()));
                    ctx.request_repaint();
                });
            }
            // The row lines controls up at the top; these are shorter than the button beside them.
            let beside = vec2(ui.available_width(), check.rect.height());
            ui.allocate_ui_with_layout(beside, Layout::right_to_left(Align::Center), |ui| {
                if self.update_working {
                    ui.add(egui::Spinner::new().size(16.0));
                }
                if matches!(self.update, Some(Ok(Some(_)) | Err(_))) {
                    ui.hyperlink_to(
                        "Go to the download page",
                        concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/latest"),
                    );
                }
            });
        });
        if let Some(Err(e)) = &self.update {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(e).color(t.error)).wrap());
        }
        if link_row(
            ui,
            t,
            "Privacy",
            "What the app reads, where it saves, and what it never does.",
        ) {
            self.go(Tab::Privacy);
        }
        if link_row(
            ui,
            t,
            "License",
            "GNU General Public License, version 3 or later: what you may do, and the warranty.",
        ) {
            self.go(Tab::License);
        }
    }
}

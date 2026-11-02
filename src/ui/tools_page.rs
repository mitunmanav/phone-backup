use crate::*;

/// The Tools page: each tool works on the backup chosen at the top.
impl App {
    pub(crate) fn adb_serial(&self) -> Option<String> {
        self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, .. } => Some(serial.clone()),
            _ => None,
        })
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
            let done = job(&say);
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

    /// The running job's progress and Stop, or what the panel's last job said.
    pub(crate) fn tool_note(&mut self, ui: &mut egui::Ui, t: &Tokens, which: &'static str) {
        if self.tool_busy == Some(which) {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.add(egui::Label::new(&self.tool_status).wrap());
            });
            let stopping = tools::STOP.load(Ordering::Relaxed);
            if which != "phone"
                && ui
                    .add_enabled(
                        !stopping,
                        egui::Button::new(if stopping { "Stopping…" } else { "Stop" }),
                    )
                    .clicked()
            {
                tools::STOP.store(true, Ordering::Relaxed);
            }
        } else if let Some((w, r)) = &self.tool_msg
            && *w == which
        {
            match r {
                Ok(m) => ui.label(m.as_str()),
                Err(e) => ui.label(RichText::new(e.as_str()).color(t.error)),
            };
        }
    }

    pub(crate) fn tools_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let muted = |s: &str| RichText::new(s).color(t.muted);
        ui.set_max_width(ui.available_width().min(640.0));
        let running = self.tool_busy.is_some();
        let label = |b: &backup::Saved| format!("{}, backed up {}", b.name, days_ago(b.age_days));
        panel(ui, t, "Backup", "", |ui| {
            if self.backups.is_empty() {
                ui.label(format!("No backups in {} yet.", self.scanned.display()));
            } else {
                let shown = self
                    .backups
                    .iter()
                    .find(|b| Some(&b.path) == self.chosen.as_ref())
                    .map_or("Choose a backup".into(), label);
                ui.add_enabled_ui(!running, |ui| {
                    egui::ComboBox::from_id_salt("tool_backup")
                        .width(ui.available_width())
                        .truncate()
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for b in &self.backups {
                                let on = Some(&b.path) == self.chosen.as_ref();
                                if ui.selectable_label(on, label(b)).clicked() && !on {
                                    self.chosen = Some(b.path.clone());
                                    self.dupes = None;
                                    self.tool_msg = None;
                                }
                            }
                        });
                });
            }
            ui.label(muted("The tools below work on this backup."));
            if ui
                .add_enabled(!running, egui::Button::new("Find another backup"))
                .clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_directory(&self.dest)
                    .pick_folder()
            {
                self.chosen = Some(dir.clone());
                self.scan(&dir);
                self.dupes = None;
                self.tool_msg = None;
            }
        });
        let folder = self.chosen.clone().filter(|_| !self.backups.is_empty());
        let can = folder.is_some() && !running;

        ui.add_space(8.0);
        panel(ui, t, "Duplicate files", "", |ui| {
            ui.label(muted("Finds photos and files saved more than once in this backup, such as pictures shared in several chats."));
            if ui
                .add_enabled(can, egui::Button::new("Find duplicates"))
                .clicked()
                && let Some(f) = folder.clone()
            {
                self.dupes = None;
                self.run_tool(ui.ctx(), "dupes", move |say| {
                    tools::Done::Dupes(tools::duplicates(&f, say))
                });
            }
            self.tool_note(ui, t, "dupes");
            let Some((groups, rows)) = &self.dupes else {
                return;
            };
            if groups.is_empty() {
                ui.label("No duplicates found.");
                return;
            }
            let ticked: Vec<(PathBuf, u64)> = groups
                .iter()
                .flatten()
                .filter(|(p, _)| self.ticked.contains(p))
                .cloned()
                .collect();
            let bytes: u64 = ticked.iter().map(|(_, n)| n).sum();
            ui.label(format!(
                "{} files are saved more than once. {} ticked to delete ({}); one copy of each stays.",
                count(groups.len()),
                count(ticked.len()),
                size(bytes)
            ));
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
                                let shown = path
                                    .strip_prefix(folder.as_deref().unwrap_or(path))
                                    .unwrap_or(path);
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
            let all_gone = groups
                .iter()
                .any(|g| g.iter().all(|(p, _)| self.ticked.contains(p)));
            if ui
                .add_enabled(
                    can && !ticked.is_empty() && !all_gone,
                    egui::Button::new("Delete ticked copies…"),
                )
                .on_disabled_hover_text(if all_gone {
                    "Leave at least one copy of each file unticked."
                } else {
                    "Tick the copies to delete."
                })
                .clicked()
            {
                self.confirm = Some(Confirm::Copies(ticked));
            }
        });

        ui.add_space(8.0);
        panel(ui, t, "HEIC photos to JPG", "", |ui| {
            ui.label(muted("Saves a JPG copy of each HEIC photo in the backup's jpg folder, keeping its date and location. Many Windows apps cannot open HEIC."));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can, egui::Button::new("Convert to JPG"))
                    .clicked()
                    && let Some(f) = folder.clone()
                {
                    self.run_tool(ui.ctx(), "heic", move |say| {
                        tools::Done::Text(tools::heic_to_jpg(&f, say))
                    });
                }
                if let Some(f) = &folder
                    && matches!(&self.tool_msg, Some(("heic", Ok(_))))
                    && ui.button("Open jpg folder").clicked()
                {
                    let _ = Command::new("explorer").arg(f.join("jpg")).spawn();
                }
            });
            self.tool_note(ui, t, "heic");
        });

        ui.add_space(8.0);
        panel(ui, t, "Duplicate contacts", "", |ui| {
            ui.label(muted(&format!("Merges contacts with the same name, phone number or email into {}. Your contacts.vcf stays as it was.", tools::MERGED)));
            let merged = matches!(&self.tool_msg, Some(("contacts", Ok(_))));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can, egui::Button::new("Merge duplicates"))
                    .clicked()
                    && let Some(f) = folder.clone()
                {
                    self.run_tool(ui.ctx(), "contacts", move |_| {
                        tools::Done::Text(tools::merge_contacts(&f))
                    });
                }
                if let Some(f) = &folder
                    && merged
                {
                    let data = f.join("data");
                    if ui.button("Open folder").clicked() {
                        let _ = Command::new("explorer").arg(&data).spawn();
                    }
                    if ui.button("Undo").clicked() {
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
                }
            });
            self.tool_note(ui, t, "contacts");
        });

        let serial = self.adb_serial();
        ui.add_space(8.0);
        panel(ui, t, "Free up space on the phone", "", |ui| {
            ui.label(muted("Deletes files from the phone that are safely in this backup. Only files whose backup copy has the same size are deleted, and the app asks first."));
            let chosen = self
                .free_pick
                .as_ref()
                .filter(|(f, _)| Some(f) == folder.as_ref())
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            ui.label(if chosen.is_empty() {
                "No files chosen yet.".to_string()
            } else {
                format!("{} chosen.", files(chosen.len()))
            });
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can, egui::Button::new("Choose files…"))
                    .clicked()
                    && let Some(f) = folder.clone()
                {
                    self.open_picker(ui.ctx(), f, true);
                }
                let ready = can && serial.is_some() && !chosen.is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("Delete from phone…"))
                    .on_disabled_hover_text(if serial.is_none() {
                        "Connect the phone with USB debugging on."
                    } else {
                        "Choose the files first."
                    })
                    .clicked()
                    && let (Some(f), Some(serial)) = (folder.clone(), serial.clone())
                {
                    self.run_tool(ui.ctx(), "phone", move |say| {
                        say("Checking the files on the phone…".into());
                        tools::Done::Matched(
                            tools::chosen_on_phone(&f, &chosen)
                                .and_then(|wanted| tools::match_on_phone(&serial, &wanted)),
                        )
                    });
                }
            });
            self.tool_note(ui, t, "phone");
        });

        ui.add_space(8.0);
        panel(ui, t, "See the phone's screen", "", |ui| {
            ui.label(muted("scrcpy, a free app by Genymobile, shows the phone's screen on this computer and lets you use it with the mouse and keyboard."));
            match &self.scrcpy {
                Some(exe) => {
                    if ui
                        .add_enabled(serial.is_some(), egui::Button::new("Open scrcpy"))
                        .on_disabled_hover_text("Connect the phone with USB debugging on.")
                        .clicked()
                    {
                        let mut c = Command::new(exe);
                        c.args(["-s", serial.as_deref().unwrap_or_default()]);
                        // One adb for both apps: two different versions stop each other's server.
                        if let Some(adb) = backup::adb_path() {
                            c.env("ADB", adb);
                        }
                        #[cfg(windows)]
                        std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000);
                        self.tool_msg = c
                            .spawn()
                            .err()
                            .map(|e| ("scrcpy", Err(format!("scrcpy did not start: {e}"))));
                    }
                }
                None => {
                    ui.label("scrcpy is not on this computer. Unzip it next to this app to use it from here.");
                    ui.hyperlink_to(
                        "Get scrcpy on GitHub",
                        "https://github.com/Genymobile/scrcpy",
                    );
                }
            }
            self.tool_note(ui, t, "scrcpy");
        });
    }

    /// Asks before a tool deletes anything; Esc or a click outside keeps the files.
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
                    "This frees {} on the phone. Their copies in the backup have the same size and stay on this computer. Deleted files cannot be brought back on the phone except by restoring them.",
                    size(list.iter().map(|(_, n)| n).sum())
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
                self.run_tool(ctx, "dupes", move |_| {
                    tools::Done::Text(tools::delete_copies(&list))
                });
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

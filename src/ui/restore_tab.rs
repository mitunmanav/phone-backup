use crate::*;

impl App {
    pub(crate) fn restore_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let phone = self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
            _ => None,
        });

        let found = match self.backups.len() {
            0 => String::new(),
            n => format!("{} found", count(n)),
        };
        panel(ui, t, "Choose a backup", &found, |ui| {
            if self.backups.is_empty() {
                ui.label(format!("No backups in {} yet.", self.scanned.display()));
                ui.label(muted(
                    "Back up a phone first, or find the folder that holds your backups.",
                ));
            }
            ui.add_enabled_ui(!busy, |ui| {
                // A long list scrolls in place, so Restore stays in view. Rows are one height and the
                // list shows a whole number of them, so no row is cut in half.
                // 2 + 22 (name) + 22 (note row) = 46, plus a little air.
                const ROW: f32 = 48.0;
                let gap = ui.spacing().item_spacing.y;
                let shown = self.backups.len().min(5) as f32;
                let height = shown * (ROW + gap) - gap;
                // Inside the page's own scroll area egui shrinks a list to the page space left in
                // view, cutting rows on short windows; the minimum makes the page scroll instead.
                egui::ScrollArea::vertical()
                    .id_salt("backups")
                    .max_height(height)
                    .min_scrolled_height(height)
                    .auto_shrink([false, true])
                    .show_rows(ui, ROW, self.backups.len(), |ui, rows| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        for b in &self.backups[rows] {
                            let label = Self::label(&self.backups, b);
                            let holds = match (b.files, b.apps) {
                                (true, true) => "Photos, files and apps",
                                (true, false) => "Photos and files",
                                _ => "Apps",
                            };
                            let width = ui.available_width();
                            ui.allocate_ui_with_layout(
                                vec2(width, ROW),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.set_min_size(vec2(width, ROW));
                                    ui.spacing_mut().interact_size.y = 22.0;
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.add_space(2.0);
                                    let on = self.chosen.as_ref() == Some(&b.path);
                                    if ui
                                        .radio(on, RichText::new(&label).font(semibold(14.0)))
                                        .on_hover_text(&label)
                                        .clicked()
                                    {
                                        self.chosen = Some(b.path.clone());
                                    }
                                    ui.horizontal(|ui| {
                                        ui.add_space(
                                            ui.spacing().icon_width + ui.spacing().icon_spacing,
                                        );
                                        ui.label(
                                            // Size and date first: on a narrow window the end is cut.
                                            RichText::new(match b.bytes {
                                                Some(n) => format!(
                                                    "{}, backed up {}. {holds}.",
                                                    size(n),
                                                    days_ago(b.age_days)
                                                ),
                                                None => format!(
                                                    "Backed up {}. {holds}.",
                                                    days_ago(b.age_days)
                                                ),
                                            })
                                            .color(t.muted)
                                            .size(12.0),
                                        );
                                    });
                                },
                            );
                        }
                    });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if let Some(path) = self.chosen.clone().filter(|_| !self.backups.is_empty()) {
                        if ui.button("Open folder").clicked() {
                            let _ = Command::new("explorer").arg(&path).spawn();
                        }
                        if ui.button("Rename…").clicked() {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            self.ask = Some(Ask::Rename(path.clone(), name, None));
                        }
                        if self.backups.len() > 1 && ui.button("Copy into…").clicked() {
                            self.ask = Some(Ask::Copy(path.clone(), None));
                        }
                        if ui.button("Delete…").clicked() {
                            self.ask = Some(Ask::Delete(path));
                        }
                    }
                    if ui.button("Find another backup").clicked()
                        && let Some(dir) = rfd::FileDialog::new()
                            .set_directory(&self.dest)
                            .pick_folder()
                    {
                        self.chosen = Some(dir.clone());
                        self.scan(&dir);
                    }
                    if let Some(path) = self.chosen.clone()
                        && ui.button("Verify backup").clicked()
                    {
                        self.verify(ui.ctx(), path);
                    }
                    let chosen = self.chosen.clone().filter(|c| self.backups.iter().any(|b| &b.path == c));
                    if ui
                        .add_enabled(chosen.is_some(), egui::Button::new("Export"))
                        .on_hover_text("Saves contacts, calls and messages as files you can open, print or import, and sorts photos into year and month folders. The backup itself is not changed.")
                        .on_disabled_hover_text("Choose a backup first")
                        .clicked()
                        && let Some(folder) = chosen
                    {
                        self.export(ui.ctx(), folder);
                    }
                });
            });
        });

        let chosen = self
            .backups
            .iter()
            .find(|b| Some(&b.path) == self.chosen.as_ref());
        let (has_files, has_apps) = chosen.map_or((false, false), |b| (b.files, b.apps));
        let only = self
            .only
            .as_ref()
            .filter(|(folder, ..)| Some(folder) == chosen.map(|b| &b.path));
        let mut pick = false;
        ui.add_space(8.0);
        panel(ui, t, "What to restore", "", |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                let can_pick = has_files && self.restore_files;
                let note = match only {
                    Some((_, chosen, total)) if can_pick => format!(
                        "{} of {} chosen. Files the phone already has are skipped.",
                        count(chosen.len()),
                        files(*total)
                    ),
                    _ if can_pick => "All files. Files the phone already has are skipped.".into(),
                    _ => "Files the phone already has are skipped, never replaced.".into(),
                };
                pick = choice(
                    ui,
                    t,
                    has_files,
                    &mut self.restore_files,
                    "Photos, videos and files",
                    &note,
                    can_pick.then_some("Choose files…"),
                );
                choice(
                    ui,
                    t,
                    has_apps,
                    &mut self.restore_apps,
                    "Apps",
                    "Reinstalls missing apps. Their logins and data do not come back.",
                    None,
                );
            });
        });
        let only = only.map(|(_, chosen, _)| chosen.clone());
        let files_on =
            self.restore_files && has_files && only.as_ref().is_none_or(|o| !o.is_empty());
        let picked = files_on || (self.restore_apps && has_apps);
        let path = chosen.map(|b| b.path.clone());
        if pick && let Some(path) = &path {
            self.open_picker(ui.ctx(), path.clone(), false);
        }

        // A ticked box the chosen backup has nothing for is ignored, not run.
        let plan: Option<Plan> = match (&phone, &path) {
            (Some((serial, _)), Some(path)) if picked => Some((
                serial.clone(),
                path.clone(),
                files_on,
                self.restore_apps && has_apps,
                only.filter(|_| files_on),
            )),
            _ => None,
        };
        // Any change to the phone, backup or ticks after a check means checking again.
        let checked = matches!(self.preview_for(plan.as_ref()), Some(Ok(_)));
        let title = match &phone {
            _ if busy && self.job == Job::Verify => "Checking the backup…".into(),
            _ if busy && self.job == Job::Copy => "Copying between backups…".into(),
            _ if busy && self.job == Job::Delete => "Deleting the backup…".into(),
            _ if busy && self.checking.is_some() => "Checking…".into(),
            _ if busy && self.job == Job::Export => "Exporting…".into(),
            _ if busy => "Restoring…".into(),
            None => "Connect the phone with USB debugging on to restore to it".into(),
            _ if path.is_none() => "Choose a backup to restore".into(),
            _ if !picked => "Tick at least one thing to restore".into(),
            Some((_, model)) => format!("Restores to {model}"),
        };
        Cmd {
            title,
            note: if busy && self.job == Job::Verify {
                "Nothing in the backup or on the phone changes.".into()
            } else if busy && self.job == Job::Export {
                "The backup itself is not changed.".into()
            } else if busy && self.job == Job::Restore {
                "Keep the phone plugged in and unlocked.".into()
            } else if plan.is_some() && !busy {
                "Nothing changes on the phone until you press Restore now.".into()
            } else {
                String::new()
            },
            label: if checked {
                "Restore now"
            } else {
                "Check restore"
            },
            enabled: plan.is_some(),
            plan: plan.map(|p| (p, !checked)),
            open: None,
            again: false,
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
                    self.open_restore_report(ui);
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

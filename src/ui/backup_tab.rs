use crate::*;

impl App {
    /// Progress of the running job; Stop is in the command bar.
    pub(crate) fn progress(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        if let Some(f) = self.fraction {
            ui.add(
                egui::ProgressBar::new(f)
                    .fill(t.accent)
                    .desired_height(10.0)
                    .corner_radius(5),
            );
        }
        ui.horizontal(|ui| {
            if self.fraction.is_none() {
                ui.add(egui::Spinner::new().size(16.0));
            }
            ui.add(egui::Label::new(&self.status).wrap());
        });
        let spent = self.progress_from.elapsed().as_secs_f32();
        // A few seconds in, so the first batch does not give a wild guess.
        if let Some(f) = self.fraction.filter(|f| *f > 0.0 && spent > 5.0) {
            // shortcut: estimate by file count, not bytes; switch to bytes if users see bad estimates
            let left = spent * (1.0 - f) / f;
            ui.label(RichText::new(time_left(left as u64)).color(t.muted));
        }
        if !self.now.is_empty() {
            ui.add(egui::Label::new(RichText::new(&self.now).color(t.muted)).truncate());
        }
    }

    pub(crate) fn backup_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let (adb, mtp) = self.connected();
        let ready = !adb.is_empty() || mtp.is_some();
        // File transfer mode can only reach files.
        let mtp_only = adb.is_empty() && mtp.is_some();

        let measured = self.measured();
        let about = |bytes: u64| format!(" About {}.", size(bytes));
        let files_size = measured.map(|(_, t)| about(t.total)).unwrap_or_default();
        let apps_size = measured.map(|(apps, _)| about(apps)).unwrap_or_default();
        let limits = match self.pick.limits() {
            l if l.is_empty() => "Everything in the phone's storage and SD card.".into(),
            l => l,
        };
        let can_choose = self.pick.files;
        panel(ui, t, "What to back up", "", |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                if choice(
                    ui,
                    t,
                    true,
                    &mut self.pick.files,
                    "Photos, videos and files",
                    &format!("{limits}{files_size}"),
                    can_choose.then_some("Choose…"),
                ) {
                    self.choosing = true;
                    self.measure(ui.ctx());
                }
                choice(
                    ui,
                    t,
                    !mtp_only,
                    &mut self.pick.apps,
                    "Apps",
                    &format!("The apps you installed, without their logins or data.{apps_size}"),
                    None,
                );
                choice(
                    ui,
                    t,
                    !mtp_only,
                    &mut self.pick.data,
                    "Contacts, calls, messages and calendar",
                    "Some phones do not allow this over USB.",
                    None,
                );
            });
            // Where it goes; the folder itself is changed in Settings.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let full = self.dest.display().to_string();
                ui.label(muted("Saves to"));
                ui.scope(|ui| {
                    ui.set_max_width(ui.available_width() - 64.0);
                    ui.add(egui::Label::new(muted(&short_path(ui, &full))).truncate())
                        .on_hover_text(&full);
                });
                if ui.add_enabled(!busy, egui::Link::new("Change…")).clicked() {
                    self.go(Tab::Settings);
                }
            });
        });
        let files_on = self.pick.files && self.pick.kinds.contains(&true);
        let picked = files_on || (!mtp_only && (self.pick.apps || self.pick.data));
        let name = match (adb.as_slice(), mtp) {
            ([(model, _)], _) => model.clone(),
            ([_, ..], _) => format!("{} phones", adb.len()),
            (_, Some(name)) => name,
            _ => String::new(),
        };
        Cmd {
            title: if busy {
                "Backing up…".into()
            } else if !ready {
                "Connect a phone to start".into()
            } else if !picked {
                "Tick at least one thing to back up".into()
            } else {
                format!("Ready to back up {name}")
            },
            note: if busy {
                "Keep the phone plugged in and unlocked.".into()
            } else {
                String::new()
            },
            label: if self.result.is_some() {
                "Back up again"
            } else {
                "Back up now"
            },
            enabled: ready && picked,
            plan: None,
        }
    }

    /// The first USB-debugging phone's serial and model.
    pub(crate) fn adb_phone(&self) -> Option<(String, String)> {
        self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
            _ => None,
        })
    }

    /// Reads the sizes on the phone in the background, unless they are known or being read.
    pub(crate) fn measure(&mut self, ctx: &egui::Context) {
        let Some((serial, _)) = self.adb_phone() else {
            return;
        };
        if self.fixture != Fixture::Real || self.sizes.as_ref().is_some_and(|(s, _)| *s == serial) {
            return;
        }
        self.sizes = Some((serial.clone(), None));
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let (list, apps) = backup::measure(&serial);
            let _ = tx.send(Msg::Sizes(serial, list, apps));
            ctx.request_repaint();
        });
    }

    /// The connected phone's apps size and the sizes for the current choices, once measured.
    pub(crate) fn measured(&mut self) -> Option<(u64, &Tally)> {
        let serial = self.adb_phone().map(|p| p.0);
        let Some((s, Some((list, apps)))) = &self.sizes else {
            return None;
        };
        if Some(s) != serial.as_ref() {
            return None;
        }
        if self.tally.as_ref().is_none_or(|(p, _)| *p != self.pick) {
            self.tally = Some((self.pick.clone(), backup::tally(list, &self.pick)));
        }
        Some((*apps, &self.tally.as_ref()?.1))
    }

    /// Folders, types and the size limit for photos, videos and files, with the size of each.
    pub(crate) fn choose_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        let serial = self.adb_phone().map(|p| p.0);
        let reading = matches!(&self.sizes, Some((s, None)) if Some(s) == serial.as_ref());
        let tally = self
            .measured()
            .map(|(_, t)| (t.folders.clone(), t.kinds, t.total, t.count));
        let pick = &mut self.pick;
        // A tick box on the left and the size of that choice on the right; long names are cut.
        let row =
            |ui: &mut egui::Ui, on: &mut bool, enabled: bool, label: String, bytes: Option<u64>| {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(b) = bytes {
                            ui.label(RichText::new(size(b)).color(t.muted));
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            ui.add_enabled(enabled, egui::Checkbox::new(on, label))
                                .changed()
                        })
                        .inner
                    })
                    .inner
                })
                .inner
            };
        let heading = |ui: &mut egui::Ui, s: &str| {
            ui.add_space(4.0);
            ui.label(RichText::new(s).font(semibold(14.0)));
        };
        let close = dialog(
            ctx,
            "choose",
            "Choose files to back up",
            480.0,
            "Done",
            |ui| {
                heading(ui, "Types");
                for (k, name) in KINDS.iter().enumerate() {
                    let bytes = tally.as_ref().map(|t| t.1[k]);
                    row(ui, &mut pick.kinds[k], true, (*name).into(), bytes);
                }
                heading(ui, "Folders");
                match &tally {
                    Some((sizes, ..)) => {
                        let mut all = pick.folders.is_empty();
                        if ui.checkbox(&mut all, "All folders").changed() {
                            // Unticking starts with every folder ticked, so one can be left out.
                            pick.folders = if all {
                                BTreeSet::new()
                            } else {
                                sizes.keys().cloned().collect()
                            };
                        }
                        // Folders chosen on an earlier run stay listed even when this phone lacks them.
                        let mut names: Vec<String> = sizes
                            .keys()
                            .chain(&pick.folders)
                            .cloned()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        // Files not in a folder go last, after the folders.
                        names.sort_by_key(|n| n.is_empty());
                        let last = pick.folders.len() == 1;
                        for name in names {
                            let mut on = all || pick.folders.contains(&name);
                            // The last ticked folder stays: none ticked would mean all again.
                            let enabled = !all && !(last && on);
                            let bytes = sizes.get(&name).copied();
                            let label = match name.as_str() {
                                "" => "Files not in a folder".into(),
                                n => n.into(),
                            };
                            if row(ui, &mut on, enabled, label, bytes) {
                                if on {
                                    pick.folders.insert(name);
                                } else {
                                    pick.folders.remove(&name);
                                }
                            }
                        }
                    }
                    None if reading => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reading the phone…");
                        });
                    }
                    None => {
                        ui.label(
                            RichText::new("Turn on USB debugging to choose folders and see sizes.")
                                .color(t.muted),
                        );
                    }
                }
                heading(ui, "Size");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Skip files over");
                    let label = |m: Option<u64>| m.map_or("No limit".into(), size);
                    egui::ComboBox::from_id_salt("max_size")
                        .selected_text(label(pick.max_size))
                        .show_ui(ui, |ui| {
                            for m in [
                                None,
                                Some(100_000_000),
                                Some(500_000_000),
                                Some(1_000_000_000),
                                Some(4_000_000_000),
                            ] {
                                ui.selectable_value(&mut pick.max_size, m, label(m));
                            }
                        });
                });
                if let Some((.., total, n)) = tally {
                    ui.add_space(8.0);
                    ui.label(format!(
                        "About {} in {} will be backed up.",
                        size(total),
                        files(n)
                    ));
                }
            },
        );
        self.choosing = !close;
    }

    /// What the last backup did.
    pub(crate) fn backup_result(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let muted = |s: &str| RichText::new(s).color(t.muted);
        match &self.result {
            Some(Err(e)) => {
                ui.add_space(16.0);
                result_box(t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Backup stopped").font(semibold(16.0)).color(t.error));
                    ui.label(e);
                    ui.label(RichText::new("Check the cable and that the phone is unlocked, then press Back up again.").color(t.muted));
                });
            }
            Some(Ok(summaries)) => {
                for s in summaries {
                    ui.add_space(16.0);
                    result_box(t).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let title = match (s.errors.is_empty(), summaries.len() > 1) {
                            (true, false) => "Backup finished".to_string(),
                            (false, false) => "Backup finished with problems".to_string(),
                            (true, true) => format!("{}: backup finished", s.name),
                            (false, true) => format!("{}: backup finished with problems", s.name),
                        };
                        ui.label(RichText::new(title).font(semibold(16.0)));
                        let in_backup = files(s.files - s.errors.len().min(s.files));
                        if s.pick.files || s.pick.apps {
                            ui.label(if s.files == 0 {
                                "No files were found on the phone.".to_string()
                            } else if s.copied == 0 {
                                format!("Nothing new to copy. The backup already holds {in_backup}.")
                            } else {
                                format!("Copied {} that {} new or changed. The backup now holds {in_backup}.", files(s.copied), backup::were(s.copied))
                            });
                        }
                        if !s.errors.is_empty() {
                            ui.label(
                                RichText::new(format!("{} could not be copied. Press Back up again to retry. The list is in report.txt.", files(s.errors.len())))
                                    .color(t.error),
                            );
                        }
                        if s.full {
                            let mut saved: Vec<_> = s.pick.apps.then(|| "installed apps".to_string()).into_iter().collect();
                            saved.extend(s.data.iter().filter(|d| d.1).map(|d| d.0.to_lowercase()));
                            let refused: Vec<_> = s.data.iter().filter(|d| !d.1).map(|d| d.0.to_lowercase()).collect();
                            if !saved.is_empty() {
                                ui.label(format!("Also saved: {}.", saved.join(", ")));
                                if saved.iter().any(|d| d == "contacts") {
                                    ui.label(muted("Contacts are in data\\contacts.vcf, which Google Contacts or any phone can import."));
                                }
                            }
                            if !refused.is_empty() {
                                ui.label(format!("Your phone did not allow reading {}.", or_list(&refused)));
                            }
                        } else {
                            ui.label("Apps, contacts and messages were not saved. Turn on USB debugging to include them.");
                        }
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Open backup folder").clicked() {
                                let _ = Command::new("explorer").arg(&s.root).spawn();
                            }
                            if ui.button("Open report").clicked() {
                                let _ = Command::new("explorer").arg(s.root.join(backup::REPORT)).spawn();
                            }
                            if !s.errors.is_empty() && ui.button("Copy problem list").clicked() {
                                ui.ctx().copy_text(s.errors.join("\n"));
                            }
                        });
                    });
                }
            }
            None => {}
        }
    }
}

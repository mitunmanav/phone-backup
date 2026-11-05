use crate::*;

impl App {
    /// Phone to phone: which phone to copy from and to, and what to copy.
    pub(crate) fn transfer_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let adb: Vec<(String, String)> = self
            .phones
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
                _ => None,
            })
            .collect();
        // Two phones of the same model: their serials tell them apart.
        let name = |serial: &Option<String>| {
            let Some((s, m)) = adb.iter().find(|p| Some(&p.0) == serial.as_ref()) else {
                return String::new();
            };
            match adb.iter().filter(|p| p.1 == *m).count() {
                1 => m.clone(),
                _ => format!("{m} ({s})"),
            }
        };
        // Keep both choices on connected, different phones as phones come and go.
        let here = |s: &Option<String>| adb.iter().any(|p| Some(&p.0) == s.as_ref());
        if !here(&self.from) {
            self.from = adb.first().map(|p| p.0.clone());
        }
        if !here(&self.to) || self.to == self.from {
            self.to = adb
                .iter()
                .find(|p| Some(&p.0) != self.from.as_ref())
                .map(|p| p.0.clone());
        }
        let ready = adb.len() >= 2;

        panel(ui, t, "Phones", "", |ui| {
            if !ready {
                ui.label("Connect both phones with USB debugging on.");
                ui.label(muted(
                    "Each one shows up here a moment after you tap Allow on it.",
                ));
                return;
            }
            ui.add_enabled_ui(!busy, |ui| {
                let (from, to) = (self.from.clone(), self.to.clone());
                for (label, slot) in [("From", &mut self.from), ("To", &mut self.to)] {
                    ui.horizontal(|ui| {
                        ui.add_sized([40.0, 32.0], egui::Label::new(label));
                        let shown = name(slot);
                        egui::ComboBox::from_id_salt(label)
                            .width(ui.available_width())
                            .truncate()
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for (s, _) in &adb {
                                    ui.selectable_value(
                                        slot,
                                        Some(s.clone()),
                                        name(&Some(s.clone())),
                                    );
                                }
                            });
                    });
                }
                // Choosing the other phone on one side swaps the two.
                if self.from == self.to {
                    if self.from != from {
                        self.to = from;
                    } else {
                        self.from = to;
                    }
                }
                if ui.button("Swap").clicked() {
                    std::mem::swap(&mut self.from, &mut self.to);
                }
            });
            ui.label(muted(
                "The first phone is backed up into its own folder, then copied to the second.",
            ));
        });

        ui.add_space(8.0);
        let to_name = name(&self.to);
        panel(ui, t, "What to copy", "", |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                choice(
                    ui,
                    t,
                    true,
                    &mut self.restore_files,
                    "Photos, videos and files",
                    "Files the second phone already has are skipped, never replaced.",
                    None,
                );
                choice(
                    ui,
                    t,
                    true,
                    &mut self.restore_apps,
                    "Apps",
                    "Installs the apps it does not have. Their logins and data do not move.",
                    None,
                );
                choice(
                    ui,
                    t,
                    true,
                    &mut self.safety,
                    "Back up the second phone first",
                    "A safety copy in its own backup folder, before anything changes on it.",
                    None,
                );
            });
            ui.label(muted(
                "Contacts and messages cannot be copied to a phone yet.",
            ));
        });

        let picked = self.restore_files || self.restore_apps;
        let folder = self
            .source
            .as_ref()
            .filter(|(s, ..)| Some(s) == self.from.as_ref())
            .map(|(_, f, ran)| (f.clone(), ran.clone()));
        let plan: Option<Plan> = match (&self.to, folder) {
            (Some(to), Some((folder, ran))) if ready && picked => Some((
                to.clone(),
                folder,
                self.restore_files,
                self.restore_apps,
                Some(ran),
            )),
            _ => None,
        };
        let checked = matches!(self.preview_for(plan.as_ref()), Some(Ok(_)));
        let from_name = name(&self.from);
        Cmd {
            title: if busy && self.checking.is_some() {
                format!("Backing up {from_name} and checking…")
            } else if busy {
                format!("Copying to {to_name}…")
            } else if !ready {
                "Connect two phones to start".into()
            } else if !picked {
                "Tick at least one thing to copy".into()
            } else {
                format!("Copies {from_name} to {to_name}")
            },
            note: if busy {
                "Keep both phones plugged in and unlocked.".into()
            } else if !ready || !picked {
                String::new()
            } else if checked {
                format!("Nothing changes on {to_name} until you press Transfer now.")
            } else {
                "The check backs up the first phone and changes nothing on either.".into()
            },
            label: if checked {
                "Transfer now"
            } else {
                "Check transfer"
            },
            enabled: ready && picked,
            plan: plan.map(|p| (p, !checked)),
            open: None,
            again: false,
        }
    }
}

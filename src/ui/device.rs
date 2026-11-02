use crate::*;

/// Public steps to turn on USB debugging, per phone maker.
pub(crate) const BRANDS: [(&str, &[&str]); 5] = [
    (
        "Samsung",
        &[
            "Settings, About phone, Software information: tap Build number 7 times.",
            "Settings, Developer options: turn on USB debugging.",
        ],
    ),
    (
        "Xiaomi, Redmi, POCO",
        &[
            "Settings, About phone: tap OS version (or MIUI version) 7 times.",
            "Settings, Additional settings, Developer options: turn on USB debugging. To restore apps, also turn on Install via USB.",
        ],
    ),
    (
        "OnePlus, OPPO, realme",
        &[
            "Settings, About device (or About phone), Version: tap Build number 7 times.",
            "Settings, System settings (or Additional settings), Developer options: turn on USB debugging.",
        ],
    ),
    (
        "Vivo",
        &[
            "Settings, About phone: tap Software version 7 times.",
            "Settings, System management (or More settings), Developer options: turn on USB debugging.",
        ],
    ),
    (
        "Pixel and others",
        &[
            "Settings, About phone: tap Build number 7 times.",
            "Settings, System, Developer options: turn on USB debugging.",
        ],
    ),
];

impl App {
    /// (model, Wi-Fi?) of each ADB phone, and the name of a phone in File transfer mode.
    pub(crate) fn connected(&self) -> (Vec<(String, bool)>, Option<String>) {
        let phones = self.phones.iter().flatten();
        let adb = phones
            .clone()
            .filter_map(|p| match p {
                Phone::Adb { model, wifi, .. } => Some((model.clone(), *wifi)),
                _ => None,
            })
            .collect();
        let mtp = phones.clone().find_map(|p| match p {
            Phone::Mtp { name } => Some(name.clone()),
            _ => None,
        });
        (adb, mtp)
    }

    /// The phone, how it is connected, and how to connect one.
    pub(crate) fn device(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let (adb, mtp) = self.connected();
        let allow = self
            .phones
            .as_ref()
            .is_some_and(|p| p.contains(&Phone::Unauthorized));
        // A filled plum dot once a phone is connected, an empty grey ring until then.
        let heading = |ui: &mut egui::Ui, text: &str| {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                if adb.is_empty() && mtp.is_none() {
                    ui.painter()
                        .circle_stroke(dot.center(), 4.5, Stroke::new(1.5, t.border));
                } else {
                    ui.painter().circle_filled(dot.center(), 5.0, t.accent);
                }
                ui.heading(text);
            });
        };
        match (&self.phones, adb.as_slice(), &mtp) {
            (None, ..) => {
                heading(ui, "Looking for your phone");
            }
            (Some(_), [(model, wifi)], _) => {
                heading(ui, model);
                ui.label(if *wifi {
                    "Connected over Wi-Fi."
                } else {
                    "Connected by USB cable."
                });
            }
            (Some(_), [_, ..], _) => {
                heading(ui, &format!("{} phones connected", adb.len()));
                ui.label(
                    adb.iter()
                        .map(|(m, _)| m.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                ui.label(muted("Each phone gets its own backup folder."));
            }
            (Some(_), [], Some(n)) => {
                heading(ui, n);
                ui.label(
                    "Connected in File transfer mode, so only photos and files can be backed up.",
                );
                if self.adb_ok {
                    ui.label(muted(
                        "Turn on USB debugging to include apps, contacts and messages.",
                    ));
                } else {
                    ui.label(muted("USB debugging also needs adb, Google's free tool."));
                    self.adb_missing(ui, t);
                }
                self.phone_help(ui, t);
            }
            (Some(_), [], None) if allow => {
                heading(ui, "Tap Allow on your phone");
                ui.label(
                    "Unlock the phone. It asks whether to allow USB debugging from this computer.",
                );
                self.phone_help(ui, t);
            }
            (Some(_), [], None) => {
                heading(ui, "Connect your phone");
                if self
                    .phones
                    .as_ref()
                    .is_some_and(|p| p.contains(&Phone::UsbFailed))
                {
                    ui.label(
                        RichText::new(
                            "Windows found a USB device it cannot use. If that is your phone, unplug it, unlock it and plug it in again. Still nothing? Try another cable and USB port: some cables only charge.",
                        )
                        .color(t.error),
                    );
                }
                steps(
                    ui,
                    t,
                    &[
                        "Plug it into this computer with a USB cable.",
                        "Unlock it, tap the USB notification and choose File transfer.",
                        "For apps, contacts and messages, also turn on USB debugging.",
                    ],
                );
                if !self.adb_ok {
                    ui.label(muted("USB debugging also needs adb, Google's free tool."));
                    self.adb_missing(ui, t);
                }
                self.phone_help(ui, t);
            }
        }

        let low: Vec<(String, u8)> = self
            .phones
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Phone::Adb {
                    model,
                    battery: Some(b),
                    ..
                } => Some((model.clone(), *b)),
                _ => None,
            })
            .collect();
        for (model, level) in &low {
            let who = if adb.len() > 1 {
                format!("{model}: battery")
            } else {
                "Battery".into()
            };
            ui.label(
                RichText::new(format!(
                    "{who} at {level}% and not charging. Charge the phone so it does not switch off mid-backup."
                ))
                .color(t.error),
            );
        }

        // Wi-Fi is a one-time setup, so it opens in a dialog instead of stretching this card.
        ui.add_space(12.0);
        if ui
            .add_enabled(!busy, egui::Button::new("Connect over Wi-Fi…"))
            .clicked()
        {
            self.wifi_open = true;
        }
    }

    /// How to turn on USB debugging per phone maker, and what to try when no phone shows up.
    /// Folded away: most people need neither.
    pub(crate) fn phone_help(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        // Fixture screenshots open them; there is no click to do it.
        let open = self.fixture == Fixture::Empty;
        ui.add_space(4.0);
        egui::CollapsingHeader::new("How to turn on USB debugging")
            .default_open(open)
            .show(ui, |ui| self.brand_steps(ui, t));
        egui::CollapsingHeader::new("Phone not showing up?")
            .default_open(open)
            .show(ui, |ui| {
                steps(
                    ui,
                    t,
                    &[
                        "Try another cable: some only charge. Plug it straight into the computer, not a hub.",
                        "Unlock the phone and keep the screen on.",
                        "Tap the USB notification on the phone and choose File transfer.",
                        "With USB debugging on, tap Allow when the phone asks. No question? In Developer options, tap Revoke USB debugging authorizations, then plug it in again.",
                        "Some phones need a USB driver on Windows: get it from the phone maker's website.",
                    ],
                );
            });
    }

    /// USB debugging only helps once adb is on this computer. Google's terms come before its
    /// download, and the manual route stays for when the download does not work.
    pub(crate) fn adb_missing(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        if self.adb_working {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label("Getting adb from Google…");
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.adb_agreed, "I agree to Google's terms");
            ui.hyperlink_to(
                "Read the terms",
                "https://developer.android.com/studio/terms",
            );
        });
        let get = ui
            .add_enabled(self.adb_agreed, egui::Button::new("Get adb (8 MB)"))
            .on_disabled_hover_text("Agree to Google's terms first");
        if get.clicked() {
            self.adb_working = true;
            self.adb_err = None;
            let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
            thread::spawn(move || {
                let _ = tx.send(Msg::GotAdb(backup::get_adb()));
                ctx.request_repaint();
            });
        }
        if let Some(e) = &self.adb_err {
            ui.add(egui::Label::new(RichText::new(e).color(t.error)).wrap());
            ui.add(egui::Label::new("Or put adb.exe next to this app yourself:").wrap());
            ui.hyperlink_to(
                "Download Platform Tools",
                "https://developer.android.com/tools/releases/platform-tools",
            );
        }
    }

    /// The phone maker picker and its steps to turn on USB debugging.
    pub(crate) fn brand_steps(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        egui::ComboBox::from_label("Phone brand")
            .selected_text(BRANDS[self.brand].0)
            .show_ui(ui, |ui| {
                for (i, (name, _)) in BRANDS.iter().enumerate() {
                    ui.selectable_value(&mut self.brand, i, *name);
                }
            });
        steps(ui, t, BRANDS[self.brand].1);
    }
}

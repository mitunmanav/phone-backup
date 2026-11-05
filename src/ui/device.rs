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

    /// How the phone with this serial stands, in a few words.
    pub(crate) fn phone_note(&self, serial: &str) -> String {
        let found = self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb {
                serial: s,
                wifi,
                level,
                ..
            } if s == serial => Some((*wifi, *level)),
            _ => None,
        });
        match found {
            Some((false, Some(l))) => format!("{l}% charged"),
            Some((true, Some(l))) => format!("{l}% charged, over Wi-Fi"),
            Some((true, None)) => "Connected over Wi-Fi".into(),
            _ => "Connected by USB cable".into(),
        }
    }

    /// One line for each phone low on battery and not charging.
    pub(crate) fn battery_warnings(&self, ui: &mut egui::Ui, t: &Tokens) {
        let many = self.connected().0.len() > 1;
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
            let who = if many {
                format!("{model}: battery")
            } else {
                "Battery".into()
            };
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{who} at {level}% and not charging. Charge the phone so it does not switch off mid-backup."
                    ))
                    .color(t.warning),
                )
                .wrap(),
            );
        }
    }

    /// Connect over Wi-Fi and a report button per phone, drawn by `button`, with how the report
    /// is going.
    pub(crate) fn phone_actions(
        &mut self,
        ui: &mut egui::Ui,
        t: &Tokens,
        button: &dyn Fn(&mut egui::Ui, &str, bool) -> egui::Response,
    ) {
        let busy = self.busy.load(Ordering::Relaxed);
        ui.horizontal_wrapped(|ui| {
            if button(ui, "Connect over Wi-Fi…", !busy).clicked() {
                self.wifi_open = true;
            }
            self.report_buttons(ui, busy, button);
        });
        if self.report_working {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label("Reading the phone…");
            });
        }
        if let Some(e) = &self.report_err {
            ui.add(egui::Label::new(RichText::new(e).color(t.error)).wrap());
        }
    }

    /// A button per ADB phone to save a page about it, for printing or showing a buyer.
    fn report_buttons(
        &mut self,
        ui: &mut egui::Ui,
        busy: bool,
        button: &dyn Fn(&mut egui::Ui, &str, bool) -> egui::Response,
    ) {
        let phones: Vec<(String, String)> = self
            .phones
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
                _ => None,
            })
            .collect();
        for (serial, model) in &phones {
            let label = if phones.len() > 1 {
                format!("Save report for {model}…")
            } else {
                "Save phone report…".into()
            };
            if button(ui, &label, !busy && !self.report_working).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_directory(&self.dest)
                    .set_file_name("phone-report.txt")
                    .save_file()
            {
                self.report_working = true;
                self.report_err = None;
                let (tx, ctx, serial) = (self.tx.clone(), ui.ctx().clone(), serial.clone());
                thread::spawn(move || {
                    let r = backup::phone_report(&serial, &path);
                    if r.is_ok() {
                        let _ = Command::new("explorer").arg(&path).spawn();
                    }
                    let _ = tx.send(Msg::PhoneReport(r));
                    ctx.request_repaint();
                });
            }
        }
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

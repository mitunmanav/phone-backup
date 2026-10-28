// No console window behind the app on Windows.
#![windows_subsystem = "windows"]

mod backup;

use backup::{Msg, Phone, Pick, Summary};
use eframe::egui::{self, vec2, Align, Color32, FontFamily, FontId, Layout, RichText, Stroke, TextStyle};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use std::{env, fs, thread};

struct Tokens {
    bg: Color32,
    field: Color32,
    line: Color32,
    ink: Color32,
    muted: Color32,
    accent: Color32,
    on_accent: Color32,
    error: Color32,
}

fn tokens(dark: bool) -> Tokens {
    let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if dark {
        Tokens {
            bg: hex(0x10161C),
            field: hex(0x18212A),
            line: hex(0x2C3946),
            ink: hex(0xE6ECF1),
            muted: hex(0x93A1AE),
            accent: hex(0xD08BC6),
            on_accent: hex(0x10161C),
            error: hex(0xFF9D6B),
        }
    } else {
        Tokens {
            bg: hex(0xE9EDF1),
            field: hex(0xFFFFFF),
            line: hex(0xC5CDD5),
            ink: hex(0x16202B),
            muted: hex(0x56616D),
            accent: hex(0x7A2E6E),
            on_accent: hex(0xFFFFFF),
            error: hex(0xA8380A),
        }
    }
}

fn visuals(dark: bool) -> egui::Visuals {
    let t = tokens(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.bg;
    v.window_fill = t.field;
    v.extreme_bg_color = t.field;
    v.override_text_color = Some(t.ink);
    v.hyperlink_color = t.accent;
    v.selection.bg_fill = t.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(2.0, t.accent);
    v.widgets.inactive.weak_bg_fill = t.field;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.line);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.accent);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 8.into();
    }
    v
}

fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// 1280 -> "1,280"
fn count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 1 -> "1 file", 1280 -> "1,280 files"
fn files(n: usize) -> String {
    format!("{} {}", count(n), if n == 1 { "file" } else { "files" })
}

const WORKING: &str = "Working";

/// "192.168.1.5:37145" or "phone.local:5555": a host, a colon and a port number.
fn valid_addr(addr: &str) -> bool {
    addr.rsplit_once(':').is_some_and(|(host, port)| !host.is_empty() && !host.contains(' ') && port.parse::<u16>().is_ok_and(|p| p > 0))
}

/// USB debugging only helps once adb is on this computer.
fn adb_missing(ui: &mut egui::Ui, t: &Tokens) {
    ui.label(RichText::new("USB debugging also needs Google's free Platform Tools on this laptop. Unzip them and put adb.exe next to this app.").color(t.muted));
    ui.hyperlink_to("Download Platform Tools", "https://developer.android.com/tools/releases/platform-tools");
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Phone Backup")
            .with_inner_size([520.0, 660.0])
            .with_min_inner_size([360.0, 440.0]),
        ..Default::default()
    };
    eframe::run_native("Phone Backup", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

struct App {
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    busy: Arc<AtomicBool>,
    /// None until the first check has finished.
    phones: Option<Vec<Phone>>,
    adb_ok: bool,
    dest: PathBuf,
    pick: Pick,
    status: String,
    now: String,
    fraction: Option<f32>,
    result: Option<Result<Vec<Summary>, String>>,
    pair_addr: String,
    pair_code: String,
    connect_addr: String,
    wifi_msg: String,
    /// Asking whether to stop a running backup before closing.
    confirm_close: bool,
    /// Close as soon as the stopped backup has finished cleaning up.
    closing: bool,
    /// The busy job is an app restore, not a backup.
    restoring: bool,
    restore_msg: Option<Result<String, String>>,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let ctx = &cc.egui_ctx;

        // Windows' own Segoe UI, read from the system; egui's built-in font if it is missing.
        let mut fonts = egui::FontDefinitions::default();
        let font_dir = PathBuf::from(env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
        if let Ok(bytes) = fs::read(font_dir.join("segoeui.ttf")) {
            fonts.font_data.insert("segoe".into(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
        }
        let mut bold = fonts.families[&FontFamily::Proportional].clone();
        if let Ok(bytes) = fs::read(font_dir.join("seguisb.ttf")) {
            fonts.font_data.insert("segoe-sb".into(), Arc::new(egui::FontData::from_owned(bytes)));
            bold.insert(0, "segoe-sb".into());
        }
        fonts.families.insert(FontFamily::Name("semibold".into()), bold);
        ctx.set_fonts(fonts);

        ctx.all_styles_mut(|s| {
            s.text_styles = [
                (TextStyle::Heading, semibold(30.0)),
                (TextStyle::Body, FontId::proportional(15.0)),
                (TextStyle::Button, FontId::proportional(15.0)),
                (TextStyle::Small, FontId::proportional(13.0)),
                (TextStyle::Monospace, FontId::monospace(13.0)),
            ]
            .into();
            s.spacing.interact_size.y = 44.0;
            s.spacing.button_padding = vec2(16.0, 10.0);
            s.spacing.item_spacing = vec2(10.0, 8.0);
        });
        ctx.set_visuals_of(egui::Theme::Light, visuals(false));
        ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
        let (tx, rx) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        {
            // Looks for phones every 2 seconds, except while a backup is using the phone.
            let (tx, ctx, busy) = (tx.clone(), ctx.clone(), busy.clone());
            thread::spawn(move || loop {
                if !busy.load(Ordering::Relaxed) {
                    if tx.send(Msg::Phones(backup::detect())).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
                thread::sleep(Duration::from_secs(2));
            });
        }

        // Home folder, not Documents: Documents is often synced to OneDrive, and this stays offline.
        let default_dest = PathBuf::from(env::var("USERPROFILE").unwrap_or_else(|_| ".".into())).join("Phone backups");
        let dest = cc.storage.and_then(|s| s.get_string("dest")).map(PathBuf::from).unwrap_or(default_dest);
        // Stored as three 0/1 digits: files, apps, data. Everything is on by default.
        let bits: Vec<bool> = cc.storage.and_then(|s| s.get_string("pick")).unwrap_or_default().chars().map(|c| c != '0').collect();
        let on = |i: usize| bits.get(i).copied().unwrap_or(true);
        let pick = Pick { files: on(0), apps: on(1), data: on(2) };

        Self {
            tx,
            rx,
            busy,
            phones: None,
            adb_ok: true,
            dest,
            pick,
            status: String::new(),
            now: String::new(),
            fraction: None,
            result: None,
            pair_addr: String::new(),
            pair_code: String::new(),
            connect_addr: String::new(),
            wifi_msg: String::new(),
            confirm_close: false,
            closing: false,
            restoring: false,
            restore_msg: None,
        }
    }

    fn start(&mut self, ctx: &egui::Context) {
        let phones = self.phones.clone().unwrap_or_default();
        let (dest, pick, tx, ctx, busy) = (self.dest.clone(), self.pick, self.tx.clone(), ctx.clone(), self.busy.clone());
        busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        self.restoring = false;
        self.result = None;
        self.status = "Starting".into();
        self.now.clear();
        self.fraction = None;
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let adb: Vec<_> = phones
                .iter()
                .filter_map(|p| match p {
                    Phone::Adb { serial, model, .. } => Some((serial, model)),
                    _ => None,
                })
                .collect();
            let result = if let Err(e) = fs::create_dir_all(&dest) {
                Err(format!("Cannot use the folder {}: {e}. Choose another folder.", dest.display()))
            } else if adb.is_empty() {
                backup::mtp_backup(&dest, &report).map(|s| vec![s])
            } else {
                adb.iter().map(|(serial, model)| backup::adb_backup(serial, model, &dest, pick, &report)).collect()
            };
            busy.store(false, Ordering::Relaxed);
            report(Msg::Done(result));
        });
    }

    fn restore(&mut self, ctx: &egui::Context, serial: String, folder: PathBuf) {
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        self.restoring = true;
        self.restore_msg = None;
        self.status = "Starting".into();
        self.now.clear();
        self.fraction = None;
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let result = backup::restore_apps(&serial, &folder, &report);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Restored(result));
        });
    }

    fn run_wifi(&mut self, ctx: &egui::Context, args: Vec<String>) {
        self.wifi_msg = WORKING.into();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = tx.send(Msg::Wifi(backup::wifi(&args)));
            ctx.request_repaint();
        });
    }

    fn content(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let phones = self.phones.clone();
        let adb_models: Vec<(String, bool)> = phones
            .iter()
            .flatten()
            .filter_map(|p| match p {
                Phone::Adb { model, wifi, .. } => Some((model.clone(), *wifi)),
                _ => None,
            })
            .collect();
        let mtp_name = phones.iter().flatten().find_map(|p| match p {
            Phone::Mtp { name } => Some(name.clone()),
            _ => None,
        });

        // The phone: what is connected and what will be saved.
        let ready = !adb_models.is_empty() || mtp_name.is_some();
        match (&phones, adb_models.as_slice(), &mtp_name) {
            (None, ..) => {
                ui.heading("Looking for your phone");
                ui.add(egui::Spinner::new().size(16.0));
            }
            (Some(_), [(model, wifi)], _) => {
                ui.heading(model);
                ui.label(if *wifi { "Connected over Wi-Fi." } else { "Connected by USB cable." });
            }
            (Some(_), [_, ..], _) => {
                ui.heading(format!("{} phones connected", adb_models.len()));
                for (model, _) in &adb_models {
                    ui.label(model);
                }
                ui.label(muted("Each phone gets its own folder."));
            }
            (Some(_), [], Some(name)) => {
                ui.heading(name);
                ui.label("Connected in File transfer mode. Photos and files will be backed up.");
                if self.adb_ok {
                    ui.label(muted("To also save apps, contacts and messages, turn on USB debugging on the phone."));
                } else {
                    adb_missing(ui, t);
                }
            }
            (Some(list), [], None) if list.contains(&Phone::Unauthorized) => {
                ui.heading("Tap Allow on your phone");
                ui.label("Unlock the phone. It asks whether to allow USB debugging from this laptop. Tap Allow.");
            }
            (Some(_), [], None) => {
                ui.heading("Connect your phone");
                ui.add_space(4.0);
                // Text rows, not controls: drop the 44 px control height here.
                ui.spacing_mut().interact_size.y = 20.0;
                for (n, step) in [
                    "Plug the phone into this laptop with a USB cable.",
                    "Unlock the phone, tap the USB notification and choose File transfer.",
                    "To also save apps, contacts and messages, turn on USB debugging.",
                ]
                .iter()
                .enumerate()
                {
                    ui.horizontal_wrapped(|ui| {
                        // Fixed-width number column so every step's text starts at the same x.
                        ui.add_sized([16.0, 20.0], egui::Label::new(RichText::new(format!("{}.", n + 1)).font(semibold(15.0)).color(t.accent)));
                        ui.label(*step);
                    });
                }
                ui.label(muted(
                    "USB debugging: Settings, About phone, tap Build number 7 times, then Developer options, USB debugging.",
                ));
                if !self.adb_ok {
                    adb_missing(ui, t);
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(16.0));
                    ui.label(muted("This screen updates when a phone is found."));
                });
                ui.spacing_mut().interact_size.y = 44.0;
            }
        }

        // What to back up. File transfer mode can only reach files.
        let mtp_only = adb_models.is_empty() && mtp_name.is_some();
        ui.add_space(20.0);
        ui.label(muted("What to back up"));
        ui.add_enabled_ui(!busy, |ui| {
            // Checkbox rows, not buttons: a 44 px row height leaves large gaps between them.
            ui.spacing_mut().interact_size.y = 28.0;
            ui.checkbox(&mut self.pick.files, "Photos, videos and files");
            ui.add_enabled_ui(!mtp_only, |ui| {
                ui.checkbox(&mut self.pick.apps, "Apps");
                ui.checkbox(&mut self.pick.data, "Contacts, call history and messages");
            });
        });
        let picked = self.pick.files || (!mtp_only && (self.pick.apps || self.pick.data));

        // Where the backup goes.
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(muted("Save backup in"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add_enabled(!busy, egui::Button::new("Change")).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().set_directory(&self.dest).pick_folder() {
                        self.dest = dir;
                    }
                }
            });
        });
        ui.add(egui::Label::new(self.dest.display().to_string()).truncate());

        // The one main action.
        ui.add_space(16.0);
        let label = if busy && self.restoring {
            "Restoring apps"
        } else if busy {
            "Backing up"
        } else if self.result.is_some() {
            "Back up again"
        } else {
            "Back up now"
        };
        // Unavailable: a quiet grey button with readable text, not a faded plum one.
        let enabled = ready && !busy && picked;
        let (fill, text) = if enabled { (t.accent, t.on_accent) } else { (t.line, t.ink) };
        let button = egui::Button::new(RichText::new(label).font(semibold(17.0)).color(text)).fill(fill).corner_radius(10);
        ui.scope(|ui| {
            if !enabled {
                ui.disable();
            }
            ui.style_mut().visuals.disabled_alpha = 1.0;
            if ui.add_sized([ui.available_width(), 52.0], button).clicked() {
                self.start(ui.ctx());
            }
        });

        if busy {
            ui.add_space(12.0);
            if let Some(f) = self.fraction {
                ui.add(egui::ProgressBar::new(f).fill(t.accent).desired_height(10.0).corner_radius(5));
            }
            ui.horizontal(|ui| {
                if self.fraction.is_none() {
                    ui.add(egui::Spinner::new().size(16.0));
                }
                ui.label(&self.status);
            });
            if !self.now.is_empty() {
                ui.add(egui::Label::new(muted(&self.now)).truncate());
            }
            ui.label(muted("Keep the phone plugged in and unlocked."));
            let stopping = backup::STOP.load(Ordering::Relaxed);
            if ui.add_enabled(!stopping, egui::Button::new(if stopping { "Stopping" } else { "Stop" })).clicked() {
                backup::STOP.store(true, Ordering::Relaxed);
            }
        }

        // What happened.
        match &self.result {
            Some(Err(e)) => {
                ui.add_space(16.0);
                ui.label(RichText::new("Backup stopped").font(semibold(20.0)).color(t.error));
                ui.label(RichText::new(e).color(t.error));
                ui.label("Check the cable and that the phone is unlocked, then press Back up again.");
            }
            Some(Ok(summaries)) => {
                for s in summaries {
                    ui.add_space(16.0);
                    let title = match (s.errors.is_empty(), summaries.len() > 1) {
                        (true, false) => "Backup finished".to_string(),
                        (false, false) => "Backup finished with problems".to_string(),
                        (true, true) => format!("{}: backup finished", s.name),
                        (false, true) => format!("{}: backup finished with problems", s.name),
                    };
                    ui.label(RichText::new(title).font(semibold(20.0)));
                    let in_backup = files(s.files - s.errors.len().min(s.files));
                    if s.pick.files || s.pick.apps {
                        ui.label(if s.files == 0 {
                            "No files were found on the phone.".to_string()
                        } else if s.copied == 0 {
                            format!("Nothing new to copy. All {in_backup} were already backed up.")
                        } else {
                            format!("Copied {} that were new or changed. The backup now holds {in_backup}.", files(s.copied))
                        });
                    }
                    if !s.errors.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "{} could not be copied. Press Back up again to retry. The list is in errors.log.",
                                files(s.errors.len())
                            ))
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
                            ui.label(format!("Your phone did not allow reading {}.", refused.join(", ")));
                        }
                    } else {
                        ui.label("Apps, contacts and messages were not saved. Turn on USB debugging to include them.");
                    }
                    ui.label(muted("Not included: app logins, chats and game progress. Android keeps those locked inside each app."));
                    if ui.button("Open backup folder").clicked() {
                        let _ = Command::new("explorer").arg(&s.root).spawn();
                    }
                }
            }
            None => {}
        }

        // Restoring is rare next to backing up, so it stays folded away too.
        ui.add_space(24.0);
        egui::CollapsingHeader::new(RichText::new("Restore apps to a phone").font(semibold(15.0))).show(ui, |ui| {
            ui.label(muted(
                "Reinstalls the apps saved in a backup that are missing from the connected phone. Their data and logins are not restored.",
            ));
            let serial = phones.iter().flatten().find_map(|p| match p {
                Phone::Adb { serial, .. } => Some(serial.clone()),
                _ => None,
            });
            if serial.is_none() {
                ui.label(muted("Connect a phone with USB debugging turned on first."));
            }
            if ui.add_enabled(serial.is_some() && !busy, egui::Button::new("Choose backup and restore")).clicked() {
                if let (Some(serial), Some(folder)) = (serial, rfd::FileDialog::new().set_directory(&self.dest).pick_folder()) {
                    self.restore(ui.ctx(), serial, folder);
                }
            }
            match &self.restore_msg {
                Some(Ok(m)) => {
                    ui.label(m);
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(t.error));
                }
                None => {}
            }
        });

        // Wi-Fi is a one-time setup, so it stays folded away.
        ui.add_space(24.0);
        egui::CollapsingHeader::new(RichText::new("Connect over Wi-Fi").font(semibold(15.0))).show(ui, |ui| {
            ui.label(muted(
                "The phone and this laptop must be on the same Wi-Fi. On the phone, open Settings, then Developer options, then Wireless debugging.",
            ));
            ui.add_space(6.0);
            ui.label(RichText::new("Pair this laptop (first time only)").font(semibold(15.0)));
            ui.label(muted("Tap Pair device with pairing code. Type the address and the code it shows."));
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    ui.label(muted("IP address and port"));
                    ui.add(egui::TextEdit::singleline(&mut self.pair_addr).hint_text("192.168.1.5:37145").desired_width(190.0));
                });
                ui.vertical(|ui| {
                    ui.label(muted("Pairing code"));
                    ui.add(egui::TextEdit::singleline(&mut self.pair_code).hint_text("123456").desired_width(110.0));
                });
            });
            let working = self.wifi_msg == WORKING;
            if ui.add_enabled(!working, egui::Button::new("Pair")).clicked() {
                let addr = self.pair_addr.trim().to_string();
                let code = self.pair_code.trim().to_string();
                if valid_addr(&addr) && code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
                    self.run_wifi(ui.ctx(), vec!["pair".into(), addr, code]);
                } else {
                    self.wifi_msg = "Type the address with its port, like 192.168.1.5:37145, and the 6-digit code.".into();
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Connect").font(semibold(15.0)));
            ui.label(muted("Type the IP address and port shown at the top of the Wireless debugging screen."));
            ui.label(muted("IP address and port"));
            ui.add(egui::TextEdit::singleline(&mut self.connect_addr).hint_text("192.168.1.5:41235").desired_width(190.0));
            if ui.add_enabled(!working, egui::Button::new("Connect")).clicked() {
                let addr = self.connect_addr.trim().to_string();
                if valid_addr(&addr) {
                    self.run_wifi(ui.ctx(), vec!["connect".into(), addr]);
                } else {
                    self.wifi_msg = "Type the address with its port, like 192.168.1.5:41235.".into();
                }
            }
            if !self.wifi_msg.is_empty() {
                let lower = self.wifi_msg.to_lowercase();
                let failed = ["fail", "error", "cannot", "unable", "type the"].iter().any(|w| lower.contains(w));
                ui.label(RichText::new(&self.wifi_msg).color(if failed { t.error } else { t.ink }));
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Phones(p) => {
                    self.phones = Some(p);
                    self.adb_ok = backup::adb_installed();
                }
                Msg::Status(s) => {
                    self.status = s;
                    self.now.clear();
                    self.fraction = None;
                }
                Msg::Progress { done, total, now } => {
                    self.now = now;
                    if total > 0 {
                        self.status = format!("Copying {} of {}", count(done), files(total));
                        self.fraction = Some(done as f32 / total as f32);
                    } else {
                        self.status = format!("{} copied so far", files(done));
                        self.fraction = None;
                    }
                }
                Msg::Wifi(s) => self.wifi_msg = s,
                Msg::Restored(r) => self.restore_msg = Some(r),
                Msg::Done(r) => {
                    self.result = Some(r);
                    // Flash the taskbar button if the window is in the background.
                    if !ui.ctx().input(|i| i.focused) {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                    }
                }
            }
        }

        // Closing mid-backup asks first, then stops adb cleanly instead of leaving it running.
        let busy = self.busy.load(Ordering::Relaxed);
        if ui.ctx().input(|i| i.viewport().close_requested()) && busy {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.closing && !busy {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.confirm_close {
            egui::Modal::new(egui::Id::unique("confirm_close")).show(ui.ctx(), |ui| {
                ui.set_max_width(320.0);
                if self.restoring {
                    ui.label(RichText::new("Stop restoring apps?").font(semibold(18.0)));
                    ui.label("Apps installed so far stay on the phone.");
                } else {
                    ui.label(RichText::new("Stop the backup?").font(semibold(18.0)));
                    ui.label("Files copied so far are kept. The next backup continues where this one stopped.");
                }
                ui.horizontal(|ui| {
                    if ui.button("Stop and close").clicked() {
                        backup::STOP.store(true, Ordering::Relaxed);
                        self.closing = true;
                        self.confirm_close = false;
                    }
                    if ui.button("Keep backing up").clicked() {
                        self.confirm_close = false;
                    }
                });
            });
        }
        let t = tokens(ui.visuals().dark_mode);
        egui::Frame::central_panel(ui.style()).fill(t.bg).inner_margin(28).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                // One 560 px column, centred when the window is wider.
                let side = ((ui.available_width() - 560.0) / 2.0).max(0.0);
                let width = ui.available_width().min(560.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_max_width(width);
                        self.content(ui, &t);
                    });
                });
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("dest", self.dest.display().to_string());
        let bit = |b: bool| if b { '1' } else { '0' };
        storage.set_string("pick", [bit(self.pick.files), bit(self.pick.apps), bit(self.pick.data)].iter().collect());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn counts_with_commas() {
        assert_eq!(super::count(0), "0");
        assert_eq!(super::count(999), "999");
        assert_eq!(super::count(1280), "1,280");
        assert_eq!(super::count(1234567), "1,234,567");
        assert_eq!(super::files(1), "1 file");
        assert_eq!(super::files(1280), "1,280 files");
    }

    #[test]
    fn checks_wifi_addresses() {
        assert!(super::valid_addr("192.168.1.5:37145"));
        assert!(super::valid_addr("phone.local:5555"));
        assert!(!super::valid_addr("192.168.1.5:"));
        assert!(!super::valid_addr(":5555"));
        assert!(!super::valid_addr("192.168.1.5"));
        assert!(!super::valid_addr("192.168.1.5:99999"));
        assert!(!super::valid_addr("192.168.1.5:0"));
    }
}

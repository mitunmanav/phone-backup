// No console window behind the app on Windows.
#![windows_subsystem = "windows"]

mod backup;
mod export;
mod fixture;
mod jobs;
mod tools;
mod ui;

use backup::{FLAGS, KINDS, Msg, Phone, Pick, Summary, Tally, count, files, or_list, size};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, Rect, RichText, Stroke, StrokeKind,
    TextStyle, vec2,
};
use fixture::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use std::{env, fs, thread};
use ui::*;

/// What the bar at the foot of the Back up and Restore pages says and offers.
struct Cmd {
    title: String,
    note: String,
    label: &'static str,
    enabled: bool,
    /// Restore only: what the main button runs, and whether it only checks.
    plan: Option<(Plan, bool)>,
}

/// Debug builds opened on PB_FIXTURE test data: they must not save their fake folder, zoom or
/// window size over the real settings.
fn test_run() -> bool {
    cfg!(debug_assertions) && env::var_os("PB_FIXTURE").is_some()
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Phone Backup")
            .with_inner_size([960.0, 700.0])
            .with_min_inner_size([360.0, 480.0]),
        persist_window: !test_run(),
        ..Default::default()
    };
    eframe::run_native(
        "Phone Backup",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// The files in a backup: path, size and time, from `backup::backup_files`.
type Listed = Result<Vec<(PathBuf, u64, i64)>, String>;
/// What `backup::measure` found: every file and its size, and the size of the apps.
type Measured = (Vec<(String, u64)>, u64);

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Backup,
    Restore,
    Transfer,
    Tools,
    Settings,
    /// Pages opened from Settings.
    Privacy,
    License,
}

/// What the running (or last) job is.
#[derive(Clone, Copy, PartialEq)]
enum Job {
    Backup,
    /// Restores, checks and phone-to-phone transfers: they all end in a restore.
    Restore,
    /// Copying files from one backup into another.
    Copy,
    /// Moving a backup to the Recycle Bin; it cannot be stopped.
    Delete,
    /// Re-hashing a backup against its checksums.
    Verify,
    /// Exporting a backup to files other apps open.
    Export,
}

/// A dialog about the chosen backup.
enum Ask {
    Delete(PathBuf),
    /// The folder, the name being typed, and why the last try was refused.
    Rename(PathBuf, String, Option<String>),
    /// The backup to copy from, and the one chosen to copy into.
    Copy(PathBuf, Option<PathBuf>),
}

struct App {
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    busy: Arc<AtomicBool>,
    /// None until the first check has finished.
    phones: Option<Vec<Phone>>,
    adb_ok: bool,
    /// The user ticked Google's terms; the download stays locked until then.
    adb_agreed: bool,
    adb_working: bool,
    /// Why the last download of adb did not work.
    adb_err: Option<String>,
    dest: PathBuf,
    pick: Pick,
    /// What `backup::measure` found on a phone, by serial; None while it runs.
    sizes: Option<(String, Option<Measured>)>,
    /// The sizes for the current choices, worked out again only when they change.
    tally: Option<(Pick, Tally)>,
    /// The "Choose files to back up" dialog is open.
    choosing: bool,
    status: String,
    now: String,
    fraction: Option<f32>,
    /// When the progress bar last appeared, for the time left.
    progress_from: Instant,
    result: Option<Result<Vec<Summary>, String>>,
    pair_addr: String,
    pair_code: String,
    connect_addr: String,
    wifi_msg: Option<Result<String, String>>,
    wifi_working: bool,
    wifi_open: bool,
    /// The QR code on screen while waiting for the phone to scan it.
    qr: Option<qrcodegen::QrCode>,
    /// Which phone maker's USB debugging steps are shown.
    brand: usize,
    /// Height of the command bar last frame; its text wraps on narrow windows.
    bar_h: f32,
    /// Asking whether to stop a running backup before closing.
    confirm_close: bool,
    /// Close as soon as the stopped backup has finished cleaning up.
    closing: bool,
    tab: Tab,
    /// When the page (or intro step) last changed, for its fade and the tab pill's slide.
    tab_at: Instant,
    /// Where the tab pill was drawn last frame, and where its current slide started.
    pill: (f32, f32),
    /// Windows' "Animation effects" setting.
    motion: bool,
    /// The first-run step on screen; None once it is done or skipped.
    intro: Option<usize>,
    /// Play the Windows sound when a backup or restore ends.
    sound: bool,
    /// The report of the last restore, for its Open report button.
    restore_report: Option<PathBuf>,
    job: Job,
    /// The backup last checked and what the check found.
    verified: Option<(PathBuf, Result<String, String>)>,
    restore_msg: Option<Result<String, String>>,
    /// What the last export made, and its folder for the Open folder button.
    export_msg: Option<(PathBuf, Result<String, String>)>,
    /// The running restore is only a check; holds what it checks.
    checking: Option<Plan>,
    /// The last check and what it found. Restore now is offered only while the choices still match.
    preview: Option<(Plan, Result<String, String>)>,
    backups: Vec<backup::Saved>,
    /// The folder `backups` was read from.
    scanned: PathBuf,
    chosen: Option<PathBuf>,
    restore_files: bool,
    restore_apps: bool,
    /// Files chosen in the picker for one backup folder: (folder, chosen, files it holds).
    only: Option<(PathBuf, Arc<HashSet<PathBuf>>, usize)>,
    picker: Option<Picker>,
    /// Phone to phone: the serials of the phone to copy from and the one to copy to.
    from: Option<String>,
    to: Option<String>,
    /// Back up the phone copied to before anything changes on it.
    safety: bool,
    /// The source phone's serial, the backup folder its last transfer check made and what that
    /// check backed up (the folder holds older backups too).
    source: Option<(String, PathBuf, Arc<HashSet<PathBuf>>)>,
    ask: Option<Ask>,
    /// What the last copy between backups or delete said.
    tidy_msg: Option<Result<String, String>>,
    tool_tx: mpsc::Sender<tools::Done>,
    tool_rx: mpsc::Receiver<tools::Done>,
    /// The Tools panel whose job is running, and what it says.
    tool_busy: Option<&'static str>,
    tool_status: String,
    /// What the last tool said, and in which panel.
    tool_msg: Option<(&'static str, Result<String, String>)>,
    /// Groups of identical files, and the rows of the list showing them.
    dupes: Option<(tools::Groups, Rows)>,
    /// Copies ticked for deleting; the first of each group starts unticked.
    ticked: HashSet<PathBuf>,
    /// Files chosen to delete from the phone: (backup folder, files).
    free_pick: Option<(PathBuf, HashSet<PathBuf>)>,
    /// A delete waiting for the user's yes.
    confirm: Option<Confirm>,
    scrcpy: Option<PathBuf>,
    /// Debug builds only: which test data is shown instead of the real phones and backups.
    fixture: Fixture,
    /// Debug builds only: scroll position to open at, for screenshots.
    scroll: Option<f32>,
    /// Debug builds only: save a picture of the window here, then close.
    shot: Option<PathBuf>,
}

/// A restore's choices: phone serial, backup folder, files, apps, and the chosen files (None: all).
type Plan = (String, PathBuf, bool, bool, Option<Arc<HashSet<PathBuf>>>);

/// Rows of the duplicate list: (group, file in it, or None for the group's heading).
type Rows = Vec<(usize, Option<usize>)>;

/// A delete that asks first.
enum Confirm {
    /// Copies in the backup on this computer.
    Copies(Vec<(PathBuf, u64)>),
    /// Files on the phone (serial) whose backup copy matches, and how many chosen ones do not.
    Phone(String, Vec<(String, u64)>, usize),
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let ctx = &cc.egui_ctx;

        // Windows' own Segoe UI, read from the system; egui's built-in font if it is missing.
        let mut fonts = egui::FontDefinitions::default();
        let font_dir = PathBuf::from(env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("Fonts");
        if let Ok(bytes) = fs::read(font_dir.join("segoeui.ttf")) {
            fonts
                .font_data
                .insert("segoe".into(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "segoe".into());
        }
        let mut bold = fonts.families[&FontFamily::Proportional].clone();
        if let Ok(bytes) = fs::read(font_dir.join("seguisb.ttf")) {
            fonts.font_data.insert(
                "segoe-sb".into(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            bold.insert(0, "segoe-sb".into());
        }
        fonts
            .families
            .insert(FontFamily::Name("semibold".into()), bold);
        ctx.set_fonts(fonts);

        ctx.all_styles_mut(|s| {
            s.text_styles = [
                // Windows 11 ramp: subtitle 20, body 14, caption 12.
                (TextStyle::Heading, semibold(16.0)),
                (TextStyle::Body, FontId::proportional(14.0)),
                (TextStyle::Button, FontId::proportional(14.0)),
                (TextStyle::Small, FontId::proportional(12.0)),
                (TextStyle::Monospace, FontId::monospace(13.0)),
            ]
            .into();
            // Desktop sizes: 32 px controls, not 44 px touch targets.
            s.spacing.interact_size.y = 32.0;
            s.spacing.button_padding = vec2(12.0, 5.0);
            s.spacing.item_spacing = vec2(8.0, 8.0);
            s.spacing.icon_width = 18.0;
            s.spacing.icon_width_inner = 10.0;
            s.spacing.icon_spacing = 8.0;
            // The scroll bar gets its own strip; floating, it covered the right edge of full-width buttons.
            // Its handle takes the text colour: by default it is the field colour, the same as its
            // track, so nothing showed that a dialog or list scrolls.
            s.spacing.scroll = egui::style::ScrollStyle {
                foreground_color: true,
                ..egui::style::ScrollStyle::solid()
            };
        });
        ctx.set_visuals_of(egui::Theme::Light, visuals(false));
        ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
        let (tx, rx) = mpsc::channel();
        let (tool_tx, tool_rx) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        {
            // Looks for phones every 2 seconds, except while a backup is using the phone.
            let (tx, ctx, busy) = (tx.clone(), ctx.clone(), busy.clone());
            thread::spawn(move || {
                loop {
                    if !busy.load(Ordering::Relaxed) {
                        if tx.send(Msg::Phones(backup::detect())).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    thread::sleep(Duration::from_secs(2));
                }
            });
        }

        // Home folder, not Documents: Documents is often synced to OneDrive, and this stays offline.
        let default_dest = PathBuf::from(env::var("USERPROFILE").unwrap_or_else(|_| ".".into()))
            .join("Phone backups");
        let dest = cc
            .storage
            .and_then(|s| s.get_string("dest"))
            .map(PathBuf::from)
            .unwrap_or(default_dest);
        // Stored as three 0/1 digits: files, apps, data. Everything is on by default.
        let bits: Vec<bool> = cc
            .storage
            .and_then(|s| s.get_string("pick"))
            .unwrap_or_default()
            .chars()
            .map(|c| c != '0')
            .collect();
        let on = |i: usize| bits.get(i).copied().unwrap_or(true);
        // Then the five types; folders one per line (none: all); the size limit in bytes.
        let get = |key: &str| {
            cc.storage
                .and_then(|s| s.get_string(key))
                .unwrap_or_default()
        };
        let pick = Pick {
            files: on(0),
            apps: on(1),
            data: on(2),
            folders: get("folders").lines().map(str::to_string).collect(),
            kinds: std::array::from_fn(|k| on(3 + k)),
            max_size: get("max_size").parse().ok(),
        };

        let mut app = Self {
            tx,
            rx,
            busy,
            phones: None,
            adb_ok: true,
            adb_agreed: false,
            adb_working: false,
            adb_err: None,
            dest,
            pick,
            sizes: None,
            tally: None,
            choosing: false,
            status: String::new(),
            now: String::new(),
            fraction: None,
            progress_from: Instant::now(),
            result: None,
            pair_addr: cc
                .storage
                .and_then(|s| s.get_string("pair_addr"))
                .unwrap_or_default(),
            pair_code: String::new(),
            connect_addr: cc
                .storage
                .and_then(|s| s.get_string("connect_addr"))
                .unwrap_or_default(),
            wifi_msg: None,
            wifi_working: false,
            wifi_open: false,
            qr: None,
            brand: 0,
            bar_h: 64.0,
            confirm_close: false,
            closing: false,
            tab: Tab::Backup,
            tab_at: Instant::now(),
            pill: (0.0, 0.0),
            motion: motion_on(),
            // A first run has no saved backup folder yet.
            intro: cc
                .storage
                .is_some_and(|s| s.get_string("dest").is_none())
                .then_some(0),
            sound: cc.storage.and_then(|s| s.get_string("sound")).as_deref() == Some("1"),
            restore_report: None,
            job: Job::Backup,
            verified: None,
            restore_msg: None,
            export_msg: None,
            checking: None,
            preview: None,
            backups: Vec::new(),
            scanned: PathBuf::new(),
            chosen: None,
            restore_files: true,
            restore_apps: true,
            only: None,
            picker: None,
            from: None,
            to: None,
            safety: true,
            source: None,
            ask: None,
            tidy_msg: None,
            tool_tx,
            tool_rx,
            tool_busy: None,
            tool_status: String::new(),
            tool_msg: None,
            dupes: None,
            ticked: HashSet::new(),
            free_pick: None,
            confirm: None,
            scrcpy: None,
            fixture: Fixture::Real,
            scroll: None,
            shot: None,
        };
        // PB_FIXTURE="worst|empty|one|many [restore] [transfer] [tools] [settings] [privacy] [license] [intro[=<step>]]
        // [delete|rename|copy] [light|dark] [wifi] [busy] [choose] [noadb[=working|failed]] [zoom=<factor>] [scroll=<px>] [pick=<backup folder>]
        // [size=<w>x<h>] [shot=<file.png>]" opens
        // straight on test data; delete, rename and copy open that dialog for the first backup; wifi opens its dialog; choose opens "Choose files to back up"; busy shows a job running; noadb shows adb missing; pick= opens the file picker on a real folder; intro opens the first-run steps; shot= saves a
        // picture of the window and closes.
        if cfg!(debug_assertions) && test_run() {
            app.intro = None;
            let spec = env::var("PB_FIXTURE").unwrap_or_default();
            for word in spec.split_whitespace() {
                match word {
                    "privacy" => app.tab = Tab::Privacy,
                    "license" => app.tab = Tab::License,
                    _ if word.starts_with("intro") => {
                        app.intro = Some(word.get(6..).and_then(|n| n.parse().ok()).unwrap_or(0))
                    }
                    "worst" => app.load_fixture(Fixture::Worst),
                    "empty" => app.load_fixture(Fixture::Empty),
                    "one" => app.load_fixture(Fixture::One),
                    "many" => app.load_fixture(Fixture::Many),
                    "restore" => app.tab = Tab::Restore,
                    "transfer" => app.tab = Tab::Transfer,
                    "delete" | "rename" | "copy" => {
                        app.tab = Tab::Restore;
                        let b = app
                            .backups
                            .first()
                            .map(|b| b.path.clone())
                            .unwrap_or_default();
                        let into = app.backups.get(1).map(|b| b.path.clone());
                        app.ask = Some(match word {
                            "delete" => Ask::Delete(b),
                            "rename" => Ask::Rename(
                                b,
                                "Mum's old phone: before the reset".into(),
                                Some("Windows does not allow that name.".into()),
                            ),
                            _ => Ask::Copy(b, into),
                        });
                    }
                    "tools" => app.tab = Tab::Tools,
                    "settings" => app.tab = Tab::Settings,
                    "light" => ctx.set_theme(egui::Theme::Light),
                    "dark" => ctx.set_theme(egui::Theme::Dark),
                    "wifi" => {
                        app.wifi_open = true;
                        app.qr = qrcodegen::QrCode::encode_text(
                            &backup::qr_code().2,
                            qrcodegen::QrCodeEcc::Medium,
                        )
                        .ok();
                    }
                    "choose" => app.choosing = true,
                    _ if word.starts_with("noadb") => {
                        app.adb_ok = false;
                        app.adb_working = word == "noadb=working";
                        if word == "noadb=failed" {
                            app.adb_err = Some(r"Cannot save to C:\Users\Bartholomew Fitzgerald\AppData\Local\Programs\Phone Backup: Access is denied. (os error 5). Move this app to a folder you can change, such as your Desktop, then try again.".into());
                        }
                    }
                    "busy" => {
                        app.busy.store(true, Ordering::Relaxed);
                        app.status = "Copying 48,213 of 128,456 files…".into();
                        app.now = r"/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4".into();
                        app.fraction = Some(0.375);
                        app.progress_from = Instant::now() - Duration::from_secs(431);
                    }
                    _ if word.starts_with("zoom=") => {
                        ctx.set_zoom_factor(word[5..].parse().unwrap_or(1.0))
                    }
                    _ if word.starts_with("pick=") => app.open_picker(ctx, word[5..].into(), false),
                    _ if word.starts_with("shot=") => app.shot = Some(word[5..].into()),
                    _ if word.starts_with("size=") => {
                        if let Some((w, h)) = word[5..].split_once('x')
                            && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(w, h)));
                        }
                    }
                    _ => app.scroll = word.strip_prefix("scroll=").and_then(|n| n.parse().ok()),
                }
            }
        }
        app
    }
}

impl App {
    /// A backup or restore ended: the sound if chosen, and a taskbar flash if the window is
    /// in the background.
    fn finished(&self, ctx: &egui::Context) {
        if self.sound {
            backup::beep();
        }
        if !ctx.input(|i| i.focused) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // About a second first, so measured sizes and fading scroll bars have settled.
        if let Some(path) = &self.shot
            && ui.ctx().cumulative_frame_nr() == 60
        {
            let (path, ctx) = (path.clone(), ui.ctx().clone());
            ui.ctx().request_screenshot(move |img| {
                let [w, h] = img.size;
                let _ = image::save_buffer(
                    &path,
                    img.as_raw(),
                    w as u32,
                    h as u32,
                    image::ExtendedColorType::Rgba8,
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            });
        }
        if self.shot.is_some() {
            ui.ctx().request_repaint();
        }
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Phones(_) if self.fixture != Fixture::Real => {}
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
                        let verb = if self.job == Job::Verify {
                            "Checking"
                        } else {
                            "Copying"
                        };
                        self.status = format!("{verb} {} of {}…", count(done), files(total));
                        if self.fraction.is_none() {
                            self.progress_from = Instant::now();
                        }
                        self.fraction = Some(done as f32 / total as f32);
                    } else {
                        self.status = format!("{} copied so far", files(done));
                        self.fraction = None;
                    }
                }
                Msg::Wifi(r) => {
                    self.wifi_working = false;
                    self.qr = None;
                    self.wifi_msg = Some(r);
                }
                Msg::GotAdb(r) => {
                    self.adb_working = false;
                    self.adb_ok = backup::adb_installed();
                    self.adb_err = r.err();
                }
                Msg::Transferred(folder, r) => {
                    if let Some((from, folder, ran)) = folder {
                        let ran = Arc::new(ran);
                        if let Some(plan) = &mut self.checking {
                            plan.1 = folder.clone();
                            plan.4 = Some(ran.clone());
                        }
                        self.source = Some((from, folder, ran));
                    }
                    if let Some(plan) = self.checking.take() {
                        self.preview = Some((plan, r));
                    }
                }
                Msg::Tidied(r) => {
                    self.tidy_msg = Some(r);
                    self.finished(ui.ctx());
                    let dir = self.scanned.clone();
                    self.scan(&dir);
                }
                Msg::FolderSizes(sizes) => {
                    for (path, n) in sizes {
                        if let Some(b) = self.backups.iter_mut().find(|b| b.path == path) {
                            b.bytes = Some(n);
                        }
                    }
                }
                Msg::Restored(r) => match self.checking.take() {
                    Some(plan) => self.preview = Some((plan, r)),
                    None => {
                        self.restore_msg = Some(r);
                        self.finished(ui.ctx());
                    }
                },
                Msg::Sizes(serial, list, apps) => {
                    self.sizes = Some((serial, Some((list, apps))));
                    self.tally = None;
                }
                Msg::Exported(folder, r) => {
                    self.export_msg = Some((folder, r));
                    self.finished(ui.ctx());
                }
                Msg::Done(r) => {
                    self.result = Some(r);
                    self.finished(ui.ctx());
                }
                Msg::Verified(folder, r) => {
                    self.verified = Some((folder, r));
                    self.finished(ui.ctx());
                }
            }
        }

        while let Ok(done) = self.tool_rx.try_recv() {
            let which = if matches!(done, tools::Done::Status(_)) {
                self.tool_busy
            } else {
                self.tool_busy.take()
            }
            .unwrap_or("");
            match done {
                tools::Done::Status(s) => self.tool_status = s,
                tools::Done::Dupes(Ok(groups)) => self.show_dupes(groups),
                tools::Done::Dupes(Err(e)) | tools::Done::Matched(Err(e)) => {
                    self.tool_msg = Some((which, Err(e)))
                }
                tools::Done::Matched(Ok((list, left))) if list.is_empty() => {
                    self.tool_msg = Some((
                        which,
                        Err(format!(
                            "None of the {} chosen match their backup copy on the phone, so nothing was deleted.",
                            files(left)
                        )),
                    ))
                }
                tools::Done::Matched(Ok((list, left))) => {
                    let serial = self.adb_serial().unwrap_or_default();
                    self.confirm = Some(Confirm::Phone(serial, list, left));
                }
                tools::Done::Text(r) => self.tool_msg = Some((which, r)),
            }
        }

        // Closing mid-backup asks first, then stops adb cleanly instead of leaving it running.
        let busy = self.busy.load(Ordering::Relaxed);
        if ui.ctx().input(|i| i.viewport().close_requested()) && busy {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.closing && !busy {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.confirm_close {
            let modal = egui::Modal::new(egui::Id::unique("confirm_close")).show(ui.ctx(), |ui| {
                ui.set_max_width(ui.available_width().min(320.0));
                let (title, body) = match self.job {
                    Job::Restore => ("Stop restoring?", "Everything restored so far stays on the phone."),
                    Job::Backup => ("Stop the backup?", "Files copied so far are kept. The next backup continues where this one stopped."),
                    Job::Copy => ("Stop copying?", "Files copied so far stay in the backup."),
                    Job::Verify => ("Stop checking the backup?", "The backup is not changed. You can check it again later."),
                    Job::Export => ("Stop exporting?", "Files exported so far are kept. The backup itself is not changed."),
                    Job::Delete => ("Close when the delete ends?", "Windows is moving the backup to the Recycle Bin. The window closes when it has finished."),
                };
                ui.label(RichText::new(title).font(semibold(16.0)));
                ui.label(body);
                ui.horizontal(|ui| {
                    let (go, stay) = match self.job {
                        Job::Restore => ("Stop and close", "Keep restoring"),
                        Job::Backup => ("Stop and close", "Keep backing up"),
                        Job::Copy => ("Stop and close", "Keep copying"),
                        Job::Delete => ("Close when done", "Keep open"),
                        Job::Verify => ("Stop and close", "Keep checking"),
                        Job::Export => ("Stop and close", "Keep exporting"),
                    };
                    if ui.button(go).clicked() {
                        backup::STOP.store(true, Ordering::Relaxed);
                        self.closing = true;
                        self.confirm_close = false;
                    }
                    if ui.button(stay).clicked() {
                        self.confirm_close = false;
                    }
                });
            });
            // Esc or a click outside means keep going, the safe answer.
            if modal.should_close() {
                self.confirm_close = false;
            }
        }
        let t = tokens(ui.visuals().dark_mode);
        if self.wifi_open {
            self.wifi_dialog(ui.ctx(), &t);
        }
        self.picker_dialog(ui.ctx(), &t);
        if self.choosing {
            self.choose_dialog(ui.ctx(), &t);
        }
        self.ask_dialog(ui.ctx(), &t);
        self.confirm_dialog(ui.ctx());

        // Esc leaves Privacy and License for Settings, where they open from.
        if matches!(self.tab, Tab::Privacy | Tab::License)
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.go(Tab::Settings);
        }
        let pad: i8 = if ui.available_width() >= 640.0 {
            28
        } else {
            16
        };
        egui::Panel::top("bar")
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(t.bg).inner_margin(egui::Margin {
                left: pad,
                right: 8,
                top: 6,
                bottom: 0,
            }))
            .show(ui, |ui| self.top_bar(ui, &t));
        egui::Frame::central_panel(ui.style())
            .fill(t.bg)
            .inner_margin(0)
            .show(ui, |ui| {
                // A lighter page under the tabs; its side and bottom edges sit just outside the
                // window.
                egui::Frame::new()
                    .fill(t.layer)
                    .stroke(Stroke::new(1.0, t.line))
                    .outer_margin(egui::Margin {
                        left: -1,
                        top: 0,
                        right: -1,
                        bottom: -1,
                    })
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        self.page(ui, &t, pad);
                    });
            });
    }

    fn persist_egui_memory(&self) -> bool {
        !test_run()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if test_run() {
            return;
        }
        storage.set_string("dest", self.dest.display().to_string());
        let bit = |b: bool| if b { '1' } else { '0' };
        storage.set_string("pair_addr", self.pair_addr.clone());
        storage.set_string("connect_addr", self.connect_addr.clone());
        storage.set_string("sound", bit(self.sound).to_string());
        storage.set_string(
            "pick",
            [self.pick.files, self.pick.apps, self.pick.data]
                .iter()
                .chain(&self.pick.kinds)
                .map(|&b| bit(b))
                .collect(),
        );
        let folders: Vec<_> = self.pick.folders.iter().cloned().collect();
        storage.set_string(
            "folders",
            folders.join(
                "
",
            ),
        );
        storage.set_string(
            "max_size",
            self.pick
                .max_size
                .map(|m| m.to_string())
                .unwrap_or_default(),
        );
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
    fn words_for_counts_and_ages() {
        assert_eq!(super::backup::were(1), "was");
        assert_eq!(super::backup::were(2), "were");
        assert_eq!(super::or_list(&["call history".into()]), "call history");
        assert_eq!(
            super::or_list(&["call history".into(), "messages".into()]),
            "call history or messages"
        );
        assert_eq!(
            super::or_list(&["a".into(), "b".into(), "c".into()]),
            "a, b or c"
        );
        assert_eq!(super::days_ago(Some(1)), "yesterday");
        assert_eq!(super::days_ago(Some(44)), "44 days ago");
        assert_eq!(super::days_ago(Some(45)), "2 months ago");
        assert_eq!(super::days_ago(Some(364)), "12 months ago");
        assert_eq!(super::days_ago(Some(365)), "1 year ago");
        assert_eq!(super::days_ago(Some(1284)), "3 years ago");
    }

    #[test]
    fn says_time_left() {
        assert_eq!(super::time_left(30), "less than a minute left");
        assert_eq!(super::time_left(690), "about 12 min left");
        assert_eq!(super::time_left(3540), "about 59 min left");
        assert_eq!(super::time_left(7800), "about 2 h 10 min left");
    }

    #[test]
    fn previews_pictures_only() {
        let dir = std::env::temp_dir().join(format!("pb-thumb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pic, text) = (dir.join("wide.png"), dir.join("note.jpg"));
        image::RgbImage::new(120, 60).save(&pic).unwrap();
        // A wrong extension does not fool it: the contents decide.
        std::fs::write(&text, "not a picture").unwrap();
        let thumb = super::thumbnail(&pic).unwrap();
        let none = super::thumbnail(&text);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(thumb.size, [80, 80]);
        assert!(none.is_none());
    }
}

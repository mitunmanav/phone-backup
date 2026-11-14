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
    /// A finished job: the main button opens this instead of running anything.
    open: Option<PathBuf>,
    /// A finished or stopped job: offer Start again, so it is not the end of the road.
    again: bool,
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
            .with_min_inner_size([360.0, 540.0])
            .with_icon({
                let (rgba, n) = logo_pixels(true);
                egui::IconData {
                    rgba,
                    width: n as u32,
                    height: n as u32,
                }
            }),
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
    /// A backup's messages, contacts, calls and calendar, opened from Tools.
    Read,
    /// Pages opened from Settings.
    Privacy,
    License,
    /// How to turn on USB debugging, opened from Back up or Settings.
    Help,
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
/// How many older versions a backup keeps: words, at most this many runs, runs newer than
/// this many months. Pruning runs after a backup with no errors and never touches the latest.
const KEEP: [(&str, Option<usize>, Option<u32>); 7] = [
    ("Keep all", None, None),
    ("The last 5", Some(5), None),
    ("The last 10", Some(10), None),
    ("The last 30", Some(30), None),
    ("From the last 3 months", None, Some(3)),
    ("From the last 6 months", None, Some(6)),
    ("From the last year", None, Some(12)),
];

enum Ask {
    /// The backup, and how many older versions go with it.
    Delete(PathBuf, usize),
    /// A backup's older versions, newest first, and the one ticked (None: the latest).
    History(PathBuf, Vec<backup::Run>, Option<String>),
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
    update_working: bool,
    /// What the last click on Check for updates found.
    update: Option<Result<Option<String>, String>>,
    /// The phone report is being read and saved.
    report_working: bool,
    report_err: Option<String>,
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
    /// How far the last job had got when it ended.
    stopped_at: Option<f32>,
    /// The running or last backup's bytes copied and to copy, per tile (`Msg::Tiles`).
    tiles: Option<([u64; 6], [u64; 6])>,
    /// The folder last asked about and the room left for it; None after a backup changes that.
    free: Option<(PathBuf, Option<u64>)>,
    /// When the progress bar last appeared, for the time left.
    progress_from: Instant,
    /// When Pause was pressed, while the job is paused.
    paused_at: Option<Instant>,
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
    /// The page the help page goes back to.
    help_from: Tab,
    /// Height of the command bar last frame; its text wraps on narrow windows.
    bar_h: f32,
    /// Asking whether to stop a running backup before closing.
    confirm_close: bool,
    /// The question came from the Stop button: stopping leaves the window open.
    stop_only: bool,
    /// Close as soon as the stopped backup has finished cleaning up.
    closing: bool,
    tab: Tab,
    /// When the page (or intro step) last changed, for its fade.
    tab_at: Instant,
    /// The app's mark in the side rail, made the first time it is drawn.
    logo: Option<egui::TextureHandle>,
    /// Windows' "Animation effects" setting.
    motion: bool,
    /// The first-run step on screen; None once it is done or skipped.
    intro: Option<usize>,
    /// Play a sound when a backup or restore ends.
    sound: bool,
    /// Which of `backup::SOUNDS` to play.
    sound_pick: usize,
    /// The report of the last restore, for its Open report button.
    restore_report: Option<PathBuf>,
    job: Job,
    /// The backup last checked and what the check found.
    verified: Option<(PathBuf, Result<String, String>)>,
    /// The last restore or transfer: what it did and how many files and apps failed, or why it stopped.
    restore_msg: Option<Result<(String, usize), String>>,
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
    /// Which of `KEEP` says how many older versions each backup keeps.
    keep: usize,
    /// The older version chosen for one backup folder: (folder, run name). None: the latest.
    version: Option<(PathBuf, String)>,
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
    /// Which tool the Tools page shows.
    tool: usize,
    tool_busy: Option<&'static str>,
    /// Stop was pressed on the running tool: ask first.
    stop_tool: bool,
    /// The zoom this app last set for the window's size; None once the user zoomed by hand.
    auto_zoom: Option<f32>,
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
    /// The backup open on the Read page.
    reading: Option<Reading>,
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
    Phone(String, Vec<tools::OnPhone>, usize),
}

impl App {
    /// Bigger windows get bigger everything, not just more empty space, up to 120%. Ctrl+plus
    /// or minus turns this off for good: the user's own zoom always wins.
    fn auto_zoom(&mut self, ctx: &egui::Context) {
        let Some(set) = self.auto_zoom else { return };
        let z = ctx.zoom_factor();
        if (z - set).abs() > 0.001 {
            self.auto_zoom = None;
            return;
        }
        let r = ctx.content_rect();
        let want = (r.width() * z / 1280.0)
            .min(r.height() * z / 800.0)
            .clamp(1.0, 1.2);
        // Only a clear change counts, and in 5% steps: a slow drag must not redraw the fonts
        // each frame, and a window on the line between two steps must not flip.
        if (want - z).abs() > 0.06 || (want == 1.0 && z != 1.0) {
            let want = (want * 20.0).round() / 20.0;
            ctx.set_zoom_factor(want);
            self.auto_zoom = Some(want);
        }
    }

    fn new(cc: &eframe::CreationContext) -> Self {
        let ctx = &cc.egui_ctx;

        // Windows 11's Segoe UI Variable, read from the system. Windows 10 only has the older
        // Segoe UI files; egui's built-in font if those are missing too.
        let mut fonts = egui::FontDefinitions::default();
        let font_dir = PathBuf::from(env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("Fonts");
        let read = |file: &str| fs::read(font_dir.join(file)).ok();
        let weight = |bytes: Vec<u8>, wght: f32| {
            egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
                coords: egui::epaint::text::VariationCoords::new([
                    (b"wght", wght),
                    (b"opsz", 14.0),
                ]),
                ..Default::default()
            })
        };
        let (regular, semibold_face) = match read("SegUIVar.ttf") {
            Some(var) => (Some(weight(var.clone(), 400.0)), Some(weight(var, 600.0))),
            None => (
                read("segoeui.ttf").map(egui::FontData::from_owned),
                read("seguisb.ttf").map(egui::FontData::from_owned),
            ),
        };
        if let Some(face) = regular {
            fonts.font_data.insert("segoe".into(), Arc::new(face));
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "segoe".into());
        }
        let mut bold = fonts.families[&FontFamily::Proportional].clone();
        if let Some(face) = semibold_face {
            fonts.font_data.insert("segoe-sb".into(), Arc::new(face));
            bold.insert(0, "segoe-sb".into());
        }
        fonts
            .families
            .insert(FontFamily::Name("semibold".into()), bold);
        // Windows 11's icon font, or Windows 10's. A family must exist or egui panics, so without
        // either the icons fall back to text.
        let mut icons = fonts.families[&FontFamily::Proportional].clone();
        if let Some(face) = read("SegoeIcons.ttf").or_else(|| read("SegMDL2.ttf")) {
            fonts
                .font_data
                .insert("icons".into(), Arc::new(egui::FontData::from_owned(face)));
            icons.insert(0, "icons".into());
        }
        fonts
            .families
            .insert(FontFamily::Name("icons".into()), icons);
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
            if !motion_on() {
                s.animation_time = 0.0;
            }
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
        if let Some(g) = cc.storage.and_then(|s| s.get_string("grey")?.parse().ok()) {
            GREY.store(g, Ordering::Relaxed);
        }
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
            update_working: false,
            update: None,
            report_working: false,
            report_err: None,
            dest,
            pick,
            sizes: None,
            tally: None,
            choosing: false,
            status: String::new(),
            now: String::new(),
            fraction: None,
            stopped_at: None,
            tiles: None,
            free: None,
            progress_from: Instant::now(),
            paused_at: None,
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
            help_from: Tab::Backup,
            bar_h: 64.0,
            confirm_close: false,
            stop_only: false,
            closing: false,
            tab: Tab::Backup,
            tab_at: Instant::now(),
            logo: None,
            motion: motion_on(),
            // A first run has no saved backup folder yet.
            intro: cc
                .storage
                .is_some_and(|s| s.get_string("dest").is_none())
                .then_some(0),
            sound: cc.storage.and_then(|s| s.get_string("sound")).as_deref() == Some("1"),
            sound_pick: cc
                .storage
                .and_then(|s| s.get_string("sound_pick"))
                .and_then(|s| s.parse().ok())
                .filter(|&i| i < backup::SOUNDS.len())
                .unwrap_or(0),
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
            version: None,
            keep: cc
                .storage
                .and_then(|s| s.get_string("keep"))
                .and_then(|s| s.parse().ok())
                .filter(|&i| i < KEEP.len())
                .unwrap_or(0),
            picker: None,
            from: None,
            to: None,
            safety: true,
            source: None,
            ask: None,
            tidy_msg: None,
            tool_tx,
            tool_rx,
            tool: 0,
            tool_busy: None,
            stop_tool: false,
            auto_zoom: (cc
                .storage
                .and_then(|s| s.get_string("auto_zoom"))
                .as_deref()
                != Some("off"))
            .then(|| ctx.zoom_factor()),
            tool_status: String::new(),
            tool_msg: None,
            dupes: None,
            ticked: HashSet::new(),
            free_pick: None,
            confirm: None,
            scrcpy: None,
            reading: None,
            fixture: Fixture::Real,
            scroll: None,
            shot: None,
        };
        // PB_FIXTURE="worst|empty|one|many|pixel [restore] [checked] [transfer] [tools] [tool=<0-5>] [history] [settings] [sound] [privacy] [license] [intro[=<step>]]
        // [delete|rename|copy] [light|dark] [wifi] [busy [paused]] [stopask] [stoptool] [choose] [done] [stopped] [noadb[=working|failed]] [badusb] [zoom=<factor>] [scroll=<px>] [pick=<backup folder>]
        // [read=<backup folder> [contacts|calls|calendar]] [size=<w>x<h>] [shot=<file.png>]" opens
        // straight on test data; delete, rename and copy open that dialog for the first backup; wifi opens its dialog; checked shows the chosen backup as checked; choose opens "Choose files to back up"; busy shows a job running; paused shows it paused; stopask asks whether to stop it; stoptool asks whether to stop a running duplicate search; done and stopped show a backup that finished cleanly or was stopped; noadb shows adb missing; badusb shows a USB device Windows could not use; pick= opens the file picker on a real folder; read= opens the Read page on a real folder; intro opens the first-run steps; shot= saves a
        // picture of the window and closes.
        // update=working|new|same|failed also shows that result of Check for updates.
        if cfg!(debug_assertions) && test_run() {
            app.intro = None;
            let spec = env::var("PB_FIXTURE").unwrap_or_default();
            for word in spec.split_whitespace() {
                match word {
                    "privacy" => app.tab = Tab::Privacy,
                    "license" => app.tab = Tab::License,
                    "help" => app.tab = Tab::Help,
                    _ if word.starts_with("intro") => {
                        app.intro = Some(word.get(6..).and_then(|n| n.parse().ok()).unwrap_or(0))
                    }
                    "worst" => app.load_fixture(Fixture::Worst),
                    "empty" => app.load_fixture(Fixture::Empty),
                    "one" => app.load_fixture(Fixture::One),
                    "many" => app.load_fixture(Fixture::Many),
                    "pixel" => app.load_fixture(Fixture::Pixel),
                    "checked" => app.fixture_checked(),
                    "restore" => app.tab = Tab::Restore,
                    "transfer" => app.tab = Tab::Transfer,
                    "history" => {
                        app.tab = Tab::Restore;
                        let b = app.backups.first().map(|b| b.path.clone());
                        app.choose(b.clone());
                        let run =
                            |name: &str, how: &str, added: usize, replaced: usize| backup::Run {
                                name: name.into(),
                                how: how.into(),
                                added: vec![(String::new(), 0); added],
                                replaced: vec![
                                    backup::Replaced {
                                        path: String::new(),
                                        size: 0,
                                        sum: None,
                                    };
                                    replaced
                                ],
                                finished: true,
                                ..Default::default()
                            };
                        let runs = vec![
                            run("2026-09-02_191407", "USB or Wi-Fi", 4210, 0),
                            run("2026-10-14_082233", "MTP", 312, 6),
                            run("2026-11-10_143205", "USB or Wi-Fi", 57, 3),
                        ];
                        let pick = Some(runs[1].name.clone());
                        app.ask = Some(Ask::History(b.unwrap_or_default(), runs, pick));
                    }
                    "delete" | "rename" | "copy" => {
                        app.tab = Tab::Restore;
                        let b = app
                            .backups
                            .first()
                            .map(|b| b.path.clone())
                            .unwrap_or_default();
                        let into = app.backups.get(1).map(|b| b.path.clone());
                        app.ask = Some(match word {
                            "delete" => Ask::Delete(b, 3),
                            "rename" => Ask::Rename(
                                b,
                                "Mum's old phone: before the reset".into(),
                                Some("Windows does not allow that name.".into()),
                            ),
                            _ => Ask::Copy(b, into),
                        });
                    }
                    "tools" => app.tab = Tab::Tools,
                    _ if word.starts_with("tool=") => app.tool = word[5..].parse().unwrap_or(0),
                    "settings" => app.tab = Tab::Settings,
                    "sound" => app.sound = true,
                    _ if word.starts_with("update=") => {
                        app.update_working = word == "update=working";
                        app.update = match &word[7..] {
                            "new" => Some(Ok(Some("12.34.56".into()))),
                            "same" => Some(Ok(None)),
                            "failed" => Some(Err("Could not reach GitHub (curl: (28) Connection timed out after 30002 milliseconds). Check the internet connection, then try again.".into())),
                            _ => None,
                        };
                    }
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
                    "stopask" => (app.confirm_close, app.stop_only) = (true, true),
                    "paused" => {
                        backup::PAUSE.store(true, Ordering::Relaxed);
                        app.paused_at = Some(Instant::now());
                    }
                    "stoptool" => (app.tool_busy, app.stop_tool) = (Some("dupes"), true),
                    "badusb" => app.phones = Some(vec![Phone::UsbFailed]),
                    _ if word.starts_with("noadb") => {
                        app.adb_ok = false;
                        app.adb_working = word == "noadb=working";
                        if word == "noadb=failed" {
                            app.adb_err = Some(r"Cannot save to C:\Users\Bartholomew Fitzgerald\AppData\Local\Programs\Phone Backup: Access is denied. (os error 5). Move this app to a folder you can change, such as your Desktop, then try again.".into());
                        }
                    }
                    "report=working" => app.report_working = true,
                    "report=failed" => {
                        app.report_err = Some("The phone did not answer. Check that it is still connected, then try again.".into());
                    }
                    "busy" => {
                        app.busy.store(true, Ordering::Relaxed);
                        if app.tab != Tab::Backup {
                            app.job = Job::Restore;
                        }
                        app.status = "Copying 48,213 of 128,456 files…".into();
                        app.now = r"/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4".into();
                        app.fraction = Some(0.375);
                        app.progress_from = Instant::now() - Duration::from_secs(431);
                        app.tiles = Some((
                            [19_200_000_000, 0, 0, 0, 0, 0],
                            [
                                31_400_000_000,
                                9_800_000_000,
                                0,
                                1_200_000_000,
                                2_600_000_000,
                                3_100_000_000,
                            ],
                        ));
                    }
                    "done" if app.tab != Tab::Backup => {
                        app.restore_msg = Some(Ok(("Copied 1,093 files back to the phone. 47,120 files were already there.".into(), 0)));
                        app.restore_report = Some(app.dest.join("restore-report.txt"));
                    }
                    "stopped" if app.tab != Tab::Backup => {
                        app.restore_msg = Some(Err(
                            "You stopped the restore. Files copied so far stay on the phone."
                                .into(),
                        ));
                        app.stopped_at = Some(0.62);
                    }
                    "done" => {
                        app.result = Some(Ok(vec![Summary {
                            name: "Phone".into(),
                            root: app.dest.clone(),
                            files: 128_456,
                            copied: 1_093,
                            errors: vec![],
                            data: vec![("Contacts", true), ("Messages", true)],
                            notes: vec![],
                            full: true,
                            pick: Pick::default(),
                        }]));
                    }
                    "stopped" => {
                        app.result = Some(Err("You stopped the backup. Files copied so far are kept; press Try again to continue.".into()));
                        app.stopped_at = Some(0.62);
                        app.tiles = Some((
                            [31_400_000_000, 2_100_000_000, 0, 0, 0, 0],
                            [
                                31_400_000_000,
                                9_800_000_000,
                                0,
                                1_200_000_000,
                                2_600_000_000,
                                3_100_000_000,
                            ],
                        ));
                    }
                    _ if word.starts_with("zoom=") => {
                        app.auto_zoom = None;
                        ctx.set_zoom_factor(word[5..].parse().unwrap_or(1.0))
                    }
                    _ if word.starts_with("pick=") => app.open_picker(ctx, word[5..].into(), false),
                    _ if word.starts_with("read=") => {
                        if let Ok(book) = export::book_as_of(Path::new(&word[5..]), None) {
                            app.reading = Some(Reading::new("Test backup".into(), book));
                            app.tab = Tab::Read;
                        }
                    }
                    "contacts" | "calls" | "calendar" => {
                        if let Some(r) = &mut app.reading {
                            r.part = match word {
                                "contacts" => Part::Contacts,
                                "calls" => Part::Calls,
                                _ => Part::Calendar,
                            };
                            r.refilter();
                        }
                    }
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
    /// A job ended: the chosen sound, or the problem sound if it did not end well, and a taskbar
    /// flash if the window is in the background.
    fn finished(&self, ctx: &egui::Context, ok: bool) {
        if self.sound {
            backup::play_sound(if ok {
                backup::SOUNDS[self.sound_pick].1
            } else {
                backup::PROBLEM
            });
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
        self.auto_zoom(ui.ctx());
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
                Msg::Tiles(done, total) => self.tiles = Some((done, total)),
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
                Msg::Update(r) => {
                    self.update_working = false;
                    self.update = Some(r);
                }
                Msg::PhoneReport(r) => {
                    self.report_working = false;
                    self.report_err = r.err();
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
                    self.finished(ui.ctx(), r.is_ok());
                    self.tidy_msg = Some(r);
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
                    Some(plan) => self.preview = Some((plan, r.map(|(m, _)| m))),
                    None => {
                        // Some files or apps not restored is a problem too.
                        self.finished(ui.ctx(), matches!(r, Ok((_, 0))));
                        self.restore_msg = Some(r);
                        self.stopped_at = self.fraction;
                    }
                },
                Msg::Sizes(serial, list, apps) => {
                    self.sizes = Some((serial, Some((list, apps))));
                    self.tally = None;
                }
                Msg::Exported(folder, r) => {
                    self.finished(ui.ctx(), r.is_ok());
                    self.export_msg = Some((folder, r));
                }
                Msg::Done(r) => {
                    self.finished(ui.ctx(), r.as_ref().is_ok_and(|d| ui::problems(d) == 0));
                    self.result = Some(r);
                    self.stopped_at = self.fraction;
                    self.free = None;
                }
                Msg::Verified(folder, r) => {
                    self.finished(ui.ctx(), r.is_ok());
                    self.verified = Some((folder, r));
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
                tools::Done::Dupes(Err(e))
                | tools::Done::Matched(_, Err(e))
                | tools::Done::Book(Err(e)) => self.tool_msg = Some((which, Err(e))),
                tools::Done::Book(Ok(book)) => {
                    let name = self
                        .backups
                        .iter()
                        .find(|b| Some(&b.path) == self.chosen.as_ref())
                        .map_or(String::new(), |b| b.name.clone());
                    self.go(Tab::Read);
                    self.reading = Some(Reading::new(name, book));
                }
                tools::Done::Matched(_, Ok((list, left))) if list.is_empty() => {
                    self.tool_msg = Some((
                        which,
                        Err(format!(
                            "None of the {} chosen match their backup copy on the phone, so nothing was deleted.",
                            files(left)
                        )),
                    ))
                }
                tools::Done::Matched(serial, Ok((list, left))) => {
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
            self.stop_only = false;
        }
        if self.closing && !busy {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // The job ended while Stop was being asked about: nothing is left to stop.
        if self.stop_only && !busy {
            self.confirm_close = false;
        }
        if self.confirm_close {
            let transfer = self.tab == Tab::Transfer && self.job == Job::Restore;
            let (title, body) = match self.job {
                _ if transfer => ("Stop the transfer?", "Files copied so far are kept."),
                Job::Restore => (
                    "Stop restoring?",
                    "Everything restored so far stays on the phone.",
                ),
                Job::Backup => (
                    "Stop the backup?",
                    "Files copied so far are kept. The next backup continues where this one stopped.",
                ),
                Job::Copy => ("Stop copying?", "Files copied so far stay in the backup."),
                Job::Verify => (
                    "Stop checking the backup?",
                    "The backup is not changed. You can check it again later.",
                ),
                Job::Export => (
                    "Stop exporting?",
                    "Files exported so far are kept. The backup itself is not changed.",
                ),
                Job::Delete => (
                    "Close when the delete ends?",
                    "Windows is moving the backup to the Recycle Bin. The window closes when it has finished.",
                ),
            };
            let (go, stay) = match self.job {
                _ if transfer => ("Stop and close", "Keep transferring"),
                Job::Restore => ("Stop and close", "Keep restoring"),
                Job::Backup => ("Stop and close", "Keep backing up"),
                Job::Copy => ("Stop and close", "Keep copying"),
                Job::Delete => ("Close when done", "Keep open"),
                Job::Verify => ("Stop and close", "Keep checking"),
                Job::Export => ("Stop and close", "Keep exporting"),
            };
            let go = if self.stop_only { "Stop" } else { go };
            // Esc or a click outside means keep going, the safe answer.
            if let Some(yes) = confirm(ui.ctx(), title, go, stay, true, |ui| {
                ui.add(egui::Label::new(body).wrap());
            }) {
                if yes {
                    backup::STOP.store(true, Ordering::Relaxed);
                    self.closing = !self.stop_only;
                }
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
        self.stop_tool_dialog(ui.ctx());

        // Esc leaves Privacy and License for Settings, where they open from.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            match self.tab {
                Tab::Privacy | Tab::License => self.go(Tab::Settings),
                Tab::Help => self.go(self.help_from),
                _ => {}
            }
        }
        // With the search box focused, Esc only leaves the box.
        if self.tab == Tab::Read
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
            && ui.ctx().memory(|m| m.focused().is_none())
        {
            self.go(Tab::Tools);
        }
        // A side rail when there is room, a strip across the top when not, nothing on the intro.
        let full = ui.max_rect();
        let main = if self.intro.is_some() {
            full
        } else if full.width() >= 720.0 {
            const RAIL: f32 = 164.0;
            let area = Rect::from_min_size(full.min, vec2(RAIL, full.height()));
            self.nav(ui, &t, area, true);
            full.with_min_x(area.right())
        } else {
            let area = Rect::from_min_size(full.min, vec2(full.width(), 54.0));
            self.nav(ui, &t, area, false);
            full.with_min_y(area.bottom())
        };
        let pad: i8 = if main.width() < 560.0 { 16 } else { 32 };
        ui.painter().rect_filled(main, 0, t.layer);
        ui.scope_builder(egui::UiBuilder::new().max_rect(main), |ui| {
            self.page(ui, &t, pad);
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
        storage.set_string("sound_pick", self.sound_pick.to_string());
        storage.set_string("keep", self.keep.to_string());
        storage.set_string("grey", GREY.load(Ordering::Relaxed).to_string());
        storage.set_string(
            "auto_zoom",
            if self.auto_zoom.is_some() {
                "on"
            } else {
                "off"
            }
            .into(),
        );
        storage.set_string(
            "pick",
            [self.pick.files, self.pick.apps, self.pick.data]
                .iter()
                .chain(&self.pick.kinds)
                .map(|&b| bit(b))
                .collect(),
        );
        // Each folder ends its line, so {""} (top-level files only) saves as a blank line, not
        // as nothing, which would load back as all folders.
        let folders: String = self.pick.folders.iter().map(|f| format!("{f}\n")).collect();
        storage.set_string("folders", folders);
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
        assert_eq!(super::days_ago(Some(344)), "11 months ago");
        assert_eq!(super::days_ago(Some(345)), "1 year ago");
        assert_eq!(super::days_ago(Some(365)), "1 year ago");
        assert_eq!(super::days_ago(Some(1284)), "3 years ago");
    }

    #[test]
    fn says_time_left() {
        assert_eq!(super::time_left(30), "less than a minute left");
        assert_eq!(super::time_left(690), "about 12 min left");
        assert_eq!(super::time_left(3540), "about 59 min left");
        assert_eq!(super::time_left(3600), "about 1 h left");
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

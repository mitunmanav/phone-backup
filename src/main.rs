// No console window behind the app on Windows.
#![windows_subsystem = "windows"]

mod backup;

use backup::{Msg, Phone, Pick, Summary, count, files, or_list, size};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, Rect, RichText, Stroke, StrokeKind,
    TextStyle, pos2, vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use std::{env, fs, thread};

struct Tokens {
    bg: Color32,
    /// The two panes: one step lighter than `bg`.
    surface: Color32,
    field: Color32,
    line: Color32,
    /// Edges of fields, tick boxes and buttons: at least 3:1 against `bg` and `field`.
    border: Color32,
    ink: Color32,
    muted: Color32,
    /// Fill of the main button, the chosen tab and progress.
    accent: Color32,
    /// Accent used as text or a thin line on `bg` and `surface`: at least 4.5:1.
    accent_ink: Color32,
    on_accent: Color32,
    error: Color32,
}

fn tokens(dark: bool) -> Tokens {
    let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if dark {
        Tokens {
            bg: hex(0x10161C),
            surface: hex(0x161E26),
            field: hex(0x1C2630),
            line: hex(0x2C3946),
            border: hex(0x657482),
            ink: hex(0xE6ECF1),
            muted: hex(0x93A1AE),
            // White on this plum is 5.9:1; the old pastel pink glowed as a big fill.
            accent: hex(0x9E3F8F),
            accent_ink: hex(0xD08BC6),
            on_accent: hex(0xFFFFFF),
            error: hex(0xFF9D6B),
        }
    } else {
        Tokens {
            bg: hex(0xE9EDF1),
            surface: hex(0xFFFFFF),
            field: hex(0xF6F8FA),
            line: hex(0xC5CDD5),
            border: hex(0x76828D),
            ink: hex(0x16202B),
            muted: hex(0x56616D),
            accent: hex(0x7A2E6E),
            accent_ink: hex(0x7A2E6E),
            on_accent: hex(0xFFFFFF),
            error: hex(0xA8380A),
        }
    }
}

fn visuals(dark: bool) -> egui::Visuals {
    let t = tokens(dark);
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = t.bg;
    v.window_fill = t.field;
    v.extreme_bg_color = t.field;
    v.override_text_color = Some(t.ink);
    v.hyperlink_color = t.accent_ink;
    v.selection.bg_fill = t.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(2.0, t.accent_ink);
    v.widgets.inactive.weak_bg_fill = t.field;
    v.widgets.inactive.bg_fill = t.field;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.accent_ink);
    // egui draws keyboard focus with the pressed style; 2 px of ink makes it visible on every fill.
    v.widgets.active.bg_stroke = Stroke::new(2.0, t.ink);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        // Windows 11: 4 px on controls, 8 px on dialogs and menus.
        w.corner_radius = 4.into();
    }
    v.window_corner_radius = 8.into();
    v.menu_corner_radius = 8.into();
    v
}

/// One of the two panes: a raised surface with a hairline edge.
fn pane(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.line))
        .corner_radius(8)
        .inner_margin(24)
}

/// What a finished job says, kept together in one quiet box.
fn result_box(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.field)
        .stroke(Stroke::new(1.0, t.line))
        .corner_radius(4)
        .inner_margin(16)
}

/// Small semibold heading over a group of controls, with room above it.
fn section(ui: &mut egui::Ui, t: &Tokens, text: &str, first: bool) {
    if !first {
        ui.add_space(16.0);
    }
    ui.label(RichText::new(text).font(semibold(13.0)).color(t.muted));
    ui.add_space(2.0);
}

fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// "192.168.1.5:37145" or "phone.local:5555": a host, a colon and a port number.
fn valid_addr(addr: &str) -> bool {
    addr.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty() && !host.contains(' ') && port.parse::<u16>().is_ok_and(|p| p > 0)
    })
}

/// USB debugging only helps once adb is on this computer.
fn adb_missing(ui: &mut egui::Ui, t: &Tokens) {
    ui.label(RichText::new("USB debugging also needs Google's free Platform Tools on this laptop. Unzip them and put adb.exe next to this app.").color(t.muted));
    ui.hyperlink_to(
        "Download Platform Tools",
        "https://developer.android.com/tools/releases/platform-tools",
    );
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Phone Backup")
            .with_inner_size([960.0, 700.0])
            .with_min_inner_size([360.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Phone Backup",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// What the file picker's worker sends back.
enum Found {
    List(Result<Vec<(PathBuf, u64)>, String>),
    Thumb(PathBuf, Option<egui::ColorImage>),
}

/// The open "Choose files" dialog. Dropping it ends its worker thread.
struct Picker {
    folder: PathBuf,
    /// None while the worker reads the backup.
    files: Option<Result<Vec<(PathBuf, u64)>, String>>,
    chosen: HashSet<PathBuf>,
    /// None: not a picture this app can show.
    thumbs: HashMap<PathBuf, Option<egui::TextureHandle>>,
    /// The rows last asked for, so the worker hears only of changes.
    asked: Vec<PathBuf>,
    ask: mpsc::Sender<Vec<PathBuf>>,
    rx: mpsc::Receiver<Found>,
}

/// An 80 px square preview of a picture, turned the way the camera held it.
fn thumbnail(path: &Path) -> Option<egui::ColorImage> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let turn = decoder.orientation().ok()?;
    let mut img = image::DynamicImage::from_decoder(decoder)
        .ok()?
        .resize_to_fill(80, 80, image::imageops::FilterType::Triangle);
    img.apply_orientation(turn);
    let rgba = img.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        &rgba,
    ))
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Backup,
    Restore,
}

/// What the drawn phone shows on its screen.
enum Screen<'a> {
    Empty,
    Allow,
    Ready(&'a str),
    Busy(Option<f32>),
}

/// The phone drawing: the outline is grey until a phone is connected, then plum; while a job
/// runs, the screen fills from the bottom as it progresses.
fn draw_phone(ui: &mut egui::Ui, t: &Tokens, scale: f32, screen: Screen) {
    let size = vec2(132.0, 252.0) * scale;
    let (area, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), size.y), egui::Sense::hover());
    let body = Rect::from_center_size(area.center(), size);
    let glass = Rect::from_min_max(
        body.min + vec2(8.0, 24.0) * scale,
        body.max - vec2(8.0, 24.0) * scale,
    );
    let p = ui.painter();
    let live = matches!(screen, Screen::Ready(_) | Screen::Busy(_));
    p.rect_filled(body, 26.0 * scale, t.field);
    p.rect_stroke(
        body,
        26.0 * scale,
        Stroke::new(2.0, if live { t.accent_ink } else { t.line }),
        StrokeKind::Inside,
    );
    p.rect_filled(
        Rect::from_center_size(
            pos2(body.center().x, body.top() + 12.0 * scale),
            vec2(34.0, 4.0) * scale,
        ),
        2.0,
        t.line,
    );
    p.rect_filled(
        Rect::from_center_size(
            pos2(body.center().x, body.bottom() + -12.0 * scale),
            vec2(44.0, 4.0) * scale,
        ),
        2.0,
        t.line,
    );
    p.rect_filled(glass, 8.0 * scale, t.bg);
    let words = |text: &str, font: FontId, color: Color32| {
        // Each wrapped line is centred, not just the block.
        let mut job =
            egui::text::LayoutJob::simple(text.to_string(), font, color, glass.width() - 16.0);
        job.halign = Align::Center;
        let galley = p.layout_job(job);
        p.galley(
            pos2(glass.center().x, glass.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    };
    match screen {
        Screen::Empty => words(
            "No phone",
            FontId::proportional(14.0 * scale.max(0.9)),
            t.muted,
        ),
        Screen::Allow => words("Tap Allow", semibold(16.0 * scale.max(0.9)), t.accent_ink),
        Screen::Ready(name) => {
            p.rect_filled(glass, 8.0 * scale, t.accent.gamma_multiply(0.14));
            words(name, semibold(17.0 * scale.max(0.85)), t.ink);
        }
        Screen::Busy(fraction) => {
            let f = fraction.unwrap_or(0.0).clamp(0.0, 1.0);
            let fill = Rect::from_min_max(
                pos2(glass.left(), glass.bottom() - glass.height() * f),
                glass.max,
            );
            p.rect_filled(fill, 8.0 * scale, t.accent.gamma_multiply(0.45));
            let text = fraction.map_or("Working…".to_string(), |f| {
                format!("{}%", (f * 100.0).floor())
            });
            // Regular weight: its digits are all one width, so the number does not wobble as it counts.
            words(&text, FontId::proportional(26.0 * scale.max(0.8)), t.ink);
        }
    }
}

/// The main action of a tab: plum when it can run, a quiet field otherwise. A fixed width, not
/// the whole pane, so it reads as a button rather than a banner.
fn primary(ui: &mut egui::Ui, t: &Tokens, label: &str, enabled: bool) -> bool {
    let (fill, text) = if enabled {
        (t.accent, t.on_accent)
    } else {
        (t.field, t.muted)
    };
    let button = egui::Button::new(RichText::new(label).font(semibold(14.0)).color(text))
        .fill(fill)
        .corner_radius(4);
    ui.scope(|ui| {
        if !enabled {
            ui.disable();
        }
        ui.style_mut().visuals.disabled_alpha = 1.0;
        ui.add_sized([ui.available_width().min(240.0), 40.0], button)
            .clicked()
    })
    .inner
}

/// A checkbox with a one-line explanation under its label.
fn choice(ui: &mut egui::Ui, t: &Tokens, enabled: bool, value: &mut bool, label: &str, note: &str) {
    // A box that cannot be used shows empty, so a grey tick does not read as "will be included".
    let mut off = false;
    let value = if enabled { value } else { &mut off };
    // No add_enabled_ui scope here: each scope adds its own gap, so rows would sit unevenly.
    let above = ui.add_enabled(enabled, egui::Checkbox::new(value, label));
    note_under(
        ui,
        t,
        note,
        enabled,
        &above,
        label,
        TextStyle::Body.resolve(ui.style()),
    );
}

/// Muted text lined up with a checkbox or radio label, tight under it. `above` is that control,
/// and `label` and `font` its text, so a label that wraps to more lines still clears the note.
fn note_under(
    ui: &mut egui::Ui,
    t: &Tokens,
    note: &str,
    enabled: bool,
    above: &egui::Response,
    label: &str,
    font: FontId,
) {
    let indent = ui.spacing().icon_width + ui.spacing().icon_spacing;
    // The control is at least 32 px tall for an easy click, with its label centred in it;
    // pull the note up into the space under the label.
    let label_h = ui
        .painter()
        .layout(
            label.into(),
            font,
            Color32::PLACEHOLDER,
            ui.available_width() - indent,
        )
        .size()
        .y;
    ui.add_space(-((above.rect.height() - label_h) / 2.0 + 4.0).max(0.0));
    ui.horizontal_top(|ui| {
        // A text row, not a control: without this it would be 32 px tall.
        ui.spacing_mut().interact_size.y = 18.0;
        if !enabled {
            ui.disable();
        }
        ui.add_space(indent);
        ui.add(egui::Label::new(RichText::new(note).color(t.muted).size(12.0)).wrap());
    });
    ui.add_space(8.0);
}

/// A folder path that fits the width, cut in the middle so the last folders stay readable:
/// "C:\Users\…\Documents\Phone backups".
fn short_path(ui: &egui::Ui, path: &str) -> String {
    let font = TextStyle::Body.resolve(ui.style());
    let fits = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.into(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
            <= ui.available_width()
    };
    let parts: Vec<&str> = path.split('\\').collect();
    if fits(path) || parts.len() < 3 {
        return path.into();
    }
    let cut = |keep: usize| format!("{}\\…\\{}", parts[0], parts[keep..].join("\\"));
    // Drop folders after the drive one at a time; if even the last folder alone is too wide, the label truncates it.
    (2..parts.len())
        .map(cut)
        .find(|s| fits(s))
        .unwrap_or_else(|| cut(parts.len() - 1))
}

/// 0 -> "today", 1 -> "yesterday", 5 -> "5 days ago", 75 -> "2 months ago", 1284 -> "3 years ago"
fn days_ago(days: Option<u64>) -> String {
    let ago = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match days {
        Some(0) => "today".into(),
        Some(1) => "yesterday".into(),
        Some(n @ ..=44) => ago(n, "day"),
        Some(n @ ..=364) => ago((n + 15) / 30, "month"),
        Some(n) => ago(n / 365, "year"),
        None => "at an unknown date".into(),
    }
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
    wifi_msg: Option<Result<String, String>>,
    wifi_working: bool,
    wifi_open: bool,
    /// Asking whether to stop a running backup before closing.
    confirm_close: bool,
    /// Close as soon as the stopped backup has finished cleaning up.
    closing: bool,
    tab: Tab,
    /// The busy job is a restore, not a backup.
    restoring: bool,
    restore_msg: Option<Result<String, String>>,
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
    /// Debug builds only: which test data is shown instead of the real phones and backups.
    fixture: Fixture,
    /// Debug builds only: scroll position to open at, for screenshots.
    scroll: Option<f32>,
}

/// A restore's choices: phone serial, backup folder, files, apps, and the chosen files (None: all).
type Plan = (String, PathBuf, bool, bool, Option<Arc<HashSet<PathBuf>>>);

/// Test data for checking the layout against realistic worst cases. Never shown in release builds.
#[derive(Clone, Copy, PartialEq)]
enum Fixture {
    Real,
    Worst,
    Empty,
    One,
    Many,
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
            s.spacing.scroll = egui::style::ScrollStyle::solid();
        });
        ctx.set_visuals_of(egui::Theme::Light, visuals(false));
        ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
        let (tx, rx) = mpsc::channel();
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
        let pick = Pick {
            files: on(0),
            apps: on(1),
            data: on(2),
        };

        let mut app = Self {
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
            confirm_close: false,
            closing: false,
            tab: Tab::Backup,
            restoring: false,
            restore_msg: None,
            checking: None,
            preview: None,
            backups: Vec::new(),
            scanned: PathBuf::new(),
            chosen: None,
            restore_files: true,
            restore_apps: true,
            only: None,
            picker: None,
            fixture: Fixture::Real,
            scroll: None,
        };
        // PB_FIXTURE="worst|empty|one|many [restore] [light|dark] [scroll=<px>] [pick=<backup folder>]"
        // opens straight on test data; pick= opens the file picker on a real folder.
        if cfg!(debug_assertions) {
            let spec = env::var("PB_FIXTURE").unwrap_or_default();
            for word in spec.split_whitespace() {
                match word {
                    "worst" => app.load_fixture(Fixture::Worst),
                    "empty" => app.load_fixture(Fixture::Empty),
                    "one" => app.load_fixture(Fixture::One),
                    "many" => app.load_fixture(Fixture::Many),
                    "restore" => app.tab = Tab::Restore,
                    "light" => ctx.set_theme(egui::Theme::Light),
                    "dark" => ctx.set_theme(egui::Theme::Dark),
                    _ if word.starts_with("pick=") => app.open_picker(ctx, word[5..].into()),
                    _ => app.scroll = word.strip_prefix("scroll=").and_then(|n| n.parse().ok()),
                }
            }
        }
        app
    }

    fn start(&mut self, ctx: &egui::Context) {
        let phones = self.phones.clone().unwrap_or_default();
        let (dest, pick, tx, ctx, busy) = (
            self.dest.clone(),
            self.pick,
            self.tx.clone(),
            ctx.clone(),
            self.busy.clone(),
        );
        busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        self.restoring = false;
        self.result = None;
        self.status = "Starting…".into();
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
                Err(format!(
                    "Cannot use the folder {}: {e}. Choose another folder.",
                    dest.display()
                ))
            } else if adb.is_empty() {
                backup::mtp_backup(&dest, &report).map(|s| vec![s])
            } else {
                // One phone failing (say, unplugged) must not skip the others.
                let (mut done, mut failed) = (Vec::new(), Vec::new());
                for (serial, model) in &adb {
                    match backup::adb_backup(serial, model, &dest, pick, &report) {
                        Ok(s) => done.push(s),
                        Err(e) if adb.len() > 1 => failed.push(format!("{model}: {e}")),
                        Err(e) => failed.push(e),
                    }
                }
                if failed.is_empty() {
                    Ok(done)
                } else {
                    let names: Vec<_> = done.iter().map(|s| s.name.clone()).collect();
                    let ok = if names.is_empty() {
                        String::new()
                    } else {
                        format!("\n{} backed up fine.", names.join(", "))
                    };
                    Err(failed.join("\n") + &ok)
                }
            };
            busy.store(false, Ordering::Relaxed);
            report(Msg::Done(result));
        });
    }

    /// Restores photos and files first, then apps, from one backup folder.
    /// With `check`, changes nothing on the phone and reports what a restore would do.
    fn restore(&mut self, ctx: &egui::Context, plan: Plan, check: bool) {
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        self.restoring = true;
        self.restore_msg = None;
        self.preview = None;
        self.checking = check.then(|| plan.clone());
        let (serial, folder, files, apps, only) = plan;
        self.status = "Starting…".into();
        self.now.clear();
        self.fraction = None;
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let mut said = Vec::new();
            let mut result = Ok(());
            if files {
                result = backup::restore_files(&serial, &folder, only.as_deref(), check, &report)
                    .map(|m| said.push(m));
            }
            if apps && result.is_ok() {
                result =
                    backup::restore_apps(&serial, &folder, check, &report).map(|m| said.push(m));
            }
            let result = match result {
                Ok(()) => Ok(said.join("\n")),
                Err(e) => Err(said.into_iter().chain([e]).collect::<Vec<_>>().join("\n")),
            };
            busy.store(false, Ordering::Relaxed);
            report(Msg::Restored(result));
        });
    }

    /// Opens the file picker; a worker lists the backup, then makes previews of the rows in view.
    fn open_picker(&mut self, ctx: &egui::Context, folder: PathBuf) {
        let (ask, asks) = mpsc::channel::<Vec<PathBuf>>();
        let (tx, rx) = mpsc::channel();
        let (ctx, dir) = (ctx.clone(), folder.clone());
        thread::spawn(move || {
            let send = |f: Found| {
                ctx.request_repaint();
                tx.send(f).is_ok()
            };
            if !send(Found::List(backup::backup_files(&dir))) {
                return;
            }
            let mut todo = Vec::new();
            loop {
                // Only the rows in view now: rows scrolled past are never decoded.
                while let Ok(rows) = asks.try_recv() {
                    todo = rows;
                }
                if todo.is_empty() {
                    match asks.recv() {
                        Ok(rows) => todo = rows,
                        Err(_) => return,
                    }
                    continue;
                }
                let path = todo.remove(0);
                let img = thumbnail(&path);
                if !send(Found::Thumb(path, img)) {
                    return;
                }
            }
        });
        self.picker = Some(Picker {
            folder,
            files: None,
            chosen: HashSet::new(),
            thumbs: HashMap::new(),
            asked: Vec::new(),
            ask,
            rx,
        });
    }

    fn picker_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(p) = &mut self.picker else { return };
        while let Ok(found) = p.rx.try_recv() {
            match found {
                Found::List(list) => {
                    if let Ok(list) = &list {
                        p.chosen = match &self.only {
                            Some((folder, chosen, _)) if *folder == p.folder => (**chosen).clone(),
                            _ => list.iter().map(|(f, _)| f.clone()).collect(),
                        };
                    }
                    p.files = Some(list);
                }
                Found::Thumb(path, img) => {
                    let tex = img.map(|img| {
                        ctx.load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR)
                    });
                    p.thumbs.insert(path, tex);
                }
            }
        }
        let mut wanted = Vec::new();
        let modal = egui::Modal::new(egui::Id::unique("picker")).show(ctx, |ui| {
            ui.set_width(520.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(RichText::new("Choose files to restore").font(semibold(16.0)));
            ui.add_space(4.0);
            match &p.files {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Reading the backup…");
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(t.error));
                }
                Some(Ok(list)) => {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Select all").clicked() {
                            p.chosen = list.iter().map(|(f, _)| f.clone()).collect();
                        }
                        if ui.button("Select none").clicked() {
                            p.chosen.clear();
                        }
                        // Kept whole: on a narrow window it moves under the buttons instead.
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "{} of {} chosen",
                                    count(p.chosen.len()),
                                    files(list.len())
                                ))
                                .color(t.muted),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                    ui.add_space(4.0);
                    let files_root = p.folder.join("files");
                    let height = (ctx.content_rect().height() - 240.0).clamp(160.0, 480.0);
                    // Only the rows in view are built, so a backup of 100,000 files stays quick.
                    egui::ScrollArea::vertical()
                        .max_height(height)
                        .auto_shrink([false, true])
                        .show_rows(ui, 40.0, list.len(), |ui, rows| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            for (path, bytes) in &list[rows] {
                                ui.horizontal(|ui| {
                                    let (tile, _) = ui.allocate_exact_size(
                                        vec2(40.0, 40.0),
                                        egui::Sense::hover(),
                                    );
                                    match p.thumbs.get(path) {
                                        Some(Some(tex)) => {
                                            egui::Image::new(tex)
                                                .corner_radius(4)
                                                .paint_at(ui, tile);
                                        }
                                        known => {
                                            if known.is_none() {
                                                wanted.push(path.clone());
                                            }
                                            // Not a picture, or not ready yet: its type instead.
                                            ui.painter().rect(
                                                tile,
                                                4,
                                                t.bg,
                                                Stroke::new(1.0, t.line),
                                                StrokeKind::Inside,
                                            );
                                            let ext: String = path
                                                .extension()
                                                .map(|e| e.to_string_lossy().to_uppercase())
                                                .unwrap_or_default()
                                                .chars()
                                                .take(4)
                                                .collect();
                                            ui.painter().text(
                                                tile.center(),
                                                Align2::CENTER_CENTER,
                                                ext,
                                                FontId::proportional(12.0),
                                                t.muted,
                                            );
                                        }
                                    }
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().interact_size.y = 20.0;
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        let name = path
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                        let mut on = p.chosen.contains(path);
                                        if ui.checkbox(&mut on, name).changed() {
                                            if on {
                                                p.chosen.insert(path.clone());
                                            } else {
                                                p.chosen.remove(path);
                                            }
                                        }
                                        let dir = path.parent().unwrap_or(path);
                                        let dir = dir
                                            .strip_prefix(&files_root)
                                            .or_else(|_| dir.strip_prefix(&p.folder))
                                            .unwrap_or(dir);
                                        ui.horizontal(|ui| {
                                            ui.add_space(
                                                ui.spacing().icon_width + ui.spacing().icon_spacing,
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} in {}",
                                                    size(*bytes),
                                                    dir.display()
                                                ))
                                                .color(t.muted)
                                                .size(12.0),
                                            );
                                        });
                                    });
                                });
                            }
                        });
                }
            }
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.button("Done").clicked()
            })
            .inner
        });
        if wanted != p.asked {
            let _ = p.ask.send(wanted.clone());
            p.asked = wanted;
        }
        // Esc or a click outside keeps the choice too: nothing has changed on the phone yet.
        if (modal.inner || modal.should_close())
            && let Some(p) = self.picker.take()
            && let Some(Ok(list)) = p.files
        {
            self.only =
                (p.chosen.len() < list.len()).then(|| (p.folder, Arc::new(p.chosen), list.len()));
        }
    }

    fn run_wifi(&mut self, ctx: &egui::Context, args: Vec<String>) {
        self.wifi_working = true;
        self.wifi_msg = None;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = tx.send(Msg::Wifi(backup::wifi(&args)));
            ctx.request_repaint();
        });
    }

    /// Re-reads the backups in `folder` and keeps the choice when it is still there.
    fn scan(&mut self, folder: &std::path::Path) {
        if self.fixture != Fixture::Real {
            return;
        }
        self.backups = backup::saved_backups(folder);
        self.scanned = folder.to_path_buf();
        if !self
            .backups
            .iter()
            .any(|b| Some(&b.path) == self.chosen.as_ref())
        {
            self.chosen = self.backups.first().map(|b| b.path.clone());
        }
    }

    fn content(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        // Side by side when there is room: the phone on the left, what to do with it on the right.
        if ui.available_width() >= 760.0 {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                // A frame keeps its parent's layout; inside this row it would lay its contents out sideways.
                pane(t).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(252.0);
                        self.device(ui, t, 0.75);
                    });
                });
                pane(t).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        self.tasks(ui, t);
                    });
                });
            });
        } else {
            // Stacked: keep lines short enough to read.
            // set_max_width can also widen, so never past the window.
            ui.set_max_width(ui.available_width().min(560.0));
            pane(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.device(ui, t, 0.6);
            });
            ui.add_space(16.0);
            pane(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.tasks(ui, t);
            });
        }
    }

    /// (model, Wi-Fi?) of each ADB phone, and the name of a phone in File transfer mode.
    fn connected(&self) -> (Vec<(String, bool)>, Option<String>) {
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

    /// Left side: the phone, how it is connected, and how to connect one.
    fn device(&mut self, ui: &mut egui::Ui, t: &Tokens, scale: f32) {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let (adb, mtp) = self.connected();
        let allow = self
            .phones
            .as_ref()
            .is_some_and(|p| p.contains(&Phone::Unauthorized));
        let name = match (adb.as_slice(), &mtp) {
            ([(model, _)], _) => Some(model.clone()),
            ([_, ..], _) => Some(format!("{} phones", adb.len())),
            ([], Some(n)) => Some(n.clone()),
            ([], None) => None,
        };
        let screen = match (&name, busy) {
            (Some(_), true) => Screen::Busy(self.fraction),
            (Some(n), false) => Screen::Ready(n),
            (None, _) if allow => Screen::Allow,
            (None, _) => Screen::Empty,
        };
        draw_phone(ui, t, scale, screen);
        ui.add_space(16.0);

        match (&self.phones, adb.as_slice(), &mtp) {
            (None, ..) => {
                ui.heading("Looking for your phone");
            }
            (Some(_), [(model, wifi)], _) => {
                ui.heading(model);
                ui.label(if *wifi {
                    "Connected over Wi-Fi."
                } else {
                    "Connected by USB cable."
                });
            }
            (Some(_), [_, ..], _) => {
                ui.heading(format!("{} phones connected", adb.len()));
                ui.label(
                    adb.iter()
                        .map(|(m, _)| m.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                ui.label(muted("Each phone gets its own backup folder."));
            }
            (Some(_), [], Some(n)) => {
                ui.heading(n);
                ui.label(
                    "Connected in File transfer mode, so only photos and files can be backed up.",
                );
                if self.adb_ok {
                    ui.label(muted(
                        "Turn on USB debugging to include apps, contacts and messages.",
                    ));
                } else {
                    adb_missing(ui, t);
                }
            }
            (Some(_), [], None) if allow => {
                ui.heading("Tap Allow on your phone");
                ui.label(
                    "Unlock the phone. It asks whether to allow USB debugging from this computer.",
                );
            }
            (Some(_), [], None) => {
                ui.heading("Connect your phone");
                // Text rows, not controls: drop the 32 px control height here.
                ui.spacing_mut().interact_size.y = 20.0;
                for (n, step) in [
                    "Plug it into this computer with a USB cable.",
                    "Unlock it, tap the USB notification and choose File transfer.",
                    "For apps, contacts and messages, also turn on USB debugging.",
                ]
                .iter()
                .enumerate()
                {
                    ui.horizontal_top(|ui| {
                        // Fixed-width number column so every line of a step starts at the same x.
                        ui.add_sized(
                            [16.0, 20.0],
                            egui::Label::new(
                                RichText::new(format!("{}.", n + 1))
                                    .font(semibold(14.0))
                                    .color(t.accent_ink),
                            ),
                        );
                        ui.add(egui::Label::new(*step).wrap());
                    });
                }
                ui.label(muted("USB debugging: Settings, About phone, tap Build number 7 times, then Developer options, USB debugging."));
                if !self.adb_ok {
                    adb_missing(ui, t);
                }
                ui.spacing_mut().interact_size.y = 32.0;
            }
        }

        // Wi-Fi is a one-time setup, so it opens in a dialog instead of stretching this pane.
        ui.add_space(16.0);
        if ui
            .add_enabled(!busy, egui::Button::new("Connect over Wi-Fi…"))
            .clicked()
        {
            self.wifi_open = true;
        }
    }

    fn wifi_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        let modal = egui::Modal::new(egui::Id::unique("wifi")).show(ctx, |ui| {
            ui.set_width(400.0_f32.min(ctx.content_rect().width() - 48.0));
            ui.label(RichText::new("Connect over Wi-Fi").font(semibold(16.0)));
            ui.add_space(4.0);
            self.wifi(ui, t);
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.button("Close").clicked()
            })
            .inner
        });
        if modal.inner || modal.should_close() {
            self.wifi_open = false;
        }
    }

    fn wifi(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let muted = |s: &str| RichText::new(s).color(t.muted);
        ui.label(muted("The phone and this computer must be on the same Wi-Fi. On the phone, open Settings, Developer options, Wireless debugging."));
        ui.add_space(12.0);
        ui.label(RichText::new("1. Pair (first time only)").font(semibold(14.0)));
        ui.label(muted(
            "Tap Pair device with pairing code, then type what it shows.",
        ));
        let mut fields = |ui: &mut egui::Ui| {
            ui.vertical(|ui| {
                let label = ui.label(muted("IP address and port"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.pair_addr)
                        .hint_text("192.168.1.5:37145")
                        .desired_width(170.0),
                )
                .labelled_by(label.id);
            });
            ui.vertical(|ui| {
                let label = ui.label(muted("Pairing code"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.pair_code)
                        .hint_text("123456")
                        .desired_width(90.0),
                )
                .labelled_by(label.id);
            });
        };
        // Side by side only when both fit; squeezed, the code's label wrapped and its box sat lower.
        if ui.available_width() >= 280.0 {
            ui.horizontal(fields);
        } else {
            fields(ui);
        }
        let working = self.wifi_working;
        if ui
            .add_enabled(!working, egui::Button::new("Pair"))
            .clicked()
        {
            let addr = self.pair_addr.trim().to_string();
            let code = self.pair_code.trim().to_string();
            if valid_addr(&addr) && code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {
                self.run_wifi(ui.ctx(), vec!["pair".into(), addr, code]);
            } else {
                self.wifi_msg = Some(Err(
                    "Type the address with its port, like 192.168.1.5:37145, and the 6-digit code."
                        .into(),
                ));
            }
        }
        ui.add_space(12.0);
        ui.label(RichText::new("2. Connect").font(semibold(14.0)));
        ui.label(muted(
            "Type the IP address and port shown at the top of the Wireless debugging screen.",
        ));
        ui.add(
            egui::TextEdit::singleline(&mut self.connect_addr)
                .hint_text("192.168.1.5:41235")
                .desired_width(f32::INFINITY),
        )
        .accessible_name("IP address and port to connect to");
        if ui
            .add_enabled(!working, egui::Button::new("Connect"))
            .clicked()
        {
            let addr = self.connect_addr.trim().to_string();
            if valid_addr(&addr) {
                self.run_wifi(ui.ctx(), vec!["connect".into(), addr]);
            } else {
                self.wifi_msg = Some(Err(
                    "Type the address with its port, like 192.168.1.5:41235.".into(),
                ));
            }
        }
        match &self.wifi_msg {
            _ if working => {
                ui.label("Working…");
            }
            Some(Ok(m)) => {
                ui.label(m);
            }
            Some(Err(m)) => {
                ui.label(RichText::new(m).color(t.error));
            }
            None => {}
        }
    }

    /// Right side: Back up or Restore, each with its one main button.
    fn tasks(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        ui.add_enabled_ui(!busy, |ui| {
            // One segmented strip, so the two tabs do not read as two more big buttons.
            egui::Frame::new()
                .fill(t.field)
                .stroke(Stroke::new(1.0, t.line))
                .corner_radius(6)
                .inner_margin(3)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (tab, label) in [(Tab::Backup, "Back up"), (Tab::Restore, "Restore")] {
                            let on = self.tab == tab;
                            let button = egui::Button::new(
                                RichText::new(label).font(semibold(14.0)).color(if on {
                                    t.on_accent
                                } else {
                                    t.ink
                                }),
                            )
                            .fill(if on { t.accent } else { Color32::TRANSPARENT })
                            // No .stroke(): it would replace the hover and focus edges.
                            .frame_when_inactive(on)
                            .selected(on)
                            .min_size(vec2(112.0, 30.0));
                            if ui.add(button).clicked() && !on {
                                self.tab = tab;
                                if tab == Tab::Restore {
                                    let dest = self.dest.clone();
                                    self.scan(&dest);
                                }
                            }
                        }
                    });
                });
        });
        ui.add_space(8.0);
        match self.tab {
            Tab::Backup => self.backup_tab(ui, t),
            Tab::Restore => self.restore_tab(ui, t),
        }
    }

    /// Progress of the running job, with Stop.
    fn progress(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        ui.add_space(12.0);
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
        if !self.now.is_empty() {
            ui.add(egui::Label::new(RichText::new(&self.now).color(t.muted)).truncate());
        }
        ui.label(RichText::new("Keep the phone plugged in and unlocked.").color(t.muted));
        let stopping = backup::STOP.load(Ordering::Relaxed);
        if ui
            .add_enabled(
                !stopping,
                egui::Button::new(if stopping { "Stopping…" } else { "Stop" }),
            )
            .clicked()
        {
            backup::STOP.store(true, Ordering::Relaxed);
        }
    }

    fn backup_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let (adb, mtp) = self.connected();
        let ready = !adb.is_empty() || mtp.is_some();
        // File transfer mode can only reach files.
        let mtp_only = adb.is_empty() && mtp.is_some();

        section(ui, t, "What to back up", true);
        ui.add_enabled_ui(!busy, |ui| {
            choice(
                ui,
                t,
                true,
                &mut self.pick.files,
                "Photos, videos and files",
                "Everything in the phone's storage and SD card.",
            );
            choice(
                ui,
                t,
                !mtp_only,
                &mut self.pick.apps,
                "Apps",
                "The apps you installed, without their logins or data.",
            );
            choice(
                ui,
                t,
                !mtp_only,
                &mut self.pick.data,
                "Contacts, call history and messages",
                "Some phones do not allow this over USB.",
            );
        });
        let picked = self.pick.files || (!mtp_only && (self.pick.apps || self.pick.data));

        section(ui, t, "Save to", false);
        ui.horizontal(|ui| {
            // The path sits in a read-only box with Change right beside it, not across the pane.
            let width = (ui.available_width() - 100.0).clamp(80.0, 420.0);
            egui::Frame::new()
                .fill(t.field)
                .stroke(Stroke::new(1.0, t.line))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(width - 22.0);
                    let full = self.dest.display().to_string();
                    ui.add(egui::Label::new(short_path(ui, &full)).truncate())
                        .on_hover_text(&full);
                });
            if ui
                .add_enabled(!busy, egui::Button::new("Change…"))
                .clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_directory(&self.dest)
                    .pick_folder()
            {
                self.dest = dir;
            }
        });

        ui.add_space(24.0);
        let label = if busy && !self.restoring {
            "Backing up…"
        } else if self.result.is_some() {
            "Back up again"
        } else {
            "Back up now"
        };
        if primary(ui, t, label, ready && !busy && picked) {
            self.start(ui.ctx());
        }
        if !busy {
            if !ready {
                ui.label(muted("Connect a phone to start."));
            } else if !picked {
                ui.label(muted("Tick at least one thing to back up."));
            }
        }
        if busy && !self.restoring {
            self.progress(ui, t);
        }

        // What happened.
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
                                RichText::new(format!("{} could not be copied. Press Back up again to retry. The list is in errors.log.", files(s.errors.len())))
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
                        if ui.button("Open backup folder").clicked() {
                            let _ = Command::new("explorer").arg(&s.root).spawn();
                        }
                    });
                }
            }
            None => {}
        }
    }

    fn restore_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let phone = self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
            _ => None,
        });

        section(ui, t, "Choose a backup", true);
        if self.backups.is_empty() {
            ui.label(format!("No backups in {} yet.", self.scanned.display()));
            ui.label(muted(
                "Back up a phone first, or find the folder that holds your backups.",
            ));
        }
        ui.add_enabled_ui(!busy, |ui| {
            // A long list scrolls in place, so Restore stays in view.
            egui::ScrollArea::vertical()
                .id_salt("backups")
                .max_height(264.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for b in &self.backups {
                        // Two phones of the same model: tell them apart by the end of the ID in "<model>_<id>".
                        let twin = self.backups.iter().filter(|o| o.name == b.name).count() > 1;
                        let id = b
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .and_then(|n| n.rsplit_once('_'))
                            .map(|(_, id)| id)
                            .filter(|_| twin);
                        let label = match id {
                            Some(id) => format!(
                                "{} (ID {})",
                                b.name,
                                id.char_indices().rev().nth(3).map_or(id, |(i, _)| &id[i..])
                            ),
                            None => b.name.clone(),
                        };
                        let above = ui.radio(
                            self.chosen.as_ref() == Some(&b.path),
                            RichText::new(&label).font(semibold(14.0)),
                        );
                        if above.clicked() {
                            self.chosen = Some(b.path.clone());
                        }
                        let holds = match (b.files, b.apps) {
                            (true, true) => "Photos, files and apps",
                            (true, false) => "Photos and files",
                            _ => "Apps",
                        };
                        note_under(
                            ui,
                            t,
                            &format!("{holds}, backed up {}.", days_ago(b.age_days)),
                            true,
                            &above,
                            &label,
                            semibold(14.0),
                        );
                    }
                });
            ui.add_space(8.0);
            if ui.button("Find another backup").clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_directory(&self.dest)
                    .pick_folder()
            {
                self.chosen = Some(dir.clone());
                self.scan(&dir);
            }
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
        section(ui, t, "What to restore", false);
        ui.add_enabled_ui(!busy, |ui| {
            choice(
                ui,
                t,
                has_files,
                &mut self.restore_files,
                "Photos, videos and files",
                "Files the phone already has are skipped, never replaced.",
            );
            if has_files && self.restore_files {
                ui.horizontal(|ui| {
                    ui.add_space(ui.spacing().icon_width + ui.spacing().icon_spacing);
                    let says = match only {
                        Some((_, chosen, total)) => {
                            format!("{} of {} chosen.", count(chosen.len()), files(*total))
                        }
                        None => "All files.".into(),
                    };
                    ui.label(RichText::new(says).color(t.muted).size(12.0));
                    pick = ui.button("Choose files…").clicked();
                });
            }
            choice(
                ui,
                t,
                has_apps,
                &mut self.restore_apps,
                "Apps",
                "Reinstalls missing apps. Their logins and data do not come back.",
            );
        });
        let only = only.map(|(_, chosen, _)| chosen.clone());
        let files_on =
            self.restore_files && has_files && only.as_ref().is_none_or(|o| !o.is_empty());
        let picked = files_on || (self.restore_apps && has_apps);
        let path = chosen.map(|b| b.path.clone());
        if pick && let Some(path) = &path {
            self.open_picker(ui.ctx(), path.clone());
        }

        ui.add_space(24.0);
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
        let preview = self
            .preview
            .as_ref()
            .filter(|(p, _)| Some(p) == plan.as_ref())
            .map(|(_, r)| r.clone());
        let checked = matches!(preview, Some(Ok(_)));
        let label = match (busy && self.restoring, self.checking.is_some(), checked) {
            (true, true, _) => "Checking…",
            (true, false, _) => "Restoring…",
            (false, _, true) => "Restore now",
            (false, _, false) => "Check restore",
        };
        if primary(ui, t, label, plan.is_some() && !busy)
            && let Some(plan) = plan
        {
            self.restore(ui.ctx(), plan, !checked);
        }
        if !busy {
            if phone.is_none() {
                ui.label(muted(
                    "Connect the phone with USB debugging on to restore to it.",
                ));
            } else if path.is_none() {
                ui.label(muted("Choose a backup to restore."));
            } else if !picked {
                ui.label(muted("Tick at least one thing to restore."));
            } else if let Some((_, model)) = &phone {
                // Named here, not on the button, where a long model wrapped the label at narrow widths.
                ui.label(muted(&format!("Restores to {model}.")));
            }
        }
        if busy && self.restoring {
            self.progress(ui, t);
        }
        if let Some(r) = preview.filter(|_| !busy) {
            ui.add_space(16.0);
            result_box(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (title, color, text) = match &r {
                    Ok(m) => ("What will happen", t.ink, m),
                    Err(e) => ("Cannot restore", t.error, e),
                };
                ui.label(RichText::new(title).font(semibold(16.0)).color(color));
                ui.label(text.as_str());
            });
        }
        match &self.restore_msg {
            Some(Ok(m)) => {
                ui.add_space(16.0);
                result_box(t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Restore finished").font(semibold(16.0)));
                    ui.label(m);
                });
            }
            Some(Err(e)) => {
                ui.add_space(16.0);
                result_box(t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("Restore stopped")
                            .font(semibold(16.0))
                            .color(t.error),
                    );
                    ui.label(e);
                });
            }
            None => {}
        }
    }
}

impl App {
    fn load_fixture(&mut self, f: Fixture) {
        self.fixture = f;
        let saved = |name: &str, files: bool, apps: bool, age: Option<u64>| backup::Saved {
            path: PathBuf::from(format!(
                r"C:\fixture\{name}_R5CT{:04X}",
                age.unwrap_or(7) * 31
            )),
            name: name.into(),
            files,
            apps,
            age_days: age,
        };
        let adb = |model: &str, wifi: bool| Phone::Adb {
            serial: format!("{model}-serial"),
            model: model.into(),
            wifi,
        };
        self.restore_msg = None;
        self.result = None;
        self.wifi_msg = None;
        match f {
            Fixture::Real => {
                self.phones = None;
                self.backups.clear();
                let dest = self.dest.clone();
                self.scan(&dest);
            }
            Fixture::Worst => {
                self.phones = Some(vec![adb("Redmi Note 13 Pro+ 5G", true)]);
                self.dest = PathBuf::from(
                    r"C:\Users\Aleksandra Wiśniewska-Kowalczyk\OneDrive - Northwind Industries Holdings\Documents\Phone backups",
                );
                self.backups = vec![
                    saved("Redmi Note 13 Pro+ 5G", true, true, Some(0)),
                    // A second phone of the same model: same name, different ID.
                    saved("Redmi Note 13 Pro+ 5G", true, false, Some(1284)),
                    // A folder the user picked by hand, not named "<model>_<id>".
                    saved(
                        "Backup from old phone before factory reset (Aleksandra)",
                        true,
                        true,
                        None,
                    ),
                    saved("Jo", false, true, Some(1)),
                ];
                self.restore_msg = Some(Err("Copied 1 file back to the phone. 1,284 files were already there. 3 files could not be copied, for example /sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4, /sdcard/DCIM/Camera/PXL_20240817_183455123.NIGHT.jpg, /sdcard/Download/bartholomew.fitzgerald@northwind-industries-holdings.example.com_invoice.pdf. Restore again to retry.\nThe phone was disconnected. Files copied so far stay on the phone; plug it back in and restore again to continue.".into()));
                self.result = Some(Ok(vec![Summary {
                    name: "Redmi Note 13 Pro+ 5G".into(),
                    root: self.dest.clone(),
                    files: 128_456,
                    copied: 1,
                    errors: vec!["x".into()],
                    data: vec![
                        ("Contacts", true),
                        ("Call history", false),
                        ("Messages", false),
                    ],
                    full: true,
                    pick: Pick {
                        files: true,
                        apps: true,
                        data: true,
                    },
                }]));
                self.wifi_open = true;
                self.wifi_msg = Some(backup::wifi_result(
                    "connect",
                    "failed to connect to 'aleksandras-redmi-note-13-pro-plus.northwind.local:41235': No connection could be made because the target machine actively refused it. (10061)",
                ));
                self.connect_addr =
                    "aleksandras-redmi-note-13-pro-plus.northwind.local:41235".into();
            }
            Fixture::Empty => {
                self.phones = Some(Vec::new());
                self.backups.clear();
            }
            Fixture::One => {
                self.phones = Some(vec![Phone::Mtp { name: "Jo".into() }]);
                self.backups = vec![saved("Jo", true, false, Some(1))];
            }
            Fixture::Many => {
                self.phones = Some(vec![
                    adb("Pixel 8", false),
                    adb("Galaxy S23 Ultra", false),
                    adb("Redmi Note 13 Pro+ 5G", true),
                ]);
                self.backups = (0..40)
                    .map(|i| saved(&format!("Pixel {}", i % 9), true, i % 3 == 0, Some(i * 17)))
                    .collect();
            }
        }
        self.chosen = self.backups.first().map(|b| b.path.clone());
        let first_adb = self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, .. } => Some(serial.clone()),
            _ => None,
        });
        self.preview = first_adb.zip(self.chosen.clone()).map(|(serial, path)| {
            let plan = (serial, path, true, true, None);
            (plan, Ok("Files: will copy 128,456 files (214.7 GB) to the phone. 1,284 files already on the phone will be skipped; nothing is replaced. This phone has no SD card named 3A1F-0C2B or 9C33-6BD5, so those files cannot be restored.\nApps: will install 212 of 214, for example com.northwindindustries.holdings.expensereports.enterprise, com.whatsapp, org.mozilla.firefox. The rest are already on the phone.".into()))
        });
        if f != Fixture::Real {
            self.scanned = self.dest.clone();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
                        self.status = format!("Copying {} of {}…", count(done), files(total));
                        self.fraction = Some(done as f32 / total as f32);
                    } else {
                        self.status = format!("{} copied so far", files(done));
                        self.fraction = None;
                    }
                }
                Msg::Wifi(r) => {
                    self.wifi_working = false;
                    self.wifi_msg = Some(r);
                }
                Msg::Restored(r) => match self.checking.take() {
                    Some(plan) => self.preview = Some((plan, r)),
                    None => self.restore_msg = Some(r),
                },
                Msg::Done(r) => {
                    self.result = Some(r);
                    // Flash the taskbar button if the window is in the background.
                    if !ui.ctx().input(|i| i.focused) {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                                egui::UserAttentionType::Informational,
                            ));
                    }
                }
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
                ui.set_max_width(320.0);
                if self.restoring {
                    ui.label(RichText::new("Stop restoring?").font(semibold(16.0)));
                    ui.label("Everything restored so far stays on the phone.");
                } else {
                    ui.label(RichText::new("Stop the backup?").font(semibold(16.0)));
                    ui.label("Files copied so far are kept. The next backup continues where this one stopped.");
                }
                ui.horizontal(|ui| {
                    if ui.button("Stop and close").clicked() {
                        backup::STOP.store(true, Ordering::Relaxed);
                        self.closing = true;
                        self.confirm_close = false;
                    }
                    if ui.button(if self.restoring { "Keep restoring" } else { "Keep backing up" }).clicked() {
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
        egui::Frame::central_panel(ui.style())
            .fill(t.bg)
            .inner_margin(28)
            .show(ui, |ui| {
                let mut area = egui::ScrollArea::vertical().auto_shrink(false);
                // Held every frame: the first frames do not know the content height yet.
                if let Some(y) = self.scroll {
                    area = area.vertical_scroll_offset(y);
                }
                area.show(ui, |ui| {
                    // At most 888 px (phone 300 + gap 48 + tasks 540), centred when the window is wider.
                    let side = ((ui.available_width() - 888.0) / 2.0).max(0.0);
                    let width = ui.available_width().min(888.0);
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
        storage.set_string("pair_addr", self.pair_addr.clone());
        storage.set_string("connect_addr", self.connect_addr.clone());
        storage.set_string(
            "pick",
            [
                bit(self.pick.files),
                bit(self.pick.apps),
                bit(self.pick.data),
            ]
            .iter()
            .collect(),
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
    fn checks_wifi_addresses() {
        assert!(super::valid_addr("192.168.1.5:37145"));
        assert!(super::valid_addr("phone.local:5555"));
        assert!(!super::valid_addr("192.168.1.5:"));
        assert!(!super::valid_addr(":5555"));
        assert!(!super::valid_addr("192.168.1.5"));
        assert!(!super::valid_addr("192.168.1.5:99999"));
        assert!(!super::valid_addr("192.168.1.5:0"));
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

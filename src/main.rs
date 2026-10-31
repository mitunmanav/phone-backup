// No console window behind the app on Windows.
#![windows_subsystem = "windows"]

mod backup;

use backup::{Msg, Phone, Pick, Summary, count, files, or_list, size};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, Rect, RichText, Stroke, StrokeKind,
    TextStyle, vec2,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use std::{env, fs, thread};

/// Windows 11 colours: the window, a lighter page beside the page list, lighter panels on it.
struct Tokens {
    /// The window: the page list and the command bar sit on it.
    bg: Color32,
    /// The page beside the page list: one step lighter than `bg`.
    layer: Color32,
    /// Panels on the page: one step lighter again.
    surface: Color32,
    field: Color32,
    line: Color32,
    /// Edges of fields, tick boxes and buttons: at least 3:1 against `surface` and `field`.
    border: Color32,
    /// Hovered and current rows of the page list.
    hover: Color32,
    ink: Color32,
    muted: Color32,
    /// Fill of the main button and progress, and links: at least 4.5:1 on every surface.
    accent: Color32,
    on_accent: Color32,
    /// Behind what a finished job says.
    info: Color32,
    error: Color32,
}

fn tokens(dark: bool) -> Tokens {
    let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if dark {
        Tokens {
            bg: hex(0x202020),
            layer: hex(0x272727),
            surface: hex(0x2E2E2E),
            field: hex(0x252525),
            line: hex(0x3C3C3C),
            border: hex(0x8A8A8A),
            hover: hex(0x363636),
            ink: hex(0xFFFFFF),
            muted: hex(0xB0B0B0),
            accent: hex(0x60CDFF),
            on_accent: hex(0x00131C),
            info: hex(0x1F3140),
            error: hex(0xFF99A4),
        }
    } else {
        Tokens {
            bg: hex(0xF3F3F3),
            layer: hex(0xFBFBFB),
            surface: hex(0xFFFFFF),
            field: hex(0xF9F9F9),
            line: hex(0xE0E0E0),
            border: hex(0x8A8A8A),
            hover: hex(0xEDEDED),
            ink: hex(0x1A1A1A),
            muted: hex(0x5E5E5E),
            accent: hex(0x0067C0),
            on_accent: hex(0xFFFFFF),
            info: hex(0xEAF2FB),
            error: hex(0xC42B1C),
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
    v.window_fill = t.layer;
    v.extreme_bg_color = t.field;
    v.override_text_color = Some(t.ink);
    v.hyperlink_color = t.accent;
    v.selection.bg_fill = t.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(2.0, t.accent);
    v.widgets.inactive.weak_bg_fill = t.surface;
    v.widgets.inactive.bg_fill = t.field;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.accent);
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

/// A titled panel: the title, with quiet `aside` text at its right, a hairline, then the body.
fn panel<R>(
    ui: &mut egui::Ui,
    t: &Tokens,
    title: &str,
    aside: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.line))
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let gap = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).font(semibold(14.0)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(aside).color(t.muted))
                        });
                    });
                });
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().top(),
                Stroke::new(1.0, t.line),
            );
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 14))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = gap;
                    ui.set_width(ui.available_width());
                    add(ui)
                })
                .inner
        })
        .inner
}

/// What a finished job says, kept together in one quiet tinted box.
fn result_box(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.info)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(16, 12))
}

/// One page in the page list: a hover fill, and on the current page a fill, semibold text and an
/// accent pill, left of the row in the side list and under it in the narrow top strip.
fn nav_item(ui: &mut egui::Ui, t: &Tokens, label: &str, on: bool, side: bool) -> bool {
    let font = if on {
        semibold(14.0)
    } else {
        FontId::proportional(14.0)
    };
    let color = if ui.is_enabled() { t.ink } else { t.muted };
    let galley = ui.painter().layout_no_wrap(label.into(), font, color);
    let width = if side {
        ui.available_width()
    } else {
        galley.size().x + 24.0
    };
    let (rect, r) = ui.allocate_exact_size(vec2(width, 34.0), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), on, label));
    if on || r.hovered() {
        ui.painter().rect_filled(rect, 4, t.hover);
    }
    if on {
        let pill = if side {
            Rect::from_min_size(rect.min + vec2(0.0, 9.0), vec2(3.0, 16.0))
        } else {
            Rect::from_center_size(
                egui::pos2(rect.center().x, rect.max.y - 1.5),
                vec2(16.0, 3.0),
            )
        };
        ui.painter().rect_filled(pill, 2, t.accent);
    }
    let text_at = egui::pos2(rect.min.x + 12.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_at, galley, color);
    // egui draws no focus for a hand-made widget.
    if r.has_focus() {
        ui.painter()
            .rect_stroke(rect, 4, Stroke::new(2.0, t.ink), StrokeKind::Inside);
    }
    r.clicked()
}

/// What the bar at the foot of the Back up and Restore pages says and offers.
struct Cmd {
    title: String,
    note: &'static str,
    label: &'static str,
    enabled: bool,
    /// Restore only: what the main button runs, and whether it only checks.
    plan: Option<(Plan, bool)>,
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
    Settings,
}

/// The main action of a page: accent when it can run, a quiet field otherwise.
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
        ui.spacing_mut().button_padding.x = 16.0;
        ui.add(button.min_size(vec2(120.0, 32.0))).clicked()
    })
    .inner
}

/// A checkbox with a one-line explanation under its label, and an optional link after the
/// explanation. Returns true when the link is clicked.
fn choice(
    ui: &mut egui::Ui,
    t: &Tokens,
    enabled: bool,
    value: &mut bool,
    label: &str,
    note: &str,
    link: Option<&str>,
) -> bool {
    // A box that cannot be used shows empty, so a grey tick does not read as "will be included".
    let mut off = false;
    let value = if enabled { value } else { &mut off };
    // No add_enabled_ui scope here: each scope adds its own gap, so rows would sit unevenly.
    let above = ui.add_enabled(enabled, egui::Checkbox::new(value, label));
    let indent = ui.spacing().icon_width + ui.spacing().icon_spacing;
    // The control is at least 32 px tall for an easy click, with its label centred in it;
    // pull the note up into the space under the label, so a label that wraps still clears it.
    let label_h = ui
        .painter()
        .layout(
            label.into(),
            TextStyle::Body.resolve(ui.style()),
            Color32::PLACEHOLDER,
            ui.available_width() - indent,
        )
        .size()
        .y;
    ui.add_space(-((above.rect.height() - label_h) / 2.0 + 4.0).max(0.0));
    // A text row, not a control: without this the row would be 32 px tall.
    let control_h = std::mem::replace(&mut ui.spacing_mut().interact_size.y, 18.0);
    let clicked = ui
        .horizontal_top(|ui| {
            ui.add_space(indent);
            // Wrapped lines start under the label, not under the box.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if !enabled {
                    ui.disable();
                }
                ui.label(RichText::new(note).color(t.muted).size(12.0));
                link.is_some_and(|l| {
                    ui.link(RichText::new(l).size(12.0).color(t.accent))
                        .clicked()
                })
            })
            .inner
        })
        .inner;
    ui.spacing_mut().interact_size.y = control_h;
    ui.add_space(8.0);
    clicked
}

/// A dialog with a title, a body that scrolls when the window is short, and one button that
/// closes it. Returns true when it should close.
fn dialog(
    ctx: &egui::Context,
    id: &'static str,
    title: &str,
    width: f32,
    button: &str,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let modal = egui::Modal::new(egui::Id::unique(id))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            let room = ctx.content_rect();
            // 16 px of window shows on each side of the narrowest window.
            ui.set_width(width.min(room.width() - 72.0));
            ui.label(RichText::new(title).font(semibold(20.0)));
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(room.height() - 160.0)
                .auto_shrink([false, true])
                .show(ui, body);
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.button(button).clicked()
            })
            .inner
        });
    modal.inner || modal.should_close()
}

/// Windows 11 dialogs: 24 px from the edge to the content; egui's popup margin is 6.
fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.global_style()).inner_margin(20)
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
    /// Height of the command bar last frame; its text wraps on narrow windows.
    bar_h: f32,
    /// Asking whether to stop a running backup before closing.
    confirm_close: bool,
    /// Close as soon as the stopped backup has finished cleaning up.
    closing: bool,
    tab: Tab,
    /// Play the Windows sound when a backup or restore ends.
    sound: bool,
    /// The report of the last restore, for its Open report button.
    restore_report: Option<PathBuf>,
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
    /// Debug builds only: save a picture of the window here, then close.
    shot: Option<PathBuf>,
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
            bar_h: 64.0,
            confirm_close: false,
            closing: false,
            tab: Tab::Backup,
            sound: cc.storage.and_then(|s| s.get_string("sound")).as_deref() == Some("1"),
            restore_report: None,
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
            shot: None,
        };
        // PB_FIXTURE="worst|empty|one|many [restore] [settings] [light|dark] [wifi] [busy]
        // [zoom=<factor>] [scroll=<px>] [pick=<backup folder>] [size=<w>x<h>] [shot=<file.png>]" opens
        // straight on test data; wifi opens its dialog; busy shows a job running; pick= opens the file picker on a real folder; shot= saves a
        // picture of the window and closes.
        if cfg!(debug_assertions) {
            let spec = env::var("PB_FIXTURE").unwrap_or_default();
            for word in spec.split_whitespace() {
                match word {
                    "worst" => app.load_fixture(Fixture::Worst),
                    "empty" => app.load_fixture(Fixture::Empty),
                    "one" => app.load_fixture(Fixture::One),
                    "many" => app.load_fixture(Fixture::Many),
                    "restore" => app.tab = Tab::Restore,
                    "settings" => app.tab = Tab::Settings,
                    "light" => ctx.set_theme(egui::Theme::Light),
                    "dark" => ctx.set_theme(egui::Theme::Dark),
                    "wifi" => app.wifi_open = true,
                    "busy" => {
                        app.busy.store(true, Ordering::Relaxed);
                        app.status = "Copying 48,213 of 128,456 files…".into();
                        app.now = r"/sdcard/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4".into();
                        app.fraction = Some(0.375);
                    }
                    _ if word.starts_with("zoom=") => {
                        ctx.set_zoom_factor(word[5..].parse().unwrap_or(1.0))
                    }
                    _ if word.starts_with("pick=") => app.open_picker(ctx, word[5..].into()),
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
        let phone = self
            .phones
            .iter()
            .flatten()
            .find_map(|p| match p {
                Phone::Adb {
                    serial: s, model, ..
                } if *s == serial => Some(model.clone()),
                _ => None,
            })
            .unwrap_or_else(|| serial.clone());
        let report_path = (!check).then(|| folder.join(backup::RESTORE_REPORT));
        self.restore_report = report_path.clone();
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
            if let Some(path) = report_path {
                let (Ok(text) | Err(text)) = &result;
                backup::save_report(&path, &format!("Restore to {phone}"), text);
            }
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
        let close = dialog(
            ctx,
            "picker",
            "Choose files to restore",
            520.0,
            "Done",
            |ui| {
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
                        // The room left in the dialog's body, so only the list scrolls, not the body too.
                        let height = ui.available_height().clamp(160.0, 480.0);
                        // Only the rows in view are built, so a backup of 100,000 files stays quick.
                        // Nested in the dialog's scroll area: without the minimum it would shrink.
                        egui::ScrollArea::vertical()
                            .max_height(height)
                            .min_scrolled_height(height.min(40.0 * list.len() as f32))
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
                                                    ui.spacing().icon_width
                                                        + ui.spacing().icon_spacing,
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
            },
        );
        if wanted != p.asked {
            let _ = p.ask.send(wanted.clone());
            p.asked = wanted;
        }
        // Esc or a click outside keeps the choice too: nothing has changed on the phone yet.
        if close
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

    /// The phone beside what to do with it (stacked when narrow), then progress and results.
    /// Returns what the command bar offers; Settings has none.
    fn content(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Option<Cmd> {
        if self.tab == Tab::Settings {
            self.settings_page(ui, t);
            return None;
        }
        let job = |app: &mut Self, ui: &mut egui::Ui| match app.tab {
            Tab::Backup => app.backup_tab(ui, t),
            _ => app.restore_tab(ui, t),
        };
        let cmd = if ui.available_width() >= 640.0 {
            ui.spacing_mut().item_spacing.x = 16.0;
            let cmd = ui.columns_const(|[left, right]| {
                // egui's columns are justified: wrapped text got stretched word gaps and buttons
                // ran full width, unlike the narrow layout.
                let plain = Layout::top_down(Align::Min);
                left.spacing_mut().item_spacing.x = 8.0;
                right.spacing_mut().item_spacing.x = 8.0;
                left.with_layout(plain, |ui| {
                    panel(ui, t, "Phone", "", |ui| self.device(ui, t))
                });
                right.with_layout(plain, |ui| job(self, ui)).inner
            });
            ui.spacing_mut().item_spacing.x = 8.0;
            cmd
        } else {
            panel(ui, t, "Phone", "", |ui| self.device(ui, t));
            ui.add_space(8.0);
            job(self, ui)
        };
        if self.busy.load(Ordering::Relaxed) {
            ui.add_space(8.0);
            panel(ui, t, "Progress", "", |ui| self.progress(ui, t));
        }
        match self.tab {
            Tab::Backup => self.backup_result(ui, t),
            _ => self.restore_result(ui, t, cmd.plan.as_ref().map(|p| &p.0)),
        }
        Some(cmd)
    }

    /// The page's title, its scrolling body and, on Back up and Restore, the command bar.
    fn page(&mut self, ui: &mut egui::Ui, t: &Tokens, side: bool, pad: i8) {
        let title = match self.tab {
            Tab::Backup => "Back up your phone",
            Tab::Restore => "Restore a backup",
            Tab::Settings => "Settings",
        };
        let head = egui::Frame::new()
            .inner_margin(egui::Margin {
                left: pad,
                right: pad,
                top: if side { 18 } else { 8 },
                bottom: 14,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !side {
                    self.nav(ui, t, false);
                    ui.add_space(4.0);
                }
                ui.label(RichText::new(title).font(semibold(22.0)));
            })
            .response
            .rect;
        let all = ui.max_rect();
        ui.painter()
            .hline(all.x_range(), head.bottom(), Stroke::new(1.0, t.line));
        let rest = Rect::from_x_y_ranges(all.x_range(), head.bottom()..=all.bottom());
        let bar = if self.tab == Tab::Settings {
            0.0
        } else {
            self.bar_h
        };
        let body = rest.with_max_y(rest.bottom() - bar);
        let cmd = ui
            .scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                let mut area = egui::ScrollArea::vertical().auto_shrink(false);
                // Held every frame: the first frames do not know the content height yet.
                if let Some(y) = self.scroll {
                    area = area.vertical_scroll_offset(y);
                }
                area.show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(pad, 16))
                        .show(ui, |ui| {
                            ui.set_max_width(ui.available_width().min(1040.0));
                            self.content(ui, t)
                        })
                        .inner
                })
                .inner
            })
            .inner;
        if let Some(cmd) = cmd {
            let bar = rest.with_min_y(rest.bottom() - self.bar_h);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                self.command_bar(ui, t, cmd, pad)
            });
        }
    }

    /// The page list: down the left side, or a strip over the page on narrow windows.
    fn nav(&mut self, ui: &mut egui::Ui, t: &Tokens, side: bool) {
        // The page cannot change while a job runs: its progress and Stop are on it.
        ui.add_enabled_ui(!self.busy.load(Ordering::Relaxed), |ui| {
            let item = |app: &mut Self, ui: &mut egui::Ui, tab: Tab, label: &str| {
                if nav_item(ui, t, label, app.tab == tab, side) && app.tab != tab {
                    app.tab = tab;
                    if tab == Tab::Restore {
                        let dest = app.dest.clone();
                        app.scan(&dest);
                    }
                }
            };
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if side {
                item(self, ui, Tab::Backup, "Back up");
                item(self, ui, Tab::Restore, "Restore");
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    item(self, ui, Tab::Settings, "Settings")
                });
            } else {
                ui.horizontal(|ui| {
                    item(self, ui, Tab::Backup, "Back up");
                    item(self, ui, Tab::Restore, "Restore");
                    item(self, ui, Tab::Settings, "Settings");
                });
            }
        });
    }

    /// The foot of the page: what will happen on the left, the main button (Stop while a job
    /// runs) on the right.
    fn command_bar(&mut self, ui: &mut egui::Ui, t: &Tokens, cmd: Cmd, pad: i8) {
        let shown = egui::Frame::new()
            .fill(t.bg)
            .inner_margin(egui::Margin::symmetric(pad, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.busy.load(Ordering::Relaxed) {
                        let stopping = backup::STOP.load(Ordering::Relaxed);
                        let stop = egui::Button::new(if stopping { "Stopping…" } else { "Stop" })
                            .min_size(vec2(120.0, 32.0));
                        if ui.add_enabled(!stopping, stop).clicked() {
                            backup::STOP.store(true, Ordering::Relaxed);
                        }
                    } else if primary(ui, t, cmd.label, cmd.enabled) {
                        match cmd.plan {
                            Some((plan, check)) => self.restore(ui.ctx(), plan, check),
                            None => self.start(ui.ctx()),
                        }
                    }
                    ui.add_space(4.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add(
                            egui::Label::new(RichText::new(&cmd.title).font(semibold(14.0))).wrap(),
                        );
                        if !cmd.note.is_empty() {
                            ui.add(
                                egui::Label::new(RichText::new(cmd.note).color(t.muted).size(12.0))
                                    .wrap(),
                            );
                        }
                    });
                });
            });
        ui.painter().hline(
            shown.response.rect.x_range(),
            shown.response.rect.top(),
            Stroke::new(1.0, t.line),
        );
        // The page saves room for this height; one more frame settles a change.
        let h = shown.response.rect.height();
        if (h - self.bar_h).abs() > 0.5 {
            self.bar_h = h;
            ui.ctx().request_repaint();
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

    /// The phone, how it is connected, and how to connect one.
    fn device(&mut self, ui: &mut egui::Ui, t: &Tokens) {
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
                    adb_missing(ui, t);
                }
            }
            (Some(_), [], None) if allow => {
                heading(ui, "Tap Allow on your phone");
                ui.label(
                    "Unlock the phone. It asks whether to allow USB debugging from this computer.",
                );
            }
            (Some(_), [], None) => {
                heading(ui, "Connect your phone");
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
                                    .color(t.accent),
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

        // Wi-Fi is a one-time setup, so it opens in a dialog instead of stretching this card.
        ui.add_space(12.0);
        if ui
            .add_enabled(!busy, egui::Button::new("Connect over Wi-Fi…"))
            .clicked()
        {
            self.wifi_open = true;
        }
    }

    fn wifi_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        if dialog(ctx, "wifi", "Connect over Wi-Fi", 400.0, "Close", |ui| {
            self.wifi(ui, t)
        }) {
            self.wifi_open = false;
        }
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        // egui sets this width outright, even past the window, so it is capped by the room left.
        ui.set_max_width(ui.available_width().min(640.0));
        panel(ui, t, "Backup folder", "", |ui| {
            ui.horizontal(|ui| {
                // The path sits in a read-only box with Change right beside it.
                let width = ui.available_width() - 100.0;
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
                    self.dest = dir.clone();
                    self.scan(&dir);
                }
            });
            ui.label(muted(
                "New backups go here. Restore lists the backups in it.",
            ));
        });
        ui.add_space(8.0);
        panel(ui, t, "Theme", "", |ui| {
            let mut theme = ui.ctx().options(|o| o.theme_preference);
            for (value, label) in [
                (egui::ThemePreference::System, "Follow Windows"),
                (egui::ThemePreference::Light, "Light"),
                (egui::ThemePreference::Dark, "Dark"),
            ] {
                ui.radio_value(&mut theme, value, label);
            }
            // egui saves this with the window size, so it is remembered.
            ui.ctx().set_theme(theme);
        });
        ui.add_space(8.0);
        panel(ui, t, "When a backup or restore ends", "", |ui| {
            ui.checkbox(&mut self.sound, "Play a sound");
            ui.label(muted(
                "The taskbar button flashes when the window is in the background.",
            ));
        });
        ui.add_space(8.0);
        panel(ui, t, "Help", "", |ui| {
            if ui.button("Save diagnostics…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_file_name("phone-backup-diagnostics.txt")
                    .save_file()
            {
                let _ = fs::write(&path, backup::diagnostics(&self.dest));
                let _ = Command::new("explorer").arg(&path).spawn();
            }
            ui.label(muted(
                "Saves the app, Windows and adb versions and the phones adb sees to a text file. Read it before you share it.",
            ));
        });
        ui.add_space(8.0);
        panel(ui, t, "About", "", |ui| {
            ui.label(format!("Phone Backup {}", env!("CARGO_PKG_VERSION")));
            ui.label(muted(
                "Free software under the GNU General Public License, version 3 or later.",
            ));
            ui.hyperlink_to("Source code on GitHub", env!("CARGO_PKG_REPOSITORY"));
        });
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

    /// Progress of the running job; Stop is in the command bar.
    fn progress(&mut self, ui: &mut egui::Ui, t: &Tokens) {
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
    }

    fn backup_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let (adb, mtp) = self.connected();
        let ready = !adb.is_empty() || mtp.is_some();
        // File transfer mode can only reach files.
        let mtp_only = adb.is_empty() && mtp.is_some();

        panel(ui, t, "What to back up", "", |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                choice(
                    ui,
                    t,
                    true,
                    &mut self.pick.files,
                    "Photos, videos and files",
                    "Everything in the phone's storage and SD card.",
                    None,
                );
                choice(
                    ui,
                    t,
                    !mtp_only,
                    &mut self.pick.apps,
                    "Apps",
                    "The apps you installed, without their logins or data.",
                    None,
                );
                choice(
                    ui,
                    t,
                    !mtp_only,
                    &mut self.pick.data,
                    "Contacts, call history and messages",
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
                    self.tab = Tab::Settings;
                }
            });
        });
        let picked = self.pick.files || (!mtp_only && (self.pick.apps || self.pick.data));
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
                "Keep the phone plugged in and unlocked."
            } else {
                ""
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

    /// What the last backup did.
    fn backup_result(&mut self, ui: &mut egui::Ui, t: &Tokens) {
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

    fn restore_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
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
                                            RichText::new(format!(
                                                "{holds}, backed up {}.",
                                                days_ago(b.age_days)
                                            ))
                                            .color(t.muted)
                                            .size(12.0),
                                        );
                                    });
                                },
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
            self.open_picker(ui.ctx(), path.clone());
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
            _ if busy && self.checking.is_some() => "Checking…".into(),
            _ if busy => "Restoring…".into(),
            None => "Connect the phone with USB debugging on to restore to it".into(),
            _ if path.is_none() => "Choose a backup to restore".into(),
            _ if !picked => "Tick at least one thing to restore".into(),
            Some((_, model)) => format!("Restores to {model}"),
        };
        Cmd {
            title,
            note: if busy {
                "Keep the phone plugged in and unlocked."
            } else if plan.is_some() {
                "Nothing changes on the phone until you press Restore now."
            } else {
                ""
            },
            label: if checked {
                "Restore now"
            } else {
                "Check restore"
            },
            enabled: plan.is_some(),
            plan: plan.map(|p| (p, !checked)),
        }
    }

    /// What the last check found, while the choices still match it.
    fn preview_for(&self, plan: Option<&Plan>) -> Option<&Result<String, String>> {
        self.preview
            .as_ref()
            .filter(|(p, _)| Some(p) == plan)
            .map(|(_, r)| r)
    }

    /// What the last check or restore found.
    fn restore_result(&mut self, ui: &mut egui::Ui, t: &Tokens, plan: Option<&Plan>) {
        if let Some(r) = self
            .preview_for(plan)
            .filter(|_| !self.busy.load(Ordering::Relaxed))
        {
            ui.add_space(16.0);
            result_box(t).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (title, color, text) = match r {
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
                    self.open_restore_report(ui);
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
                    self.open_restore_report(ui);
                });
            }
            None => {}
        }
    }

    fn open_restore_report(&self, ui: &mut egui::Ui) {
        if let Some(path) = &self.restore_report {
            ui.add_space(4.0);
            if ui.button("Open report").clicked() {
                let _ = Command::new("explorer").arg(path).spawn();
            }
        }
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
                    None => {
                        self.restore_msg = Some(r);
                        self.finished(ui.ctx());
                    }
                },
                Msg::Done(r) => {
                    self.result = Some(r);
                    self.finished(ui.ctx());
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
                ui.set_max_width(ui.available_width().min(320.0));
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

        // The page list down the left, or on narrow windows a strip over the page.
        let side = ui.available_width() >= 640.0;
        if side {
            egui::Panel::left("nav")
                .exact_size(200.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(t.bg).inner_margin(8))
                .show(ui, |ui| self.nav(ui, &t, true));
        }
        let pad: i8 = if side { 28 } else { 16 };
        let edge: i8 = if side { 0 } else { -1 };
        egui::Frame::central_panel(ui.style())
            .fill(t.bg)
            .inner_margin(0)
            .show(ui, |ui| {
                // A lighter page with a rounded top-left corner; its right and bottom edges sit
                // just outside the window.
                egui::Frame::new()
                    .fill(t.layer)
                    .stroke(Stroke::new(1.0, t.line))
                    .corner_radius(egui::CornerRadius {
                        nw: if side { 8 } else { 0 },
                        ..Default::default()
                    })
                    .outer_margin(egui::Margin {
                        left: edge,
                        top: edge,
                        right: -1,
                        bottom: -1,
                    })
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        self.page(ui, &t, side, pad);
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

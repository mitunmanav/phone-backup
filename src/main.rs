// No console window behind the app on Windows.
#![windows_subsystem = "windows"]

mod backup;
mod export;
mod tools;

use backup::{FLAGS, KINDS, Msg, Phone, Pick, Summary, Tally, count, files, or_list, size};
use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Layout, Rect, RichText, Stroke, StrokeKind,
    TextStyle, vec2,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
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
    note: String,
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

/// Public steps to turn on USB debugging, per phone maker.
const BRANDS: [(&str, &[&str]); 5] = [
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

/// Numbered steps, each wrapped line lined up under its first.
fn steps(ui: &mut egui::Ui, t: &Tokens, list: &[&str]) {
    // Text rows, not controls: drop the 32 px control height here.
    let control_h = ui.spacing().interact_size.y;
    ui.spacing_mut().interact_size.y = 20.0;
    for (n, step) in list.iter().enumerate() {
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
    ui.spacing_mut().interact_size.y = control_h;
}

/// The QR code with its 4-module quiet border, black on white in both themes so phone cameras
/// read it.
fn qr_image(ui: &mut egui::Ui, qr: &qrcodegen::QrCode) {
    let n = qr.size() + 8;
    let px = (ui.available_width() / n as f32).floor().clamp(2.0, 5.0);
    let (rect, response) = ui.allocate_exact_size(vec2(px, px) * n as f32, egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Image, true, "QR code to scan with the phone")
    });
    let p = ui.painter();
    let origin = rect.min.round();
    p.rect_filled(rect, 0, Color32::WHITE);
    for y in 0..qr.size() {
        for x in 0..qr.size() {
            if qr.get_module(x, y) {
                let at = origin + vec2((x + 4) as f32, (y + 4) as f32) * px;
                p.rect_filled(Rect::from_min_size(at, vec2(px, px)), 0, Color32::BLACK);
            }
        }
    }
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

/// The files in a backup: path, size and time, from `backup::backup_files`.
type Listed = Result<Vec<(PathBuf, u64, i64)>, String>;
/// What `backup::measure` found: every file and its size, and the size of the apps.
type Measured = (Vec<(String, u64)>, u64);

/// What the file picker's worker sends back.
enum Found {
    List(Listed),
    Thumb(PathBuf, Option<egui::ColorImage>),
}

/// The open "Choose files" dialog. Dropping it ends its worker thread.
struct Picker {
    folder: PathBuf,
    /// None while the worker reads the backup.
    files: Option<Listed>,
    /// `backup::flags` of each file.
    flags: Vec<u8>,
    chosen: HashSet<PathBuf>,
    /// None: not a picture this app can show.
    thumbs: HashMap<PathBuf, Option<egui::TextureHandle>>,
    /// The rows last asked for, so the worker hears only of changes.
    asked: Vec<PathBuf>,
    ask: mpsc::Sender<Vec<PathBuf>>,
    rx: mpsc::Receiver<Found>,
    /// Choosing files to delete from the phone, not to restore.
    delete: bool,
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
    Transfer,
    Tools,
    Settings,
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

/// A dialog that asks before doing something: a title, a body that scrolls when the window is
/// short, the button that does it (first, as on Windows) and Cancel. Some(true) for the button,
/// Some(false) for Cancel, Esc or a click outside; None while it stays open.
fn confirm(
    ctx: &egui::Context,
    title: &str,
    yes: &str,
    can: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> Option<bool> {
    let modal = egui::Modal::new(egui::Id::unique("confirm"))
        .frame(dialog_frame(ctx))
        .show(ctx, |ui| {
            let room = ctx.content_rect();
            ui.set_width(440f32.min(room.width() - 72.0));
            ui.add(egui::Label::new(RichText::new(title).font(semibold(20.0))).wrap());
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(room.height() - 160.0)
                .auto_shrink([false, true])
                .show(ui, body);
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if ui.button("Cancel").clicked() {
                    return Some(false);
                }
                ui.add_enabled(can, egui::Button::new(yes))
                    .clicked()
                    .then_some(true)
            })
            .inner
        });
    modal.inner.or(modal.should_close().then_some(false))
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

/// 30 -> "less than a minute left", 690 -> "about 12 min left", 7800 -> "about 2 h 10 min left"
fn time_left(secs: u64) -> String {
    let min = secs.div_ceil(60);
    match min {
        ..=1 => "less than a minute left".into(),
        2..=59 => format!("about {min} min left"),
        _ => format!("about {} h {} min left", min / 60, min % 60),
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
        // PB_FIXTURE="worst|empty|one|many [restore] [transfer] [tools] [settings] [delete|rename|copy] [light|dark] [wifi] [busy]
        // [choose] [zoom=<factor>] [scroll=<px>] [pick=<backup folder>] [size=<w>x<h>] [shot=<file.png>]" opens
        // straight on test data; delete, rename and copy open that dialog for the first backup; wifi opens its dialog; choose opens "Choose files to back up"; busy shows a job running; pick= opens the file picker on a real folder; shot= saves a
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

    fn start(&mut self, ctx: &egui::Context) {
        let phones = self.phones.clone().unwrap_or_default();
        let (dest, pick, tx, ctx, busy) = (
            self.dest.clone(),
            self.pick.clone(),
            self.tx.clone(),
            ctx.clone(),
            self.busy.clone(),
        );
        self.begin(Job::Backup);
        self.result = None;
        thread::spawn(move || {
            backup::keep_awake(true);
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
                backup::mtp_backup(&dest, &pick, &report).map(|s| vec![s])
            } else {
                // One phone failing (say, unplugged) must not skip the others.
                let (mut done, mut failed) = (Vec::new(), Vec::new());
                for (serial, model) in &adb {
                    match backup::adb_backup(serial, model, &dest, pick.clone(), &report) {
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
            backup::keep_awake(false);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Done(result));
        });
    }

    /// Marks a job as running, before its worker starts.
    fn begin(&mut self, job: Job) {
        self.busy.store(true, Ordering::Relaxed);
        backup::STOP.store(false, Ordering::Relaxed);
        self.job = job;
        self.status = "Starting…".into();
        self.now.clear();
        self.fraction = None;
    }

    /// The model of the ADB phone with this serial, or the serial when it is gone.
    fn model_of(&self, serial: &str) -> String {
        self.phones
            .iter()
            .flatten()
            .find_map(|p| match p {
                Phone::Adb {
                    serial: s, model, ..
                } if s == serial => Some(model.clone()),
                _ => None,
            })
            .unwrap_or_else(|| serial.to_string())
    }

    /// Restores photos and files first, then apps, from one backup folder.
    /// With `check`, changes nothing on the phone and reports what a restore would do.
    fn restore(&mut self, ctx: &egui::Context, plan: Plan, check: bool) {
        self.begin(Job::Restore);
        self.restore_msg = None;
        self.preview = None;
        self.checking = check.then(|| plan.clone());
        let (serial, folder, files, apps, only) = plan;
        let phone = self.model_of(&serial);
        self.restore_report = (!check).then(|| folder.join(backup::RESTORE_REPORT));
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            backup::keep_awake(true);
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let pick = Pick {
                files,
                apps,
                data: false,
                ..Pick::default()
            };
            let result = backup::restore(
                &serial,
                &phone,
                &folder,
                pick,
                only.as_deref(),
                check,
                &report,
            );
            backup::keep_awake(false);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Restored(result));
        });
    }

    /// Re-hashes a backup's files in a worker and compares them with its checksums.
    fn verify(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.begin(Job::Verify);
        self.verified = None;
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            backup::keep_awake(true);
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let result = backup::verify(&folder, &report);
            backup::keep_awake(false);
            busy.store(false, Ordering::Relaxed);
            report(Msg::Verified(folder, result));
        });
    }

    /// Exports the backup's contacts, calls, messages, calendar and photos to its exports folder,
    /// then opens that folder.
    fn export(&mut self, ctx: &egui::Context, folder: PathBuf) {
        self.begin(Job::Export);
        self.export_msg = None;
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let result = export::export(&folder, &report);
            let out = folder.join(export::EXPORTS);
            if result.is_ok() {
                let _ = Command::new("explorer").arg(&out).spawn();
            }
            busy.store(false, Ordering::Relaxed);
            report(Msg::Exported(out, result));
        });
    }

    /// Phone to phone. Without a checked plan: backs up the first phone and checks what
    /// restoring it to the second would do. With one: the safety backup, then the restore.
    fn transfer(&mut self, ctx: &egui::Context, plan: Option<(Plan, bool)>) {
        let (Some(from), Some(to)) = (self.from.clone(), self.to.clone()) else {
            return;
        };
        let (from_model, to_model) = (self.model_of(&from), self.model_of(&to));
        let pick = Pick {
            files: self.restore_files,
            apps: self.restore_apps,
            data: false,
            ..Pick::default()
        };
        self.begin(Job::Restore);
        self.restore_msg = None;
        self.preview = None;
        let (tx, ctx, busy, dest, safety) = (
            self.tx.clone(),
            ctx.clone(),
            self.busy.clone(),
            self.dest.clone(),
            self.safety,
        );
        let report = move |m: Msg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        match plan {
            Some(((_, folder, _, _, Some(ran)), false)) => {
                self.checking = None;
                self.restore_report = Some(folder.join(backup::RESTORE_REPORT));
                thread::spawn(move || {
                    let r = backup::transfer_now(
                        (&to, &to_model),
                        &dest,
                        &folder,
                        &ran,
                        pick,
                        safety,
                        &report,
                    );
                    busy.store(false, Ordering::Relaxed);
                    report(Msg::Restored(r));
                });
            }
            _ => {
                // The folder is filled in when the backup of the first phone has made it.
                self.checking = Some((to.clone(), PathBuf::new(), pick.files, pick.apps, None));
                self.restore_report = None;
                thread::spawn(move || {
                    let (folder, r) = backup::transfer_check(
                        (&from, &from_model),
                        (&to, &to_model),
                        &dest,
                        pick,
                        &report,
                    );
                    busy.store(false, Ordering::Relaxed);
                    report(Msg::Transferred(folder.map(|(f, ran)| (from, f, ran)), r));
                });
            }
        }
    }

    /// Copies one backup into another, or deletes one, on a worker.
    fn tidy(&mut self, ctx: &egui::Context, job: Job, from: PathBuf, into: Option<PathBuf>) {
        self.begin(job);
        self.tidy_msg = None;
        if job == Job::Delete {
            self.status = "Moving the backup to the Recycle Bin…".into();
        }
        let (tx, ctx, busy) = (self.tx.clone(), ctx.clone(), self.busy.clone());
        thread::spawn(move || {
            let report = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let r = match into {
                Some(into) => backup::merge_backups(&from, &into, &report),
                None => backup::delete_backup(&from),
            };
            busy.store(false, Ordering::Relaxed);
            report(Msg::Tidied(r));
        });
    }

    /// Opens the file picker; a worker lists the backup, then makes previews of the rows in view.
    fn open_picker(&mut self, ctx: &egui::Context, folder: PathBuf, delete: bool) {
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
            flags: Vec::new(),
            chosen: HashSet::new(),
            thumbs: HashMap::new(),
            asked: Vec::new(),
            ask,
            rx,
            delete,
        });
    }

    fn picker_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        let Some(p) = &mut self.picker else { return };
        while let Ok(found) = p.rx.try_recv() {
            match found {
                Found::List(list) => {
                    if let Ok(list) = &list {
                        p.chosen = match (&self.only, &self.free_pick) {
                            // Nothing is chosen for deleting until the user ticks it.
                            (_, Some((folder, chosen))) if p.delete && *folder == p.folder => {
                                chosen.clone()
                            }
                            _ if p.delete => HashSet::new(),
                            (Some((folder, chosen, _)), _) if *folder == p.folder => {
                                (**chosen).clone()
                            }
                            _ => list.iter().map(|(f, ..)| f.clone()).collect(),
                        };
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs() as i64);
                        p.flags = backup::flags(list, now);
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
            if p.delete {
                "Choose files to delete from the phone"
            } else {
                "Choose files to restore"
            },
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
                                p.chosen = list.iter().map(|(f, ..)| f.clone()).collect();
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
                        // Files that may not be worth restoring, one button per reason.
                        let flagged: Vec<_> = FLAGS
                            .iter()
                            .map(|&(bit, word)| {
                                let n = p.flags.iter().filter(|f| *f & bit != 0).count();
                                (bit, word, n)
                            })
                            .filter(|f| f.2 > 0)
                            .collect();
                        if !flagged.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for (bit, word, n) in flagged {
                                    let label = match (word, n) {
                                        ("large", _) => format!("Untick {} over 1 GB", files(n)),
                                        ("duplicate", 1) => "Untick 1 duplicate".into(),
                                        ("duplicate", _) => {
                                            format!("Untick {} duplicates", count(n))
                                        }
                                        ("empty", 1) => "Untick 1 empty file".into(),
                                        ("empty", _) => format!("Untick {} empty files", count(n)),
                                        (_, _) => format!("Untick {} {word}", files(n)),
                                    };
                                    if ui.button(label).clicked() {
                                        for ((path, ..), f) in list.iter().zip(&p.flags) {
                                            if f & bit != 0 {
                                                p.chosen.remove(path);
                                            }
                                        }
                                    }
                                }
                            });
                        }
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
                                for ((path, bytes, _), flag) in
                                    list[rows.clone()].iter().zip(&p.flags[rows])
                                {
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
                                                // Why it may not be worth restoring, before the folder so it is never cut.
                                                let why: Vec<_> = FLAGS
                                                    .iter()
                                                    .filter(|(bit, _)| flag & bit != 0)
                                                    .map(|(_, word)| *word)
                                                    .collect();
                                                if !why.is_empty() {
                                                    let why = why.join(", ");
                                                    let mut why =
                                                        why[..1].to_uppercase() + &why[1..];
                                                    why.push('.');
                                                    ui.label(RichText::new(why).size(12.0));
                                                }
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
            if p.delete {
                self.free_pick = Some((p.folder, p.chosen));
                return;
            }
            self.only =
                (p.chosen.len() < list.len()).then(|| (p.folder, Arc::new(p.chosen), list.len()));
        }
    }

    fn run_wifi(&mut self, ctx: &egui::Context, args: Vec<String>) {
        self.stop_qr();
        self.wifi_working = true;
        self.wifi_msg = None;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = tx.send(Msg::Wifi(backup::wifi(&args)));
            ctx.request_repaint();
        });
    }

    /// Shows a new QR code and waits in the background for the phone to scan it.
    fn show_qr(&mut self, ctx: &egui::Context) {
        let (name, password, text) = backup::qr_code();
        self.qr = qrcodegen::QrCode::encode_text(&text, qrcodegen::QrCodeEcc::Medium).ok();
        self.wifi_msg = None;
        let id = backup::QR_WAIT.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        thread::spawn(move || {
            if let Some(r) = backup::qr_pair(&name, &password, id) {
                let _ = tx.send(Msg::Wifi(r));
                ctx.request_repaint();
            }
        });
    }

    fn stop_qr(&mut self) {
        if self.qr.take().is_some() {
            backup::QR_WAIT.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Re-reads the backups in `folder` and keeps the choice when it is still there.
    fn scan(&mut self, folder: &std::path::Path) {
        if self.fixture != Fixture::Real {
            return;
        }
        self.backups = backup::saved_backups(folder);
        self.scanned = folder.to_path_buf();
        let (tx, paths): (_, Vec<PathBuf>) = (
            self.tx.clone(),
            self.backups.iter().map(|b| b.path.clone()).collect(),
        );
        // Adding up 100,000 files takes seconds, so off the window's thread. The phone check
        // repaints every 2 seconds, which shows the sizes.
        thread::spawn(move || {
            let sizes = paths
                .into_iter()
                .map(|p| {
                    let n = backup::folder_size(&p);
                    (p, n)
                })
                .collect();
            let _ = tx.send(Msg::FolderSizes(sizes));
        });
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
        if self.tab == Tab::Tools {
            self.tools_page(ui, t);
            return None;
        }
        let job = |app: &mut Self, ui: &mut egui::Ui| match app.tab {
            Tab::Backup => app.backup_tab(ui, t),
            Tab::Transfer => app.transfer_tab(ui, t),
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
            Tab::Transfer => "Copy one phone to another",
            Tab::Tools => "Tools",
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
        let bar = if matches!(self.tab, Tab::Settings | Tab::Tools) {
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
                    if matches!(tab, Tab::Restore | Tab::Tools) {
                        let dest = app.dest.clone();
                        app.scan(&dest);
                    }
                    if tab == Tab::Tools {
                        app.scrcpy = tools::scrcpy_path();
                    }
                }
            };
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if side {
                item(self, ui, Tab::Backup, "Back up");
                item(self, ui, Tab::Restore, "Restore");
                item(self, ui, Tab::Transfer, "Transfer");
                item(self, ui, Tab::Tools, "Tools");
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    item(self, ui, Tab::Settings, "Settings")
                });
            } else {
                ui.horizontal(|ui| {
                    item(self, ui, Tab::Backup, "Back up");
                    item(self, ui, Tab::Restore, "Restore");
                    item(self, ui, Tab::Transfer, "Transfer");
                    item(self, ui, Tab::Tools, "Tools");
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
                        if ui
                            .add_enabled(!stopping && self.job != Job::Delete, stop)
                            .clicked()
                        {
                            backup::STOP.store(true, Ordering::Relaxed);
                        }
                    } else if primary(ui, t, cmd.label, cmd.enabled) {
                        match cmd.plan {
                            _ if self.tab == Tab::Transfer => self.transfer(ui.ctx(), cmd.plan),
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
                                egui::Label::new(
                                    RichText::new(&cmd.note).color(t.muted).size(12.0),
                                )
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
                    adb_missing(ui, t);
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
    fn phone_help(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        // Fixture screenshots open them; there is no click to do it.
        let open = self.fixture == Fixture::Empty;
        ui.add_space(4.0);
        egui::CollapsingHeader::new("How to turn on USB debugging")
            .default_open(open)
            .show(ui, |ui| {
                egui::ComboBox::from_label("Phone brand")
                    .selected_text(BRANDS[self.brand].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in BRANDS.iter().enumerate() {
                            ui.selectable_value(&mut self.brand, i, *name);
                        }
                    });
                steps(ui, t, BRANDS[self.brand].1);
            });
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
                if !self.adb_ok {
                    adb_missing(ui, t);
                }
            });
    }

    fn wifi_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        if dialog(ctx, "wifi", "Connect over Wi-Fi", 400.0, "Close", |ui| {
            self.wifi(ui, t)
        }) {
            self.wifi_open = false;
            self.stop_qr();
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
        let working = self.wifi_working;
        if let Some(qr) = &self.qr {
            ui.label(muted("Tap Pair device with QR code and scan this."));
            qr_image(ui, qr);
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label("Waiting for the phone to scan the code…");
            });
        } else if ui
            .add_enabled(!working, egui::Button::new("Show QR code"))
            .clicked()
        {
            self.show_qr(ui.ctx());
        }
        ui.add_space(8.0);
        ui.label(muted(
            "Or tap Pair device with pairing code, then type what it shows.",
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

    fn backup_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
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
                    self.tab = Tab::Settings;
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
    fn adb_phone(&self) -> Option<(String, String)> {
        self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
            _ => None,
        })
    }

    /// Reads the sizes on the phone in the background, unless they are known or being read.
    fn measure(&mut self, ctx: &egui::Context) {
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
    fn measured(&mut self) -> Option<(u64, &Tally)> {
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
    fn choose_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
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
        }
    }

    /// Phone to phone: which phone to copy from and to, and what to copy.
    fn transfer_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
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
    fn label(all: &[backup::Saved], b: &backup::Saved) -> String {
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
    fn ask_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
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
            // A size not added up yet, on the backup of unknown date.
            bytes: age.map(|a| 2_147_483_648 + a * 937_000_123),
        };
        let adb = |model: &str, wifi: bool, battery: Option<u8>| Phone::Adb {
            serial: format!("{model}-serial"),
            model: model.into(),
            wifi,
            battery,
        };
        self.restore_msg = None;
        self.export_msg = None;
        self.result = None;
        self.wifi_msg = None;
        self.verified = None;
        match f {
            Fixture::Real => {
                self.phones = None;
                self.backups.clear();
                let dest = self.dest.clone();
                self.scan(&dest);
            }
            Fixture::Worst => {
                self.phones = Some(vec![adb("Redmi Note 13 Pro+ 5G", true, Some(9))]);
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
                self.verified = Some((
                    self.backups[0].path.clone(),
                    Err("1,284 files are changed, for example files/Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-20240817-WA0007.mp4, files/DCIM/Camera/PXL_20240817_183455123.NIGHT.jpg, files/Download/bartholomew.fitzgerald@northwind-industries-holdings.example.com_invoice.pdf.\n3 files are missing, for example data/sms.txt, apps/com.whatsapp/base.apk, files/ünïcødé 日本.txt.\nBack up again to copy them from the phone, if it still has them.".into()),
                ));
                let exports = self
                    .dest
                    .join("Redmi Note 13 Pro+ 5G_R5CT0000")
                    .join("exports");
                self.export_msg = Some((
                    exports.clone(),
                    Ok(format!(
                        "Exported contacts, call history, messages, calendar, photos and videos to {}.",
                        exports.display()
                    )),
                ));
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
                    pick: Pick::default(),
                }]));
                self.wifi_msg = Some(backup::wifi_result(
                    "connect",
                    "failed to connect to 'aleksandras-redmi-note-13-pro-plus.northwind.local:41235': No connection could be made because the target machine actively refused it. (10061)",
                ));
                self.connect_addr =
                    "aleksandras-redmi-note-13-pro-plus.northwind.local:41235".into();
                let long = "Aleksandra's scanned university documents and receipts 2019-2024";
                let list = [
                    ("DCIM/Camera/PXL_20240817_183455123.NIGHT.jpg", 48_213_000_000),
                    ("DCIM/Camera/VID_20240817.mp4", 96_100_000_000),
                    ("Download/bartholomew.fitzgerald@northwind-industries-holdings.example.com_invoice.pdf", 1_200_000),
                    (&format!("{long}/scan.pdf"), 3_400_000_000),
                    ("Music/a.flac", 12_000_000_000),
                    ("Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Video/VID-1.mp4", 7_000_000_000),
                    ("日本語のフォルダ/メモ.txt", 4_000),
                    ("notes.txt", 0),
                ]
                .iter()
                .map(|(p, s)| (p.to_string(), *s))
                .collect();
                self.sizes = Some((
                    "Redmi Note 13 Pro+ 5G-serial".into(),
                    Some((list, 21_400_000_000)),
                ));
                self.pick.folders = ["DCIM".to_string(), long.to_string(), String::new()].into();
                self.pick.kinds = [true, true, false, true, false];
                self.pick.max_size = Some(4_000_000_000);
                let root = self.backups[0].path.join("files");
                let at = |p: &str, n: u64| (root.join(p), n);
                self.show_dupes(vec![
                    vec![
                        at(r"DCIM\Camera\VID_20240817_183455123_HDR_ultra_long_name.mp4", 1_284_000_000),
                        at(r"Android\media\com.whatsapp\WhatsApp\Media\WhatsApp Video\Sent\VID-20240817-WA0007.mp4", 1_284_000_000),
                    ],
                    vec![
                        at(r"DCIM\Camera\IMG_1.jpg", 3_400_000),
                        at(r"Pictures\Telegram\IMG_1.jpg", 3_400_000),
                        at(r"Download\ünïcødé 日本 IMG_1.jpg", 3_400_000),
                    ],
                ]);
                self.tool_msg = Some(("contacts", Ok("Merged 1,284 contacts into 1,190, saved as contacts-merged.vcf next to contacts.vcf. The original is unchanged.".into())));
                self.free_pick = Some((self.backups[0].path.clone(), [root.join("x")].into()));
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
                    adb("Pixel 8", false, None),
                    adb("Galaxy S23 Ultra", false, Some(14)),
                    adb("Redmi Note 13 Pro+ 5G", true, None),
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

/// The Tools page: each tool works on the backup chosen at the top.
impl App {
    fn adb_serial(&self) -> Option<String> {
        self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, .. } => Some(serial.clone()),
            _ => None,
        })
    }

    /// Runs a tool on a worker thread; `which` is the panel that shows its progress and result.
    fn run_tool(
        &mut self,
        ctx: &egui::Context,
        which: &'static str,
        job: impl FnOnce(&dyn Fn(String)) -> tools::Done + Send + 'static,
    ) {
        tools::STOP.store(false, Ordering::Relaxed);
        self.tool_busy = Some(which);
        self.tool_status = "Working…".into();
        self.tool_msg = None;
        let (tx, ctx) = (self.tool_tx.clone(), ctx.clone());
        thread::spawn(move || {
            let say = |s: String| {
                let _ = tx.send(tools::Done::Status(s));
                ctx.request_repaint();
            };
            let done = job(&say);
            let _ = tx.send(done);
            ctx.request_repaint();
        });
    }

    /// Lists the groups found; every copy but the first of each group starts ticked.
    fn show_dupes(&mut self, groups: Vec<Vec<(PathBuf, u64)>>) {
        self.ticked = groups
            .iter()
            .flat_map(|g| g.iter().skip(1).map(|(p, _)| p.clone()))
            .collect();
        let rows = groups
            .iter()
            .enumerate()
            .flat_map(|(i, g)| {
                std::iter::once((i, None)).chain((0..g.len()).map(move |f| (i, Some(f))))
            })
            .collect();
        self.dupes = Some((groups, rows));
    }

    /// The running job's progress and Stop, or what the panel's last job said.
    fn tool_note(&mut self, ui: &mut egui::Ui, t: &Tokens, which: &'static str) {
        if self.tool_busy == Some(which) {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(16.0));
                ui.add(egui::Label::new(&self.tool_status).wrap());
            });
            let stopping = tools::STOP.load(Ordering::Relaxed);
            if which != "phone"
                && ui
                    .add_enabled(
                        !stopping,
                        egui::Button::new(if stopping { "Stopping…" } else { "Stop" }),
                    )
                    .clicked()
            {
                tools::STOP.store(true, Ordering::Relaxed);
            }
        } else if let Some((w, r)) = &self.tool_msg
            && *w == which
        {
            match r {
                Ok(m) => ui.label(m.as_str()),
                Err(e) => ui.label(RichText::new(e.as_str()).color(t.error)),
            };
        }
    }

    fn tools_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let muted = |s: &str| RichText::new(s).color(t.muted);
        ui.set_max_width(ui.available_width().min(640.0));
        let running = self.tool_busy.is_some();
        let label = |b: &backup::Saved| format!("{}, backed up {}", b.name, days_ago(b.age_days));
        panel(ui, t, "Backup", "", |ui| {
            if self.backups.is_empty() {
                ui.label(format!("No backups in {} yet.", self.scanned.display()));
            } else {
                let shown = self
                    .backups
                    .iter()
                    .find(|b| Some(&b.path) == self.chosen.as_ref())
                    .map_or("Choose a backup".into(), label);
                ui.add_enabled_ui(!running, |ui| {
                    egui::ComboBox::from_id_salt("tool_backup")
                        .width(ui.available_width())
                        .truncate()
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for b in &self.backups {
                                let on = Some(&b.path) == self.chosen.as_ref();
                                if ui.selectable_label(on, label(b)).clicked() && !on {
                                    self.chosen = Some(b.path.clone());
                                    self.dupes = None;
                                    self.tool_msg = None;
                                }
                            }
                        });
                });
            }
            ui.label(muted("The tools below work on this backup."));
            if ui
                .add_enabled(!running, egui::Button::new("Find another backup"))
                .clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_directory(&self.dest)
                    .pick_folder()
            {
                self.chosen = Some(dir.clone());
                self.scan(&dir);
                self.dupes = None;
                self.tool_msg = None;
            }
        });
        let folder = self.chosen.clone().filter(|_| !self.backups.is_empty());
        let can = folder.is_some() && !running;

        ui.add_space(8.0);
        panel(ui, t, "Duplicate files", "", |ui| {
            ui.label(muted("Finds photos and files saved more than once in this backup, such as pictures shared in several chats."));
            if ui
                .add_enabled(can, egui::Button::new("Find duplicates"))
                .clicked()
                && let Some(f) = folder.clone()
            {
                self.dupes = None;
                self.run_tool(ui.ctx(), "dupes", move |say| {
                    tools::Done::Dupes(tools::duplicates(&f, say))
                });
            }
            self.tool_note(ui, t, "dupes");
            let Some((groups, rows)) = &self.dupes else {
                return;
            };
            if groups.is_empty() {
                ui.label("No duplicates found.");
                return;
            }
            let ticked: Vec<(PathBuf, u64)> = groups
                .iter()
                .flatten()
                .filter(|(p, _)| self.ticked.contains(p))
                .cloned()
                .collect();
            let bytes: u64 = ticked.iter().map(|(_, n)| n).sum();
            ui.label(format!(
                "{} files are saved more than once. {} ticked to delete ({}); one copy of each stays.",
                count(groups.len()),
                count(ticked.len()),
                size(bytes)
            ));
            // Rows of 22 px with 4 px between; show_rows takes the height without the gap.
            const ROW: f32 = 26.0;
            let height = (rows.len() as f32 * ROW - 4.0).min(320.0);
            egui::ScrollArea::vertical()
                .id_salt("dupes")
                .max_height(height)
                .min_scrolled_height(height)
                .auto_shrink([false, true])
                .show_rows(ui, ROW - 4.0, rows.len(), |ui, range| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    ui.spacing_mut().interact_size.y = ROW - 4.0;
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for &(g, f) in &rows[range] {
                        let group = &groups[g];
                        match f {
                            None => {
                                ui.label(
                                    RichText::new(format!(
                                        "{} copies, {} each",
                                        group.len(),
                                        size(group[0].1)
                                    ))
                                    .font(semibold(14.0)),
                                );
                            }
                            Some(f) => {
                                let path = &group[f].0;
                                let shown = path
                                    .strip_prefix(folder.as_deref().unwrap_or(path))
                                    .unwrap_or(path);
                                let mut on = self.ticked.contains(path);
                                if ui
                                    .add_enabled(
                                        !running,
                                        egui::Checkbox::new(&mut on, shown.display().to_string()),
                                    )
                                    .on_hover_text(path.display().to_string())
                                    .changed()
                                {
                                    if on {
                                        self.ticked.insert(path.clone());
                                    } else {
                                        self.ticked.remove(path);
                                    }
                                }
                            }
                        }
                    }
                });
            // Every copy of a file ticked would delete it from the backup altogether.
            let all_gone = groups
                .iter()
                .any(|g| g.iter().all(|(p, _)| self.ticked.contains(p)));
            if ui
                .add_enabled(
                    can && !ticked.is_empty() && !all_gone,
                    egui::Button::new("Delete ticked copies…"),
                )
                .on_disabled_hover_text(if all_gone {
                    "Leave at least one copy of each file unticked."
                } else {
                    "Tick the copies to delete."
                })
                .clicked()
            {
                self.confirm = Some(Confirm::Copies(ticked));
            }
        });

        ui.add_space(8.0);
        panel(ui, t, "HEIC photos to JPG", "", |ui| {
            ui.label(muted("Saves a JPG copy of each HEIC photo in the backup's jpg folder, keeping its date and location. Many Windows apps cannot open HEIC."));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can, egui::Button::new("Convert to JPG"))
                    .clicked()
                    && let Some(f) = folder.clone()
                {
                    self.run_tool(ui.ctx(), "heic", move |say| {
                        tools::Done::Text(tools::heic_to_jpg(&f, say))
                    });
                }
                if let Some(f) = &folder
                    && matches!(&self.tool_msg, Some(("heic", Ok(_))))
                    && ui.button("Open jpg folder").clicked()
                {
                    let _ = Command::new("explorer").arg(f.join("jpg")).spawn();
                }
            });
            self.tool_note(ui, t, "heic");
        });

        ui.add_space(8.0);
        panel(ui, t, "Duplicate contacts", "", |ui| {
            ui.label(muted(&format!("Merges contacts with the same name, phone number or email into {}. Your contacts.vcf stays as it was.", tools::MERGED)));
            let merged = matches!(&self.tool_msg, Some(("contacts", Ok(_))));
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can, egui::Button::new("Merge duplicates"))
                    .clicked()
                    && let Some(f) = folder.clone()
                {
                    self.run_tool(ui.ctx(), "contacts", move |_| {
                        tools::Done::Text(tools::merge_contacts(&f))
                    });
                }
                if let Some(f) = &folder
                    && merged
                {
                    let data = f.join("data");
                    if ui.button("Open folder").clicked() {
                        let _ = Command::new("explorer").arg(&data).spawn();
                    }
                    if ui.button("Undo").clicked() {
                        self.tool_msg = Some((
                            "contacts",
                            match fs::remove_file(data.join(tools::MERGED)) {
                                Ok(()) => Ok(format!(
                                    "Removed {}. contacts.vcf was never changed.",
                                    tools::MERGED
                                )),
                                Err(e) => Err(format!("Cannot remove {}: {e}", tools::MERGED)),
                            },
                        ));
                    }
                }
            });
            self.tool_note(ui, t, "contacts");
        });

        let serial = self.adb_serial();
        ui.add_space(8.0);
        panel(ui, t, "Free up space on the phone", "", |ui| {
            ui.label(muted("Deletes files from the phone that are safely in this backup. Only files whose backup copy has the same size are deleted, and the app asks first."));
            let chosen = self
                .free_pick
                .as_ref()
                .filter(|(f, _)| Some(f) == folder.as_ref())
                .map(|(_, c)| c.clone())
                .unwrap_or_default();
            ui.label(if chosen.is_empty() {
                "No files chosen yet.".to_string()
            } else {
                format!("{} chosen.", files(chosen.len()))
            });
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(can, egui::Button::new("Choose files…"))
                    .clicked()
                    && let Some(f) = folder.clone()
                {
                    self.open_picker(ui.ctx(), f, true);
                }
                let ready = can && serial.is_some() && !chosen.is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("Delete from phone…"))
                    .on_disabled_hover_text(if serial.is_none() {
                        "Connect the phone with USB debugging on."
                    } else {
                        "Choose the files first."
                    })
                    .clicked()
                    && let (Some(f), Some(serial)) = (folder.clone(), serial.clone())
                {
                    self.run_tool(ui.ctx(), "phone", move |say| {
                        say("Checking the files on the phone…".into());
                        tools::Done::Matched(
                            tools::chosen_on_phone(&f, &chosen)
                                .and_then(|wanted| tools::match_on_phone(&serial, &wanted)),
                        )
                    });
                }
            });
            self.tool_note(ui, t, "phone");
        });

        ui.add_space(8.0);
        panel(ui, t, "See the phone's screen", "", |ui| {
            ui.label(muted("scrcpy, a free app by Genymobile, shows the phone's screen on this computer and lets you use it with the mouse and keyboard."));
            match &self.scrcpy {
                Some(exe) => {
                    if ui
                        .add_enabled(serial.is_some(), egui::Button::new("Open scrcpy"))
                        .on_disabled_hover_text("Connect the phone with USB debugging on.")
                        .clicked()
                    {
                        let mut c = Command::new(exe);
                        c.args(["-s", serial.as_deref().unwrap_or_default()]);
                        // One adb for both apps: two different versions stop each other's server.
                        if let Some(adb) = backup::adb_path() {
                            c.env("ADB", adb);
                        }
                        #[cfg(windows)]
                        std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000);
                        self.tool_msg = c
                            .spawn()
                            .err()
                            .map(|e| ("scrcpy", Err(format!("scrcpy did not start: {e}"))));
                    }
                }
                None => {
                    ui.label("scrcpy is not on this computer. Unzip it next to this app to use it from here.");
                    ui.hyperlink_to(
                        "Get scrcpy on GitHub",
                        "https://github.com/Genymobile/scrcpy",
                    );
                }
            }
            self.tool_note(ui, t, "scrcpy");
        });
    }

    /// Asks before a tool deletes anything; Esc or a click outside keeps the files.
    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let (title, body, yes) = match c {
            Confirm::Copies(list) => (
                format!("Delete {} from the backup?", files(list.len())),
                format!(
                    "This frees {} on this computer. One copy of each file stays in the backup. While the phone still has these files, the next backup copies them again.",
                    size(list.iter().map(|(_, n)| n).sum())
                ),
                format!("Delete {}", files(list.len())),
            ),
            Confirm::Phone(_, list, left) => {
                let mut body = format!(
                    "This frees {} on the phone. Their copies in the backup have the same size and stay on this computer. Deleted files cannot be brought back on the phone except by restoring them.",
                    size(list.iter().map(|(_, n)| n).sum())
                );
                if *left > 0 {
                    body += &format!(
                        " {} chosen are left alone: they are not on the phone or differ from the backup.",
                        files(*left)
                    );
                }
                (
                    format!("Delete {} from the phone?", files(list.len())),
                    body,
                    format!("Delete {}", files(list.len())),
                )
            }
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm_delete"))
            .frame(dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width().min(360.0));
                ui.label(RichText::new(title).font(semibold(16.0)));
                ui.label(body);
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(yes).clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Keep them").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if modal.should_close() {
            answer = Some(false);
        }
        let Some(yes) = answer else { return };
        match self.confirm.take() {
            Some(Confirm::Copies(list)) if yes => {
                self.dupes = None;
                self.run_tool(ctx, "dupes", move |_| {
                    tools::Done::Text(tools::delete_copies(&list))
                });
            }
            Some(Confirm::Phone(serial, list, _)) if yes => {
                self.free_pick = None;
                self.run_tool(ctx, "phone", move |say| {
                    say("Deleting from the phone…".into());
                    tools::Done::Text(tools::delete_from_phone(&serial, &list))
                });
            }
            _ => {}
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

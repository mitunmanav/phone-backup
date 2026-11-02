use crate::*;

/// A titled panel: the title, with quiet `aside` text at its right, a hairline, then the body.
pub(crate) fn panel<R>(
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
pub(crate) fn result_box(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.info)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(16, 12))
}

/// One page tab: ink text on the current tab, muted on the others, and a fill on hover. The
/// accent pill under the current tab is drawn by the top bar, so it can slide.
pub(crate) fn nav_item(ui: &mut egui::Ui, t: &Tokens, label: &str, on: bool) -> egui::Response {
    let color = if on && ui.is_enabled() {
        t.ink
    } else {
        t.muted
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(14.0), color);
    let width = galley.size().x
        + if ui.available_width() < 400.0 {
            16.0
        } else {
            24.0
        };
    let (rect, r) = ui.allocate_exact_size(vec2(width, 34.0), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), on, label));
    if r.hovered() {
        ui.painter().rect_filled(rect, 4, t.hover);
    }
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
    // egui draws no focus for a hand-made widget.
    if r.has_focus() {
        ui.painter()
            .rect_stroke(rect, 4, Stroke::new(2.0, t.ink), StrokeKind::Inside);
    }
    r
}

/// A settings row: its name, and a note under it, on the left; its control on the right; a
/// hairline under it.
pub(crate) fn row(
    ui: &mut egui::Ui,
    t: &Tokens,
    name: &str,
    note: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(4.0);
    let text = |ui: &mut egui::Ui| {
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(egui::Label::new(name).wrap());
            if !note.is_empty() {
                ui.add(egui::Label::new(RichText::new(note).color(t.muted).size(12.0)).wrap());
            }
        });
    };
    // Starts 0 tall and grows to its content; with_layout would take the whole page height.
    let size = vec2(ui.available_width(), 0.0);
    let controls = Layout::right_to_left(Align::Min);
    if size.x < 480.0 {
        // Narrow: the text gets the full width, the control goes under it.
        ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), text);
        ui.allocate_ui_with_layout(size, controls, control);
    } else {
        ui.allocate_ui_with_layout(size, controls, |ui| {
            control(ui);
            ui.add_space(8.0);
            text(ui);
        });
    }
    ui.add_space(4.0);
    let x = ui.max_rect().x_range();
    ui.painter()
        .hline(x, ui.cursor().top(), Stroke::new(1.0, t.line));
    ui.add_space(1.0);
}

/// A settings row that opens a page: the whole row is the button, with a chevron at its right.
pub(crate) fn link_row(ui: &mut egui::Ui, t: &Tokens, name: &str, note: &str) -> bool {
    let width = ui.available_width();
    let text = |s: &str, size: f32, color: Color32| {
        ui.painter()
            .layout(s.into(), FontId::proportional(size), color, width - 48.0)
    };
    let (name_g, note_g) = (text(name, 14.0, t.ink), text(note, 12.0, t.muted));
    let h = name_g.size().y + 2.0 + note_g.size().y + 20.0;
    let (rect, r) = ui.allocate_exact_size(vec2(width, h), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, name));
    let p = ui.painter();
    // The fill and focus ring reach 8 px past the text, so the text lines up with plain rows.
    let fill = rect.expand2(vec2(8.0, 0.0));
    if r.hovered() {
        p.rect_filled(fill, 4, t.hover);
    }
    let at = rect.min + vec2(0.0, 10.0);
    let below = at + vec2(0.0, name_g.size().y + 2.0);
    p.galley(at, name_g, t.ink);
    p.galley(below, note_g, t.muted);
    // The chevron nudges right on hover: this row leads somewhere.
    let nudge = ui.ctx().animate_bool_with_time(r.id, r.hovered(), 0.15) * 3.0;
    p.text(
        egui::pos2(rect.max.x - 16.0 + nudge, rect.center().y),
        Align2::CENTER_CENTER,
        "›",
        FontId::proportional(18.0),
        t.muted,
    );
    p.hline(rect.x_range(), rect.max.y, Stroke::new(1.0, t.line));
    if r.has_focus() {
        p.rect_stroke(fill, 4, Stroke::new(2.0, t.ink), StrokeKind::Inside);
    }
    r.clicked()
}

/// A Settings section's heading.
pub(crate) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(20.0);
    ui.label(RichText::new(title).font(semibold(14.0)));
}

/// Numbered steps, each wrapped line lined up under its first.
pub(crate) fn steps(ui: &mut egui::Ui, t: &Tokens, list: &[&str]) {
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

/// USB debugging only helps once adb is on this computer.
pub(crate) fn adb_missing(ui: &mut egui::Ui, t: &Tokens) {
    ui.label(RichText::new("USB debugging also needs Google's free Platform Tools on this laptop. Unzip them and put adb.exe next to this app.").color(t.muted));
    ui.hyperlink_to(
        "Download Platform Tools",
        "https://developer.android.com/tools/releases/platform-tools",
    );
}

/// The main action of a page: accent when it can run, a quiet field otherwise.
pub(crate) fn primary(ui: &mut egui::Ui, t: &Tokens, label: &str, enabled: bool) -> bool {
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
pub(crate) fn choice(
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
pub(crate) fn dialog(
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
pub(crate) fn confirm(
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
pub(crate) fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::popup(&ctx.global_style()).inner_margin(20)
}

/// A folder path that fits the width, cut in the middle so the last folders stay readable:
/// "C:\Users\…\Documents\Phone backups".
pub(crate) fn short_path(ui: &egui::Ui, path: &str) -> String {
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
pub(crate) fn days_ago(days: Option<u64>) -> String {
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
pub(crate) fn time_left(secs: u64) -> String {
    let min = secs.div_ceil(60);
    match min {
        ..=1 => "less than a minute left".into(),
        2..=59 => format!("about {min} min left"),
        _ => format!("about {} h {} min left", min / 60, min % 60),
    }
}

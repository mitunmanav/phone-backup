use crate::*;
use egui::{Pos2, pos2};

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

// Controls paint at BUTTON height but answer clicks across HIT, so rows stay light and targets large.
pub(crate) const HIT: f32 = 44.0;
const BUTTON: f32 = 38.0;
const RADIUS: u8 = 6;
pub(crate) const TILE_GAP: f32 = 10.0;

pub(crate) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgba_premultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

/// A critically damped spring held in egui's memory; repaints only while moving.
pub(crate) fn spring(ctx: &egui::Context, id: egui::Id, target: f32, response: f32) -> f32 {
    let dt = ctx.input(|i| i.stable_dt).min(1.0 / 30.0);
    let (mut v, mut vel) =
        ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || (target, 0.0f32)));
    let w = std::f32::consts::TAU / response;
    vel += (w * w * (target - v) - 2.0 * w * vel) * dt;
    v += vel * dt;
    if (target - v).abs() > 1e-3 || vel.abs() > 1e-3 {
        ctx.request_repaint();
    } else {
        v = target;
        vel = 0.0;
    }
    ctx.data_mut(|d| d.insert_temp(id, (v, vel)));
    v
}

/// `secs`, or 0 with Windows' animation effects off.
fn quick(ui: &egui::Ui, secs: f32) -> f32 {
    if ui.style().animation_time > 0.0 {
        secs
    } else {
        0.0
    }
}

/// Text at an exact weight and optical size. The family keeps the weight on Windows 10, whose
/// Segoe UI files ignore both settings.
pub(crate) fn txt(s: &str, size: f32, wght: f32, c: Color32) -> RichText {
    let family = if wght >= 600.0 {
        FontFamily::Name("semibold".into())
    } else {
        FontFamily::Proportional
    };
    RichText::new(s)
        .font(FontId::new(size, family))
        .color(c)
        .variation("wght", wght)
        .variation("opsz", size.clamp(8.0, 36.0))
}

fn galley(
    ui: &egui::Ui,
    text: RichText,
    wrap: egui::TextWrapMode,
    max: f32,
) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(text).into_galley(ui, Some(wrap), max, TextStyle::Body)
}

/// Paints one line of text, cut with an ellipsis at `max_w`. Anything cut can still be read in
/// full by pointing at it.
pub(crate) fn put(ui: &egui::Ui, at: Pos2, align: Align2, text: RichText, max_w: f32) -> Rect {
    let g = galley(ui, text, egui::TextWrapMode::Truncate, max_w);
    let r = align.anchor_size(at, g.size());
    if g.elided {
        let full = g.job.text.clone();
        ui.interact(r, ui.scope_id().with(("cut", &full)), egui::Sense::hover())
            .on_hover_text(full);
    }
    ui.painter().galley(r.min, g, Color32::PLACEHOLDER);
    r
}

/// One line whose digits each take the same width, so a live number does not shiver as it
/// counts: Segoe UI's "1" is narrower than its other digits, and egui has no tabular figures.
pub(crate) fn tabular(
    ui: &egui::Ui,
    at: Pos2,
    align: Align2,
    s: &str,
    text: impl Fn(&str) -> RichText,
) {
    let run = |run: &str| galley(ui, text(run), egui::TextWrapMode::Extend, f32::INFINITY);
    let slot = run("4").size().x;
    // Letters stay together, or they would lose their kerning.
    let mut runs: Vec<(bool, String)> = Vec::new();
    for c in s.chars() {
        match runs.last_mut() {
            Some((false, letters)) if !c.is_ascii_digit() => letters.push(c),
            _ => runs.push((c.is_ascii_digit(), c.to_string())),
        }
    }
    let cells: Vec<_> = runs.iter().map(|(d, s)| (*d, run(s))).collect();
    let width: f32 = cells
        .iter()
        .map(|(digit, g)| if *digit { slot } else { g.size().x })
        .sum();
    let r = align.anchor_size(at, vec2(width, run("0").size().y));
    let mut x = r.left();
    for (digit, g) in cells {
        let w = if digit { slot } else { g.size().x };
        ui.painter().galley(
            pos2(x + (w - g.size().x) / 2.0, r.top()),
            g,
            Color32::PLACEHOLDER,
        );
        x += w;
    }
}

pub(crate) fn icon(ui: &egui::Ui, center: Pos2, glyph: &str, size: f32, c: Color32) {
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(size, FontFamily::Name("icons".into())),
        c,
    );
}

/// A connector on the cable's end: neck, square body, metal tip, pointing along `out`.
pub(crate) fn connector(p: &egui::Painter, at: Pos2, out: egui::Vec2, body: Color32, tip: Color32) {
    p.line_segment([at, at + out * 2.0], Stroke::new(4.0, body));
    p.line_segment([at + out * 2.0, at + out * 10.0], Stroke::new(8.0, body));
    p.line_segment([at + out * 10.0, at + out * 14.0], Stroke::new(4.0, tip));
}

/// The tick that lands on the end that received the files.
pub(crate) fn badge(ui: &egui::Ui, t: &Tokens, c: Pos2, seal: f32) {
    if seal > 0.01 {
        let r = 10.0 * (0.8 + 0.2 * seal);
        ui.painter()
            .circle_filled(c, r + 2.5, t.bg.gamma_multiply(seal));
        ui.painter()
            .circle_filled(c, r, t.accent.gamma_multiply(seal));
        icon(ui, c, "\u{E73E}", 11.0, t.on_accent.gamma_multiply(seal));
    }
}

/// egui draws no focus for a hand-made widget.
fn ring(ui: &egui::Ui, r: &egui::Response, rect: Rect, radius: u8, t: &Tokens) {
    if r.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(3.0),
            radius + 3,
            Stroke::new(2.0, t.accent),
            StrokeKind::Outside,
        );
    }
}

/// The main action of a page: accent when it can run, a quiet fill otherwise.
pub(crate) fn primary(ui: &mut egui::Ui, t: &Tokens, label: &str, enabled: bool) -> bool {
    let g = galley(
        ui,
        txt(label, 13.0, 600.0, t.on_accent),
        egui::TextWrapMode::Extend,
        f32::INFINITY,
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 32.0, HIT), sense);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let ctx = ui.ctx();
    let press = ctx.animate_bool_with_time(r.id, r.is_pointer_button_down_on(), quick(ui, 0.1));
    let hover = ctx.animate_bool_with_time(r.id.with("h"), r.hovered(), quick(ui, 0.12));
    let face = Rect::from_center_size(
        rect.center(),
        vec2(rect.width(), BUTTON) * (1.0 - 0.03 * press),
    );
    let (fill, ink) = if enabled {
        (
            mix(t.accent, t.ink, 0.12 * hover + 0.16 * press),
            t.on_accent,
        )
    } else {
        (t.line, t.muted)
    };
    ui.painter().rect_filled(face, RADIUS, fill);
    ui.painter()
        .galley_with_override_text_color(face.center() - g.size() / 2.0, g, ink);
    ring(ui, &r, face, RADIUS, t);
    r.clicked()
}

/// An outlined button for everything but the main action. A long label is cut, not overflowed.
pub(crate) fn quiet(ui: &mut egui::Ui, t: &Tokens, label: &str, enabled: bool) -> egui::Response {
    let ink = if enabled { t.ink } else { t.muted };
    let g = galley(
        ui,
        txt(label, 13.0, 400.0, ink),
        egui::TextWrapMode::Truncate,
        ui.max_rect().width() - 32.0,
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 32.0, HIT), sense);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let ctx = ui.ctx();
    let press = ctx.animate_bool_with_time(r.id, r.is_pointer_button_down_on(), quick(ui, 0.1));
    let hover = ctx.animate_bool_with_time(r.id.with("h"), r.hovered() && enabled, quick(ui, 0.12));
    let face = Rect::from_center_size(
        rect.center(),
        vec2(rect.width(), BUTTON) * (1.0 - 0.03 * press),
    );
    let edge = if enabled {
        mix(t.line, t.border, 0.35)
    } else {
        t.line
    };
    ui.painter().rect(
        face,
        RADIUS,
        mix(t.surface, t.hover, hover),
        Stroke::new(1.0, edge),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley_with_override_text_color(face.center() - g.size() / 2.0, g, ink);
    ring(ui, &r, face, RADIUS, t);
    r
}

/// How many tiles fit across, and how big each is. Three or more across never leave one tile
/// alone on the last row.
pub(crate) fn tile_grid(width: f32, room: f32, n: usize) -> (usize, egui::Vec2) {
    let mut cols = (((width + TILE_GAP) / (140.0 + TILE_GAP)) as usize).clamp(1, n);
    if cols > 2 && n % cols == 1 {
        cols -= 1;
    }
    let rows = n.div_ceil(cols) as f32;
    let w = (width - TILE_GAP * (cols as f32 - 1.0)) / cols as f32;
    (
        cols,
        vec2(
            w,
            ((room - TILE_GAP * (rows - 1.0)) / rows).clamp(52.0, 68.0),
        ),
    )
}

/// A choice shown as a picture: glyph, name and a line of words, with a glowing edge and a corner
/// tick when chosen. `locked` cannot be changed for now; `dim` cannot be chosen at all.
pub(crate) fn tile(
    ui: &mut egui::Ui,
    t: &Tokens,
    (glyph, name, line): (&str, &str, &str),
    on: bool,
    locked: bool,
    dim: bool,
    size: egui::Vec2,
) -> bool {
    let locked = locked || dim;
    let sense = if locked {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (rect, r) = ui.allocate_exact_size(size, sense);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, !locked, on, name));
    let ctx = ui.ctx();
    let v = ctx.animate_bool_with_easing(r.id.with("on"), on, egui::emath::easing::cubic_out);
    let hover = ctx.animate_bool_with_time(r.id.with("h"), r.hovered() && !locked, quick(ui, 0.1));
    let p = ui.painter();
    let edge = if on {
        Stroke::new(2.0, t.accent)
    } else {
        Stroke::new(1.0, mix(t.line, t.border, hover))
    };
    p.rect(
        rect,
        RADIUS,
        mix(t.surface, t.hover, hover),
        edge,
        StrokeKind::Inside,
    );
    ring(ui, &r, rect, RADIUS, t);
    let tick = rect.right_top() + vec2(-14.0, 14.0);
    if !dim {
        p.circle_stroke(tick, 6.5, Stroke::new(1.0, mix(t.border, t.accent, v)));
    }
    if v > 0.01 {
        p.circle_filled(tick, 6.5 * v, t.accent);
        icon(ui, tick, "\u{E73E}", 8.0 * v, t.on_accent);
    }
    let glyph_ink = if dim {
        t.border
    } else {
        mix(t.muted, t.accent, v)
    };
    icon(
        ui,
        pos2(rect.left() + 22.0, rect.center().y),
        glyph,
        20.0,
        glyph_ink,
    );
    let x = rect.left() + 42.0;
    let name_ink = if dim { t.muted } else { t.ink };
    put(
        ui,
        pos2(x, rect.center().y - 18.0),
        Align2::LEFT_TOP,
        txt(name, 13.0, 600.0, name_ink),
        rect.right() - x - 24.0,
    );
    put(
        ui,
        pos2(x, rect.center().y + 1.0),
        Align2::LEFT_TOP,
        txt(line, 12.0, 400.0, t.muted),
        rect.right() - x - 8.0,
    );
    let clicked = r.clicked();
    if !locked {
        r.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    clicked
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

#[cfg(test)]
mod tests {
    use super::tile_grid;

    #[test]
    fn three_or_more_across_leave_no_lone_tile() {
        for width in (120..1400).step_by(10) {
            let (cols, size) = tile_grid(width as f32, 0.0, 7);
            assert!(cols <= 2 || 7 % cols != 1, "{cols} across at {width}");
            assert!(size.x * cols as f32 <= width as f32);
        }
    }
}

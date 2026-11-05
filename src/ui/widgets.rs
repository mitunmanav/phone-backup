use crate::*;
use egui::{Pos2, pos2};

/// What a finished job says, kept together in one quiet tinted box.
pub(crate) fn result_box(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.info)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(16, 12))
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

/// A part of the page that scrolls by itself. Reaches 10 points out on both sides, so the edges of
/// chosen cards are not cut.
pub(crate) fn own_scroll(
    ui: &mut egui::Ui,
    salt: &str,
    height: f32,
    add: impl FnOnce(&mut egui::Ui),
) {
    let r = Rect::from_min_size(
        ui.cursor().min - vec2(10.0, 0.0),
        vec2(ui.available_width() + 20.0, height),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .auto_shrink(false)
            .max_height(height)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(10, 0))
                    .show(ui, add);
            });
    });
}

/// Cuts the middle out of a long name, so both of its ends still show.
pub(crate) fn cut_middle(ui: &egui::Ui, s: &str, size: f32, wght: f32, max_w: f32) -> String {
    let width = |x: &str| {
        let text = txt(x, size, wght, Color32::PLACEHOLDER);
        galley(ui, text, egui::TextWrapMode::Extend, f32::INFINITY)
            .size()
            .x
    };
    if width(s) <= max_w {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    for keep in (4..chars.len()).rev() {
        let tail = keep * 2 / 5;
        let short: String = chars[..keep - tail]
            .iter()
            .chain(['…'].iter())
            .chain(chars[chars.len() - tail..].iter())
            .collect();
        if width(&short) <= max_w {
            return short;
        }
    }
    s.to_owned()
}

/// A list of choices under a control; returns the one picked.
pub(crate) fn choices(
    r: &egui::Response,
    t: &Tokens,
    width: f32,
    options: &[String],
) -> Option<usize> {
    let frame = egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.line))
        .corner_radius(8)
        .inner_margin(4);
    egui::Popup::menu(r)
        .width(width)
        .frame(frame)
        .show(|ui| {
            ui.set_min_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut hit = None;
            for (i, o) in options.iter().enumerate() {
                let (rect, r) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 36.0), egui::Sense::click());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, o));
                if r.hovered() || r.has_focus() {
                    ui.painter().rect_filled(rect, 4, t.hover);
                }
                let shown = cut_middle(ui, o, 13.0, 400.0, rect.width() - 20.0);
                put(
                    ui,
                    pos2(rect.left() + 10.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    txt(&shown, 13.0, 400.0, t.ink),
                    rect.width() - 20.0,
                );
                let r = if shown == *o { r } else { r.on_hover_text(o) };
                if r.clicked() {
                    hit = Some(i);
                }
            }
            hit
        })?
        .inner
}

/// A name that opens a list of the others when pressed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn chooser(
    ui: &mut egui::Ui,
    t: &Tokens,
    at: Pos2,
    label: &str,
    (size, max_w): (f32, f32),
    enabled: bool,
    salt: &str,
    options: &[String],
) -> Option<usize> {
    let chev = if enabled { 16.0 } else { 0.0 };
    let shown = cut_middle(ui, label, size, 600.0, max_w - chev);
    let rect = put(
        ui,
        at,
        Align2::LEFT_TOP,
        txt(&shown, size, 600.0, t.ink),
        max_w - chev,
    );
    let hit = rect.expand2(vec2(if enabled { 18.0 } else { 0.0 }, 12.0));
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let r = ui.interact(hit, ui.scope_id().with(salt), sense);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, label));
    let r = if shown == label {
        r
    } else {
        r.on_hover_text(label)
    };
    ring(ui, &r, hit, 6, t);
    if !enabled {
        return None;
    }
    let ink = if r.hovered() { t.ink } else { t.muted };
    icon(
        ui,
        pos2(rect.right() + 10.0, rect.center().y + 1.0),
        "\u{E70D}",
        10.0,
        ink,
    );
    choices(
        &r.on_hover_cursor(egui::CursorIcon::PointingHand),
        t,
        280.0,
        options,
    )
}

/// On or off. Paints 20 tall, answers across 44.
pub(crate) fn switch(ui: &mut egui::Ui, t: &Tokens, on: bool, label: &str) -> bool {
    let (track, _) = ui.allocate_exact_size(vec2(36.0, 20.0), egui::Sense::hover());
    let r = ui.interact(
        track.expand(12.0),
        ui.scope_id().with(label),
        egui::Sense::click(),
    );
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, label));
    let v = ui
        .ctx()
        .animate_bool_with_easing(r.id, on, egui::emath::easing::cubic_out);
    let p = ui.painter();
    p.rect(
        track,
        10,
        mix(t.surface, t.accent, v),
        Stroke::new(1.0, mix(t.border, t.accent, v)),
        StrokeKind::Inside,
    );
    let x = track.left() + 10.0 + (track.width() - 20.0) * v;
    p.circle_filled(pos2(x, track.center().y), 6.0, mix(t.muted, t.on_accent, v));
    ring(ui, &r, track, 10, t);
    let clicked = r.clicked();
    r.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// A plate's quiet frame.
pub(crate) fn plate_frame(t: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(1.0, t.line))
        .corner_radius(RADIUS)
        .inner_margin(egui::Margin::symmetric(14, 10))
}

/// A setting: a picture and its words, with the control beside them, or under them when narrow.
pub(crate) fn plate(
    ui: &mut egui::Ui,
    t: &Tokens,
    glyph: &str,
    title: &str,
    note: &str,
    add: impl FnOnce(&mut egui::Ui),
) {
    let words = |ui: &mut egui::Ui, wrap: bool| {
        let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::hover());
        icon(ui, pos2(r.left() + 10.0, r.center().y), glyph, 20.0, t.ink);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let line = |text: RichText| {
                let label = egui::Label::new(text).selectable(false);
                if wrap { label.wrap() } else { label.truncate() }
            };
            ui.add(line(txt(title, 13.0, 400.0, t.ink)));
            if !note.is_empty() {
                ui.add(line(txt(note, 12.0, 400.0, t.muted)));
            }
        });
    };
    plate_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if ui.available_width() < 400.0 {
            ui.horizontal_top(|ui| words(ui, true));
            ui.add_space(6.0);
            add(ui);
        } else {
            ui.horizontal(|ui| {
                ui.set_min_height(44.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    add(ui);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| words(ui, false));
                });
            });
        }
    });
    ui.add_space(6.0);
}

/// Numbered steps, each wrapped line lined up under its first.
pub(crate) fn steps(ui: &mut egui::Ui, t: &Tokens, list: &[&str]) {
    let gap = std::mem::replace(&mut ui.spacing_mut().item_spacing.y, 9.0);
    for (n, step) in list.iter().enumerate() {
        ui.horizontal_top(|ui| {
            // Fixed-width number column so every line of a step starts at the same x.
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 20.0), egui::Sense::hover());
            put(
                ui,
                r.min,
                Align2::LEFT_TOP,
                txt(&format!("{}.", n + 1), 13.0, 600.0, t.accent),
                22.0,
            );
            ui.add(egui::Label::new(txt(step, 13.0, 400.0, t.ink)).wrap());
        });
    }
    ui.spacing_mut().item_spacing.y = gap;
}

// Controls paint at BUTTON height but answer clicks across HIT, so rows stay light and targets large.
pub(crate) const HIT: f32 = 44.0;
const BUTTON: f32 = 38.0;
pub(crate) const RADIUS: u8 = 6;
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

/// A loose cable: `n` points on a string held at `a` and `b`, pushed by `push`, with `grab` holding
/// one point somewhere else. Repaints only while it moves.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rope(
    ctx: &egui::Context,
    id: egui::Id,
    n: usize,
    a: Pos2,
    b: Pos2,
    slack: f32,
    push: egui::Vec2,
    grab: Option<(usize, Pos2)>,
) -> Vec<Pos2> {
    let (mut pts, mut prev) = ctx
        .data(|d| d.get_temp::<(Vec<Pos2>, Vec<Pos2>)>(id))
        .filter(|r| r.0.len() == n)
        .unwrap_or_else(|| {
            let line: Vec<Pos2> = (0..n)
                .map(|i| a + (b - a) * (i as f32 / (n - 1) as f32))
                .collect();
            (line.clone(), line)
        });
    let seg = a.distance(b) * slack / (n - 1) as f32;
    let steps = ((ctx.input(|i| i.stable_dt) * 120.0).ceil() as usize).clamp(1, 4);
    let mut moved = 0.0f32;
    for _ in 0..steps {
        for i in 0..n {
            let v = (pts[i] - prev[i]) * 0.975;
            prev[i] = pts[i];
            pts[i] += v + push;
            moved = moved.max(v.length());
        }
        for _ in 0..14 {
            pts[0] = a;
            pts[n - 1] = b;
            if let Some((i, at)) = grab {
                pts[i] = at;
            }
            for i in 0..n - 1 {
                let d = pts[i + 1] - pts[i];
                let len = d.length().max(1e-3);
                if len > seg {
                    let fix = d * ((len - seg) / len * 0.5);
                    pts[i] += fix;
                    pts[i + 1] -= fix;
                }
            }
        }
        pts[0] = a;
        pts[n - 1] = b;
    }
    if moved > 0.03 || grab.is_some() {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(id, (pts.clone(), prev)));
    pts
}

/// The app's mark, a phone with a copper cable: RGBA pixels and the side length.
pub(crate) fn logo_pixels() -> (Vec<u8>, usize) {
    let n = 64usize;
    let (tile, copper, white) = ([0x0B, 0x6E, 0x63], [0xE3, 0x9C, 0x66], [255, 255, 255]);
    let round = |x: f32, y: f32, l: f32, top: f32, r: f32, b: f32, rad: f32| {
        let dx = (l + rad - x).max(x - (r - rad)).max(0.0);
        let dy = (top + rad - y).max(y - (b - rad)).max(0.0);
        x >= l && x <= r && y >= top && y <= b && dx * dx + dy * dy <= rad * rad
    };
    let cable: Vec<Pos2> = (0..=40)
        .map(|i| {
            let s = i as f32 / 40.0;
            let m = 1.0 - s;
            let at = |a: f32, b: f32, c: f32, d: f32| {
                m * m * m * a + 3.0 * m * m * s * b + 3.0 * m * s * s * c + s * s * s * d
            };
            pos2(at(13.0, 13.0, 26.0, 26.0), at(24.0, 31.0, 31.0, 14.0))
        })
        .collect();
    let mut rgba = Vec::with_capacity(n * n * 4);
    for py in 0..n {
        for px in 0..n {
            let (mut sum, mut hits) = ([0u32; 3], 0u32);
            for sy in 0..3 {
                for sx in 0..3 {
                    let x = (px as f32 + (sx as f32 + 0.5) / 3.0) / 2.0;
                    let y = (py as f32 + (sy as f32 + 0.5) / 3.0) / 2.0;
                    if !round(x, y, 0.0, 0.0, 32.0, 32.0, 7.0) {
                        continue;
                    }
                    let on_cable = cable.iter().any(|c| c.distance_sq(pos2(x, y)) < 1.7);
                    let c = if round(x, y, 9.0, 6.5, 17.0, 21.0, 1.0) {
                        tile
                    } else if round(x, y, 7.0, 4.0, 19.0, 24.0, 2.5) {
                        white
                    } else if on_cable || round(x, y, 24.0, 8.0, 28.0, 14.0, 0.8) {
                        copper
                    } else {
                        tile
                    };
                    for i in 0..3 {
                        sum[i] += c[i] as u32;
                    }
                    hits += 1;
                }
            }
            let d = hits.max(1);
            rgba.extend_from_slice(&[
                (sum[0] / d) as u8,
                (sum[1] / d) as u8,
                (sum[2] / d) as u8,
                (hits * 255 / 9) as u8,
            ]);
        }
    }
    (rgba, n)
}

/// egui draws no focus for a hand-made widget.
pub(crate) fn ring(ui: &egui::Ui, r: &egui::Response, rect: Rect, radius: u8, t: &Tokens) {
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

/// Words that act: for ways back and ways out, where a button would shout.
pub(crate) fn link(ui: &mut egui::Ui, t: &Tokens, label: &str) -> bool {
    let g = galley(
        ui,
        txt(label, 13.0, 500.0, t.accent),
        egui::TextWrapMode::Extend,
        f32::INFINITY,
    );
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x, HIT), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, label));
    let top = rect.center() - g.size() / 2.0;
    let under = top.y + g.size().y;
    ui.painter()
        .galley_with_override_text_color(top, g, t.accent);
    if r.hovered() {
        ui.painter()
            .hline(rect.x_range(), under, Stroke::new(1.0, t.accent));
    }
    ring(ui, &r, rect.shrink2(vec2(0.0, 8.0)), 2, t);
    let clicked = r.clicked();
    r.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// A small labelled button that stays lit when chosen.
pub(crate) fn chip(
    ui: &mut egui::Ui,
    t: &Tokens,
    glyph: &str,
    label: &str,
    on: bool,
    min_w: f32,
) -> bool {
    let weight = if on { 600.0 } else { 400.0 };
    let g = galley(
        ui,
        txt(label, 13.0, weight, t.ink),
        egui::TextWrapMode::Extend,
        f32::INFINITY,
    );
    let pad = if glyph.is_empty() { 28.0 } else { 54.0 };
    let width = (g.size().x + pad).max(min_w);
    let (rect, r) = ui.allocate_exact_size(vec2(width, HIT), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, label));
    let face = rect.shrink2(vec2(0.0, 3.0));
    let edge = if on {
        Stroke::new(2.0, t.accent)
    } else if r.hovered() {
        Stroke::new(1.0, t.border)
    } else {
        Stroke::new(1.0, t.line)
    };
    ui.painter()
        .rect(face, RADIUS, t.surface, edge, StrokeKind::Inside);
    if !glyph.is_empty() {
        let ink = if on { t.accent } else { t.muted };
        let at = pos2(rect.left() + 22.0, rect.center().y);
        icon(ui, at, glyph, 16.0, ink);
    }
    ui.painter().galley(
        pos2(rect.left() + pad - 14.0, rect.center().y - g.size().y / 2.0),
        g,
        Color32::PLACEHOLDER,
    );
    ring(ui, &r, face, RADIUS, t);
    let clicked = r.clicked();
    r.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// Picture, heading and text in cards of equal height per row, as many across as fit.
pub(crate) fn info_cards(ui: &mut egui::Ui, t: &Tokens, parts: &[(&str, &str, &str)]) {
    const GAP: f32 = 12.0;
    let w = ui.available_width();
    let cols = (((w + GAP) / (300.0 + GAP)) as usize).clamp(1, 3);
    let card = (w - GAP * (cols as f32 - 1.0)) / cols as f32;
    for row in parts.chunks(cols) {
        let bodies: Vec<_> = row
            .iter()
            .map(|p| {
                let body = txt(p.2, 13.0, 400.0, t.ink);
                galley(ui, body, egui::TextWrapMode::Wrap, card - 32.0)
            })
            .collect();
        let h = bodies.iter().map(|g| g.size().y).fold(0.0, f32::max) + 66.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for (p, g) in row.iter().zip(bodies) {
                let (r, _) = ui.allocate_exact_size(vec2(card, h), egui::Sense::hover());
                let edge = Stroke::new(1.0, t.line);
                ui.painter()
                    .rect(r, RADIUS, t.surface, edge, StrokeKind::Inside);
                icon(ui, r.min + vec2(28.0, 28.0), p.0, 22.0, t.accent);
                put(
                    ui,
                    pos2(r.left() + 48.0, r.top() + 28.0),
                    Align2::LEFT_CENTER,
                    txt(p.1, 13.0, 600.0, t.ink),
                    card - 60.0,
                );
                ui.painter()
                    .galley(r.min + vec2(16.0, 50.0), g, Color32::PLACEHOLDER);
            }
        });
        ui.add_space(GAP - 3.0);
    }
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
    use super::{logo_pixels, tile_grid};

    #[test]
    fn logo_is_opaque_inside_and_clear_at_the_corners() {
        let (rgba, n) = logo_pixels();
        assert_eq!(rgba.len(), n * n * 4);
        assert_eq!(rgba[3], 0);
        assert_eq!(rgba[(n * n / 2 + n / 2) * 4 + 3], 255);
    }

    #[test]
    fn three_or_more_across_leave_no_lone_tile() {
        for width in (120..1400).step_by(10) {
            let (cols, size) = tile_grid(width as f32, 0.0, 7);
            assert!(cols <= 2 || 7 % cols != 1, "{cols} across at {width}");
            assert!(size.x * cols as f32 <= width as f32);
        }
    }
}

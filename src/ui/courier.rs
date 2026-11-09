//! A small pixel courier waits by a plugged-in phone, then carries boxes along the cable to
//! shelves in the PC while a backup runs, one shelf per tile. Drawn in whole pixels, scaled up
//! with no smoothing.
use super::backup_tab::Phase;
use crate::*;

const HEAD: [&str; 5] = ["..CCCC..", "..CCCCCC", "..SSSS..", "..SSKS..", "..SSSS.."];
const FRONT: [&str; 5] = ["..CCCC..", ".CCCCCC.", "..SSSS..", "..KSSK..", "..SSSS.."];
const BODY: [&str; 3] = [".JJJJJJ.", "SJJJJJJS", "..JJJJ.."];
const ARMS_UP: [&str; 3] = [".JJJJJJ.", ".JJJJJJ.", "..JJJJ.."];
const ARM_OUT: [&str; 3] = [".JJJJJJ.", "SJJJJJJ.", "..JJJJ.."];
const LEGS: [[&str; 2]; 4] = [
    ["..P..P..", "..F..F.."],
    [".P....P.", "FF....FF"],
    ["...PP...", "...FF..."],
    ["..P..P..", "..F...F."],
];
/// Per tile: the carried load's width and height, then one box on the shelf. Videos are crates.
const LOAD: [[i32; 4]; 7] = [
    [5, 4, 3, 3],
    [8, 7, 6, 5],
    [5, 4, 3, 3],
    [5, 5, 3, 4],
    [6, 5, 4, 4],
    [6, 5, 4, 4],
    [6, 3, 4, 3],
];

/// What the courier is doing, kept between frames.
#[derive(Clone, Default)]
struct Walk {
    started: bool,
    x: f32,
    carry: bool,
    /// The tile of the load he carries.
    k: usize,
    wait: f32,
    flip: bool,
    run: f32,
    /// Shelf fill when he picked up his load; the shelves show it once he drops it.
    load: [f32; 7],
    shelf: [f32; 7],
    /// Seconds since the backup finished.
    since: f32,
    /// A reaction to clicks: 0 wave, 1 jump, 2 dizzy, and its age.
    react: Option<(u8, f32)>,
    clicks: Vec<f64>,
    /// After a reaction he hurries for a moment.
    boost: f32,
}

impl App {
    /// How full each tile's shelf is, from what the backup reports.
    fn shelves(&self, phase: Phase) -> [f32; 7] {
        // As the tiles: File transfer mode can only reach files.
        let (adb, mtp) = self.connected();
        let mtp_only = adb.is_empty() && mtp.is_some();
        std::array::from_fn(|k| {
            let on = match k {
                5 => !mtp_only && self.pick.apps,
                6 => !mtp_only && self.pick.data,
                k => self.pick.files && self.pick.kinds[k],
            };
            match (phase, self.tiles) {
                // Counts left over from a restore or transfer are not this backup's.
                (Phase::Idle, _) => 0.0,
                (Phase::Done(_), _) => on as u8 as f32,
                (_, Some((done, total))) if k < 6 && total[k] > 0 => {
                    (done[k] as f32 / total[k] as f32).min(1.0)
                }
                _ => 0.0,
            }
        })
    }

    /// The courier scene in `area`: waiting by the phone, or a backup running, stopped or finished.
    pub(crate) fn courier(&self, ui: &egui::Ui, t: &Tokens, area: Rect, phase: Phase) {
        let ctx = ui.ctx().clone();
        let id = egui::Id::unique("courier");
        let dark = ui.visuals().dark_mode;
        let hex = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
        let skin = hex(0xE8B996);
        let ink = |c: char| match c {
            'C' => t.accent,
            'S' => skin,
            'K' => hex(0x141414),
            'J' => hex(if dark { 0x5B6272 } else { 0x2F3440 }),
            'P' => hex(if dark { 0x3E424C } else { 0x4A4D55 }),
            _ => hex(if dark { 0x9A9A9A } else { 0x141414 }),
        };

        // About 40 pixels tall at any size, and at least 100 wide.
        let ps = (area.height() / 36.0)
            .min(area.width() / 100.0)
            .floor()
            .clamp(2.0, 6.0);
        let (w, h) = ((area.width() / ps) as i32, (area.height() / ps) as i32);
        let o = area.min.round();
        let p = ui.painter();
        let px = |x: i32, y: i32, ww: i32, hh: i32, c: Color32| {
            let at = o + vec2(x as f32, y as f32) * ps;
            p.rect_filled(
                Rect::from_min_size(at, vec2(ww as f32, hh as f32) * ps),
                0,
                c,
            );
        };
        let sprite = |rows: &[&str], x: i32, y: i32, flip: bool| {
            for (j, row) in rows.iter().enumerate() {
                for (i, c) in row.chars().enumerate() {
                    let i = if flip { row.len() - 1 - i } else { i };
                    if c != '.' {
                        px(x + i as i32, y + j as i32, 1, 1, ink(c));
                    }
                }
            }
        };

        // Ground, phone, cable and the PC with one shelf per tile.
        let gy = h - 2;
        let (ph_w, ph_h) = (12, (gy - 4).min(22));
        let py = gy - ph_h;
        let rw = 30;
        let rx = w - rw - 1;
        let rack_top = 4;
        let sh = ((gy - rack_top) / 7).max(3);
        let (c0, c1) = (ph_w, rx - 2);
        px(0, gy, w, 1, t.line);
        px(0, py, ph_w, ph_h, t.surface);
        px(0, py, ph_w, 1, t.border);
        px(0, py, 1, ph_h, t.border);
        px(ph_w - 1, py, 1, ph_h, t.border);
        px(c0, gy - 1, c1 - c0, 1, t.border);
        px(c0, gy - 2, 2, 2, t.accent);
        px(c1 - 2, gy - 2, 2, 2, t.accent);
        px(rx - 1, rack_top - 4, rw + 2, gy - rack_top + 4, t.line);
        px(rx - 1, rack_top - 4, rw + 2, 1, t.border);

        let f = self.shelves(phase);
        let mut s: Walk = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
        let (home, shelf) = (c0 as f32 + 1.0, (rx - 10) as f32);
        // A new backup: shelves emptier than last shown.
        if !s.started || f.iter().zip(&s.shelf).any(|(a, b)| a + 0.01 < *b) {
            s = Walk {
                started: true,
                x: home,
                load: f,
                shelf: f,
                ..Walk::default()
            };
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let now = ctx.input(|i| i.time);
        let motion = self.motion;
        let running = phase == Phase::Running;
        let paused = running && self.paused_at.is_some();
        // The tile being copied: the first one not yet full, then contacts.
        let active = self
            .tiles
            .and_then(|(done, total)| (0..6).find(|&k| done[k] < total[k]))
            .unwrap_or(6);

        // Click him: once he waves, twice he jumps, four times he gets dizzy.
        let hit = Rect::from_min_size(
            o + vec2((s.x - 1.0) * ps - 12.0, (gy - 18) as f32 * ps - 12.0),
            vec2(10.0 * ps + 24.0, 19.0 * ps + 24.0),
        );
        let mut hover = false;
        if motion {
            let r = ui
                .interact(hit, id.with("hit"), egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, "Wave to the courier")
            });
            hover = r.hovered();
            if r.clicked() {
                s.clicks.retain(|c| now - c < 1.2);
                s.clicks.push(now);
                s.react = match s.clicks.len() {
                    1 => Some((0, 0.0)),
                    2 => Some((1, 0.0)),
                    n if n >= 4 => {
                        s.clicks.clear();
                        Some((2, 0.0))
                    }
                    _ => s.react,
                };
            }
        }
        if let Some((kind, age)) = &mut s.react {
            *age += dt;
            if *age >= [1.0, 0.5, 1.4][*kind as usize] {
                s.react = None;
                s.boost = 2.0;
            }
        }
        s.boost = (s.boost - dt).max(0.0);
        s.since = if matches!(phase, Phase::Done(_)) {
            s.since + dt
        } else {
            0.0
        };

        let busy = s.react.is_some() && running;
        // A trip takes at most 2.5 seconds, so wide windows do not leave the shelves far behind.
        let v = (26.0f32).max((shelf - home) / 2.5)
            * if hover { 0.3 } else { 1.0 }
            * if s.boost > 0.0 { 1.6 } else { 1.0 };
        let (mut frame, mut hop, mut front, mut wave, mut scratch) = (0, 0.0, hover, false, false);
        match phase {
            Phase::Running if paused => {
                // Waits at the phone, tapping a foot.
                s.carry = false;
                s.flip = true;
                s.x += (home - s.x) * (dt * 3.0).min(1.0);
                if motion {
                    frame = if (now * 3.0) as i64 % 2 == 1 { 3 } else { 0 };
                }
            }
            Phase::Running if !motion => {
                s.x = shelf;
                s.carry = false;
            }
            Phase::Running if !busy => {
                if s.wait > 0.0 {
                    s.wait -= dt;
                } else if s.carry {
                    s.x += v * dt;
                    s.flip = false;
                    if s.x >= shelf {
                        (s.x, s.carry, s.wait, s.shelf) = (shelf, false, 0.25, s.load);
                    }
                } else {
                    s.x -= v * 1.4 * dt;
                    s.flip = true;
                    if s.x <= home {
                        (s.x, s.carry, s.k, s.load, s.wait) = (home, true, active, f, 0.25);
                    }
                }
                if s.wait <= 0.0 {
                    s.run += dt * v * 0.3;
                    frame = [1, 0, 2, 0][s.run as usize % 4];
                }
            }
            Phase::Done(n) => {
                s.carry = false;
                s.flip = false;
                s.x += (shelf - s.x) * (dt * 4.0).min(1.0);
                front = true;
                // Two hops and a wave, or a scratch of the head when something went wrong.
                if motion && n > 0 {
                    scratch = s.since < 2.0 && (s.since * 6.0) as i32 % 2 == 0;
                } else if motion && s.since < 1.2 {
                    hop = (s.since * std::f32::consts::PI / 0.6).sin().abs() * 4.0;
                    wave = (s.since * 5.0) as i32 % 2 == 0;
                }
            }
            Phase::Stopped => {
                s.carry = false;
                front = true;
            }
            _ => {}
        }
        let mut flip = s.flip;
        let mut stars = None;
        match s.react {
            Some((0, age)) => wave = (age * 5.0) as i32 % 2 == 0,
            Some((1, age)) => hop = (std::f32::consts::PI * age / 0.5).sin() * 7.0,
            Some((_, age)) => {
                stars = (age < 1.0).then_some(age);
                flip = (age * if age < 1.0 { 8.0 } else { 14.0 }) as i32 % 2 == 0;
                front = age < 1.0;
            }
            None => {}
        }

        // The shelves show what he has delivered; with motion off, the true amounts.
        let shown = if motion && running { s.shelf } else { f };
        for (k, fill) in shown.iter().enumerate() {
            let sy = rack_top + k as i32 * sh;
            let [_, _, bw, bh] = LOAD[k];
            let bh = bh.min(sh - 1);
            px(rx, sy + sh - 1, rw, 1, mix(t.line, t.border, 0.5));
            let slots = if k == 6 { 1 } else { (rw - 2) / (bw + 1) };
            for i in 0..slots {
                let a = (fill * slots as f32 - i as f32).clamp(0.0, 1.0);
                if a > 0.0 {
                    let (bx, by) = (rx + 1 + i * (bw + 1), sy + sh - 1 - bh);
                    px(bx, by, bw, bh, t.accent.gamma_multiply(0.4 + 0.6 * a));
                    px(bx, by + 1, bw, 1, t.glint.gamma_multiply(0.6));
                }
            }
        }

        // The phone's screen: lines while copying, then a tick, or ! when something went wrong.
        let copying = running && !paused;
        let screen = if copying {
            t.accent.gamma_multiply(0.2)
        } else {
            t.bg
        };
        px(2, py + 3, ph_w - 4, ph_h - 6, screen);
        match phase {
            _ if copying => {
                let tick = if motion { (now * 4.0) as i32 } else { 0 };
                for r in 0..(ph_h - 8) / 3 {
                    let len = ((r * 3 + tick) % 4) + 3;
                    px(3, py + 5 + r * 3, len, 1, t.accent.gamma_multiply(0.7));
                }
            }
            Phase::Done(0) => {
                let mid = py + ph_h / 2;
                for (x, y) in [(2, 0), (3, 1), (4, 2), (5, 1), (6, 0), (7, -1), (8, -2)] {
                    px(x, mid + y, 1, 1, t.accent);
                }
            }
            Phase::Done(_) => {
                let mid = py + ph_h / 2;
                px(5, mid - 4, 2, 5, t.error);
                px(5, mid + 2, 2, 2, t.error);
            }
            _ => {}
        }

        // The courier himself.
        let cx = s.x.round() as i32;
        let top = (gy as f32 - hop).round() as i32 - 10;
        let arms: &[&str] = if s.carry {
            &ARMS_UP
        } else if wave || scratch {
            &ARM_OUT
        } else {
            &BODY
        };
        sprite(if front { &FRONT } else { &HEAD }, cx, top, flip);
        sprite(arms, cx, top + 5, flip);
        sprite(&LEGS[frame], cx, top + 8, flip);
        let side = if flip { cx - 1 } else { cx + 8 };
        if wave {
            px(side, top + 3, 1, 2, skin);
        }
        if scratch {
            px(side, top + 1, 1, 3, skin);
        }
        let mut lift = 0;
        if s.carry {
            let [lw, lh, ..] = LOAD[s.k];
            let bob = if frame == 2 { -1 } else { 0 };
            let (bx, by) = (cx + 4 - lw / 2, top - 1 - lh + bob);
            px(cx, top - 1, 1, 1, skin);
            px(cx + 7, top - 1, 1, 1, skin);
            px(bx, by, lw, lh, t.accent);
            px(bx, by + lh / 2, lw, 1, t.glint.gamma_multiply(0.8));
            lift = lh + 1;
        }
        if let Some(age) = stars {
            for i in 0..3 {
                let a = age * 9.0 + i as f32 * 2.1;
                let (x, y) = (a.cos() * 5.0, a.sin() * 1.5);
                px(
                    cx + 3 + x.round() as i32,
                    top - 3 - lift + y.round() as i32,
                    1,
                    1,
                    t.glint,
                );
            }
        }

        let moving = motion && (copying || s.react.is_some() || s.since < 2.0);
        if moving {
            ctx.request_repaint();
        } else if running {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        ctx.data_mut(|d| d.insert_temp(id, s));
    }
}

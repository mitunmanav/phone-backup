use crate::*;
use egui::{Pos2, pos2};

const ICON_FOLDER: &str = "\u{E8B7}";
/// Glyph, name and a note, for `KINDS` in order and then Apps and Contacts. The phone holds its
/// contacts, texts, calls and calendar behind one switch, so they share a tile.
const TILES: [(&str, &str, &str); 7] = [
    ("\u{E91B}", KINDS[0], "All pictures"),
    ("\u{E714}", KINDS[1], "Clips"),
    ("\u{E8D6}", KINDS[2], "Music, voice"),
    ("\u{E8A5}", KINDS[3], "PDFs, Office files"),
    ("\u{E8B7}", KINDS[4], "Everything else"),
    ("\u{E71D}", "Apps", "Without logins"),
    ("\u{E77B}", "Contacts", "Texts, calls, more"),
];

/// How the running or last backup stands; `Done` holds how many things went wrong.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    Idle,
    Running,
    Done(usize),
    Stopped,
}

/// One job's cable: what it joins and how that job stands.
pub(crate) struct Cord<'a> {
    pub(crate) id: &'a str,
    pub(crate) a: Pos2,
    pub(crate) b: Pos2,
    /// Where the phone's end hangs with no phone.
    pub(crate) loose: Pos2,
    pub(crate) sag: f32,
    pub(crate) phase: Phase,
    pub(crate) connected: bool,
    /// The phone is at `b`, for a copy onto a phone.
    pub(crate) phone_at_b: bool,
    /// Stands upright as a coiled cord that can be pulled around inside this box.
    pub(crate) coil: Option<Rect>,
}

/// Files that could not be copied, and what the phone refused to give.
pub(crate) fn problems(done: &[Summary]) -> usize {
    done.iter()
        .map(|s| s.errors.len() + s.data.iter().filter(|d| !d.1).count())
        .sum()
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.into()
    } else {
        format!("{word}s")
    }
}

/// A finished job's title when `n` things went wrong.
pub(crate) fn finished_with(n: usize) -> String {
    format!("Finished with {n} {}", plural(n, "problem"))
}

/// A tile's word while its backup runs, or after it stopped: bytes copied of bytes to copy.
fn tile_progress(done: u64, total: u64, stopped: bool) -> String {
    // None on the phone, or all of it already in the backup.
    if total == 0 {
        "Nothing new".into()
    } else if done >= total {
        "Copied".into()
    } else if done > 0 {
        format!("{} of {}", size(done), size(total))
    } else if stopped {
        "Not copied".into()
    } else {
        "Waiting".into()
    }
}

/// Switches one type of file. With files off altogether, turning one on means only that one.
fn toggle_kind(pick: &mut Pick, k: usize) {
    if !pick.files {
        pick.files = true;
        pick.kinds = [false; 5];
    }
    pick.kinds[k] = !pick.kinds[k];
}

impl App {
    /// A few seconds in, so the first batch does not give a wild guess.
    pub(crate) fn time_left_now(&self) -> Option<String> {
        if self.paused_at.is_some() {
            return Some("Paused".into());
        }
        let spent = self.progress_from.elapsed().as_secs_f32();
        // shortcut: estimate by file count, not bytes; switch to bytes if users see bad estimates
        self.fraction
            .filter(|f| *f > 0.0 && spent > 5.0)
            .map(|f| time_left((spent * (1.0 - f) / f) as u64))
    }

    /// How the running or last backup stands.
    fn phase(&self) -> Phase {
        match &self.result {
            _ if self.busy.load(Ordering::Relaxed) => Phase::Running,
            None => Phase::Idle,
            Some(Ok(done)) => Phase::Done(problems(done)),
            Some(Err(_)) => Phase::Stopped,
        }
    }

    /// Draws a cable; the copy travels a to b. Returns how plugged in and how sealed it looks.
    pub(crate) fn cable(&self, ui: &egui::Ui, t: &Tokens, cord: Cord) -> (f32, f32) {
        let Cord {
            id,
            a,
            b,
            loose,
            sag,
            phase,
            connected,
            phone_at_b,
            coil,
        } = cord;
        let vertical = coil.is_some();
        let ctx = ui.ctx().clone();
        let key = |word: &str| egui::Id::unique((word, id));
        // The coil can be pulled over the tiles, so it paints above them.
        let above = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, key("layer")));
        let p = if vertical { &above } else { ui.painter() };
        let secs = |s: f32| if self.motion { s } else { 0.0 };
        let running = phase == Phase::Running;
        let connected = (connected || running) as u8 as f32;
        let plug = if self.motion {
            spring(&ctx, key("plug"), connected, 0.45)
        } else {
            connected
        };
        let axis = if vertical {
            vec2(0.0, 1.0)
        } else {
            vec2(1.0, 0.0)
        };
        let side = vec2(axis.y, axis.x) * sag;
        let (p0, p3) = if phone_at_b {
            (a, loose + (b - loose) * plug)
        } else {
            (loose + (a - loose) * plug, b)
        };
        let span = (p3 - p0).dot(axis);
        let slack = 0.35 + 0.65 * plug;
        let c1 = p0 + axis * span * 0.33 + side * if phone_at_b { 1.0 } else { slack };
        let c2 = p3 - axis * span * 0.33 + side * if phone_at_b { slack } else { 1.0 };
        let measure = |pts: &[Pos2]| {
            let mut len = vec![0.0f32];
            for w in pts.windows(2) {
                len.push(len[len.len() - 1] + w[0].distance(w[1]));
            }
            len
        };
        let walk = |pts: &[Pos2], len: &[f32], d: f32| {
            let i = len.partition_point(|l| *l < d).clamp(1, pts.len() - 1);
            let s = (d - len[i - 1]) / (len[i] - len[i - 1]).max(1e-3);
            pts[i - 1] + (pts[i] - pts[i - 1]) * s.clamp(0.0, 1.0)
        };
        let mut pts = egui::epaint::CubicBezierShape::from_points_stroke(
            [p0, c1, c2, p3],
            false,
            Color32::TRANSPARENT,
            Stroke::NONE,
        )
        .flatten(Some(0.15));
        let mut len = measure(&pts);
        // A coiled phone cord you can pull: drag it anywhere, let go and it springs back.
        if let Some(bounds) = coil {
            let rope_id = key("coil");
            let was: Option<Vec<Pos2>> = ctx
                .data(|d| d.get_temp::<(Vec<Pos2>, Vec<Pos2>)>(rope_id))
                .map(|r| r.0);
            let mut zone = Rect::from_two_pos(p0, p3).expand2(vec2(30.0, 0.0));
            if let Some(w) = &was {
                zone = zone.union(Rect::from_points(w).expand(14.0));
            }
            let mut grab = None;
            if self.motion {
                let r = ui.interact(zone, rope_id.with("drag"), egui::Sense::drag());
                let near = |q: Pos2| {
                    was.as_ref()
                        .and_then(|w| {
                            w.iter()
                                .enumerate()
                                .map(|(i, c)| (i, c.distance(q)))
                                .min_by(|a, b| a.1.total_cmp(&b.1))
                        })
                        .filter(|m| m.1 < 18.0)
                };
                let held_id = rope_id.with("held");
                if r.drag_started()
                    && let Some(m) = r.interact_pointer_pos().and_then(near)
                {
                    ctx.data_mut(|d| d.insert_temp(held_id, m.0));
                }
                if !r.dragged() {
                    ctx.data_mut(|d| d.remove::<usize>(held_id));
                }
                let held = ctx.data(|d| d.get_temp::<usize>(held_id));
                if let (Some(i), Some(q)) = (held, r.interact_pointer_pos()) {
                    grab = Some((i, bounds.shrink(10.0).clamp(q)));
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if r.hover_pos().and_then(near).is_some() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
            }
            let line = if self.motion {
                rope(&ctx, rope_id, 28, p0, p3, 1.01, vec2(0.03, 0.0), grab)
            } else {
                pts.clone()
            };
            let line_len = measure(&line);
            let reach = line_len[line_len.len() - 1];
            // A fixed number of turns: pulling spreads the coils and thins them.
            let rest = p0.distance(p3).max(1.0);
            let turns = rest * 0.3;
            let thin = (rest / reach.max(1.0)).clamp(0.35, 1.0);
            let dots = ((reach / 1.2) as usize).max(2);
            pts = (0..=dots)
                .map(|i| {
                    let d = reach * i as f32 / dots as f32;
                    let turn = d / reach.max(1.0) * turns;
                    let fade = (d.min(reach - d) / 20.0).min(1.0);
                    walk(&line, &line_len, d)
                        + vec2(turn.sin() * 7.0, turn.cos() * 3.5) * fade * thin
                })
                .collect();
            len = measure(&pts);
        }
        let total = len[len.len() - 1];
        let at = |d: f32| walk(&pts, &len, d);
        let wire = if vertical { 2.0 } else { 3.0 };
        p.add(egui::Shape::line(pts.clone(), Stroke::new(wire, t.border)));

        // The copied part, lit as far as the copy has really gone.
        let (target, known) = match phase {
            Phase::Running => (self.fraction.unwrap_or(0.0), self.fraction.is_some()),
            Phase::Stopped => (self.stopped_at.unwrap_or(0.0), false),
            Phase::Done(_) => (1.0, false),
            Phase::Idle => (0.0, false),
        };
        let fill = ctx.animate_value_with_time(key("fill"), target, secs(0.35));
        let reach = total * fill;
        let done = matches!(phase, Phase::Done(_));
        let sweep = ctx.animate_bool_with_time(key("sweep"), done, secs(0.9));
        let along = move |q: Pos2| if vertical { q.y } else { q.x };
        if fill > 0.001 {
            let sweep_x = if done && self.motion && sweep < 1.0 {
                along(p0) - 80.0 + (span + 160.0) * sweep
            } else {
                f32::MIN
            };
            let (accent, glint) = (t.accent, t.glint);
            let width = if vertical { 3.0 } else { 4.0 };
            let lit = egui::epaint::PathStroke::new_uv(width, move |_, pos| {
                let d = along(pos) - sweep_x + 40.0;
                let w = if (0.0..80.0).contains(&d) {
                    (std::f32::consts::PI * d / 80.0).sin().powi(2)
                } else {
                    0.0
                };
                mix(accent, glint, w)
            });
            let step = if vertical { 1.2 } else { 3.0 };
            let steps = ((reach / step) as usize).max(2);
            let dense: Vec<Pos2> = (0..=steps)
                .map(|i| at(reach * i as f32 / steps as f32))
                .collect();
            p.add(egui::Shape::line(dense, lit));
        }
        if running && self.motion {
            // Files riding the cable; across all of it while the amount is not known yet.
            let reach = if known { reach } else { total };
            let gap = 64.0;
            let mut d = (ctx.input(|i| i.time) as f32 * 120.0).rem_euclid(gap);
            while d < reach {
                let fade = (d / 24.0).min(1.0) * ((reach - d) / 24.0).min(1.0);
                p.rect(
                    Rect::from_center_size(at(d), vec2(8.0, 10.0)),
                    2,
                    t.glint.gamma_multiply(fade),
                    Stroke::new(1.5, t.bg.gamma_multiply(fade)),
                    StrokeKind::Outside,
                );
                d += gap;
            }
        }
        if known {
            let head = at(reach);
            p.add(
                egui::Shadow {
                    offset: [0, 0],
                    blur: 14,
                    spread: 1,
                    color: t.accent.gamma_multiply(0.5),
                }
                .as_shape(
                    Rect::from_center_size(head, egui::Vec2::splat(10.0)),
                    egui::CornerRadius::same(5),
                ),
            );
            p.circle_filled(head, 5.0, t.accent);
            p.circle_stroke(head, 5.0, Stroke::new(1.5, t.bg));
        }
        if running {
            ctx.request_repaint_after(Duration::from_millis(if self.motion { 16 } else { 500 }));
        }

        // The tick lands only when nothing went wrong.
        let seal = ctx.animate_bool_with_easing(
            key("seal"),
            phase == Phase::Done(0),
            egui::emath::easing::cubic_out,
        );
        let body = mix(t.border, t.ink, 0.55);
        let settle = |d: egui::Vec2| (d + (axis - d) * plug).normalized();
        let (d0, d3) = if phone_at_b {
            (axis, settle((p3 - c2).normalized()))
        } else {
            (settle((c1 - p0).normalized()), axis)
        };
        connector(p, p0, -d0, body, t.border);
        connector(p, p3, d3, mix(body, t.accent, seal), t.border);
        (plug, seal)
    }

    /// Percent or outcome for a copy onto a phone, at `mid`, with its note at `sub` when given.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mid_words(
        &self,
        ui: &egui::Ui,
        t: &Tokens,
        (mid, sub, align): (Pos2, Option<Pos2>, Align2),
        size: f32,
        phase: Phase,
        done_word: &str,
        unplugged: bool,
    ) {
        let note = |text: &str| {
            if let Some(at) = sub {
                put(ui, at, align, txt(text, 12.0, 400.0, t.muted), 160.0);
            }
        };
        match phase {
            Phase::Idle => {}
            Phase::Running => {
                let percent = format!("{:.0}%", (self.fraction.unwrap_or(0.0) * 100.0).floor());
                tabular(ui, mid, align, &percent, |s| txt(s, size, 600.0, t.ink));
                note(&self.time_left_now().unwrap_or_default());
            }
            Phase::Stopped => {
                put(ui, mid, align, txt("Stopped", size, 600.0, t.ink), 150.0);
                if unplugged {
                    note("Phone unplugged");
                }
            }
            Phase::Done(_) => {
                put(ui, mid, align, txt(done_word, size, 600.0, t.accent), 150.0);
            }
        }
    }

    /// The cable across the top of the page and the words under its ends, all from real state.
    fn hero(&mut self, ui: &mut egui::Ui, t: &Tokens, height: f32) {
        // Half widths of the two glyphs at 36 points, so they line up with the words under them.
        const PHONE: f32 = 11.0;
        const FOLDER: f32 = 18.0;
        let w = ui.available_width();
        let narrow = w < 560.0;
        let (rect, response) = ui.allocate_exact_size(vec2(w, height), egui::Sense::hover());
        let (lx, rx) = (rect.left(), rect.right());
        let by = rect.bottom() - if narrow { 42.0 } else { 46.0 };
        let y = rect.top() + (by - rect.top() - 4.0) * 0.5;
        let phone = pos2(lx + PHONE, y);
        let folder = pos2(rx - FOLDER, y);
        let a = pos2(phone.x + PHONE + 16.0, y);
        let b = pos2(folder.x - FOLDER - 16.0, y);
        let loose = pos2(b.x - (b.x - a.x) * 0.42, y + (by - y - 22.0).max(16.0));
        let sag = if narrow { 16.0 } else { 30.0 };
        let (adb, mtp) = self.connected();
        let phase = self.phase();
        let connected = !adb.is_empty() || mtp.is_some();
        // The courier waits by a plugged-in phone and carries the backup; with no phone, the
        // cable's loose plug asks for one.
        if (self.job == Job::Backup && phase != Phase::Idle) || (phase == Phase::Idle && connected)
        {
            let area = Rect::from_min_max(rect.min, pos2(rx, by - 8.0));
            self.courier(ui, t, area, phase);
        } else {
            let (plug, seal) = self.cable(
                ui,
                t,
                Cord {
                    id: "backup",
                    a,
                    b,
                    loose,
                    sag,
                    phase,
                    connected,
                    phone_at_b: false,
                    coil: None,
                },
            );
            let shown = self.adb_phone().map(|p| p.0);
            let (glyph, size) = self.device_icon(shown.as_deref(), 36.0);
            icon(ui, phone, glyph, size, mix(t.muted, t.ink, plug));
            icon(ui, folder, ICON_FOLDER, 36.0, mix(t.ink, t.accent, seal));
            badge(ui, t, folder + vec2(FOLDER - 3.0, 17.0), seal);
        }

        // What each end says.
        if self.free.as_ref().is_none_or(|(d, _)| *d != self.dest) {
            self.free = Some((self.dest.clone(), backup::free_on(&self.dest)));
        }
        let free = self.free.as_ref().and_then(|f| f.1);
        let free_words = free.map_or(String::new(), |f| format!("{} free on this PC", size(f)));
        let dest_name = self.dest.file_name().map_or_else(
            || self.dest.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        let allow = self
            .phones
            .as_ref()
            .is_some_and(|p| p.contains(&Phone::Unauthorized));
        let (name, how) = match (adb.as_slice(), mtp) {
            ([(model, _)], _) => (
                model.clone(),
                self.adb_phone()
                    .map_or(String::new(), |p| self.phone_note(&p.0)),
            ),
            ([_, ..], _) => (
                format!("{} phones", adb.len()),
                adb.iter()
                    .map(|(m, _)| m.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            (_, Some(name)) => (name, "File transfer mode".to_string()),
            _ if phase == Phase::Running => ("Your phone".to_string(), String::new()),
            _ if self.phones.is_none() => ("Looking for your phone".to_string(), String::new()),
            _ if allow => (
                "Tap Allow on your phone".to_string(),
                "Unlock it to see the question".to_string(),
            ),
            _ => (
                "No phone yet".to_string(),
                "Plug it in with a USB cable and unlock it".to_string(),
            ),
        };
        let percent = self
            .fraction
            .map(|f| format!("{:.0}%", (f * 100.0).floor()));
        let (word, word_ink, word_note) = match phase {
            Phase::Idle => (None, t.ink, String::new()),
            Phase::Running => (percent, t.ink, self.time_left_now().unwrap_or_default()),
            Phase::Stopped => (Some("Stopped".to_string()), t.ink, String::new()),
            Phase::Done(n) => {
                let copied: usize = match &self.result {
                    Some(Ok(done)) => done.iter().map(|s| s.copied).sum(),
                    _ => 0,
                };
                let note = if copied == 0 && n == 0 {
                    "Nothing new to copy".to_string()
                } else {
                    format!("{} copied", files(copied))
                };
                if n == 0 {
                    (Some("Backed up".to_string()), t.accent, note)
                } else {
                    (Some("Finished".to_string()), t.warning, note)
                }
            }
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Image,
                true,
                format!(
                    "{name}. {how}. {}. {word_note}. Saves to {dest_name}. {free_words}.",
                    word.as_deref().unwrap_or_default()
                ),
            )
        });

        let (title_size, sub_y) = if narrow {
            (15.0, by + 22.0)
        } else {
            (18.0, by + 26.0)
        };
        let title = |s: &str| txt(s, title_size, 600.0, t.ink);
        let sub = |s: &str| txt(s, 12.0, 400.0, t.muted);
        let left_top = Align2::LEFT_TOP;
        let full = rx - lx;
        if narrow {
            // Room for one end only: the folder's words join the phone's while idle.
            let line = match phase {
                Phase::Idle if adb.is_empty() && !how.is_empty() => how.clone(),
                Phase::Idle if free_words.is_empty() => format!("To {dest_name}"),
                Phase::Idle => format!("To {dest_name}, {free_words}"),
                _ => word_note.clone(),
            };
            let name_max = if word.is_some() { full - 118.0 } else { full };
            put(ui, pos2(lx, by), left_top, title(&name), name_max);
            put(ui, pos2(lx, sub_y), left_top, sub(&line), full);
        } else {
            let right_max = full / 2.0 - 80.0;
            let left_max = if word.is_some() {
                right_max
            } else {
                full * 0.6
            };
            put(ui, pos2(lx, by), left_top, title(&name), left_max);
            put(ui, pos2(lx, sub_y), left_top, sub(&how), left_max);
            // Dropped 4 points so its baseline meets the larger name.
            let shown = put(
                ui,
                pos2(rx, by + 4.0),
                Align2::RIGHT_TOP,
                txt(&dest_name, 13.0, 600.0, t.ink),
                right_max,
            );
            ui.interact(shown, ui.scope_id().with("dest"), egui::Sense::hover())
                .on_hover_text(self.dest.display().to_string());
            put(
                ui,
                pos2(rx, sub_y),
                Align2::RIGHT_TOP,
                sub(&free_words),
                right_max,
            );
        }
        // Percent or outcome: in the middle, or at the right when narrow.
        let (mid, mid_align) = if narrow {
            (rx, Align2::RIGHT_TOP)
        } else {
            (rect.center().x, Align2::CENTER_TOP)
        };
        if let Some(word) = &word {
            tabular(ui, pos2(mid, by), mid_align, word, |s| {
                txt(s, title_size, 600.0, word_ink)
            });
        }
        if !narrow {
            put(ui, pos2(mid, sub_y), mid_align, sub(&word_note), 160.0);
        }
    }

    pub(crate) fn backup_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let (adb, mtp) = self.connected();
        let ready = !adb.is_empty() || mtp.is_some();
        // File transfer mode can only reach files.
        let mtp_only = adb.is_empty() && mtp.is_some();
        if !adb.is_empty() && !busy {
            self.measure(ui.ctx());
        }
        // Sizes are of the first phone: with more, they would say the wrong thing.
        let measured = if adb.len() == 1 {
            self.measured().map(|(apps, t)| (apps, t.kinds, t.total))
        } else {
            None
        };

        // The cable takes what the tiles leave, so it grows with the window. The same height with
        // or without a phone, so plugging one in moves nothing.
        let width = ui.available_width();
        let (cols, small) = tile_grid(width, 0.0, TILES.len());
        let block = TILES.len().div_ceil(cols) as f32 * (small.y + TILE_GAP);
        // Everything else the first screen must show: page margin, gap, the row above the tiles,
        // and any battery warnings and limits.
        let low = self
            .phones
            .iter()
            .flatten()
            .filter(|p| {
                matches!(
                    p,
                    Phone::Adb {
                        battery: Some(_),
                        ..
                    }
                )
            })
            .count() as f32;
        let limits_row = if self.pick.limits().is_empty() {
            0.0
        } else {
            24.0
        };
        // The body starts below the page heading, which this leaves out.
        let rest = ui.clip_rect().bottom()
            - ui.cursor().top()
            - 14.0
            - 8.0
            - HIT
            - 3.0 * ui.spacing().item_spacing.y
            - low * 28.0
            - limits_row;
        let min = if width < 560.0 { 136.0 } else { 148.0 };
        let hero = (rest - block).clamp(min, 300.0);
        self.hero(ui, t, hero);
        ui.add_space(8.0);
        self.battery_warnings(ui, t);

        let phase = self.phase();
        if ready {
            ui.horizontal(|ui| {
                ui.set_min_height(HIT);
                let heading = match phase {
                    Phase::Running => "Backing up",
                    Phase::Done(_) => "What was backed up",
                    Phase::Stopped => "What was copied",
                    Phase::Idle => "What to back up",
                };
                ui.label(txt(heading, 13.0, 600.0, t.ink));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if phase == Phase::Idle && quiet(ui, t, "Choose folders…", true).clicked() {
                        self.choosing = true;
                    }
                });
            });
            self.tiles(ui, t, rest - hero, phase, mtp_only, measured);
            let limits = self.pick.limits();
            if !limits.is_empty() {
                ui.add(egui::Label::new(txt(&limits, 12.0, 400.0, t.muted)).wrap());
            }
            if adb.len() > 1 {
                ui.label(muted("Each phone gets its own backup folder."));
            }
            if mtp_only {
                ui.label(muted(if self.adb_ok {
                    "Turn on USB debugging to include apps, contacts and messages."
                } else {
                    "USB debugging also needs adb, Google's free tool."
                }));
                self.connect_help(ui, t, false);
            }
        } else if let Some(phones) = &self.phones {
            let (allow, bad_usb) = (
                phones.contains(&Phone::Unauthorized),
                phones.contains(&Phone::UsbFailed),
            );
            if allow {
                ui.add(
                    egui::Label::new(
                        "Unlock the phone. It asks whether to allow USB debugging from this computer.",
                    )
                    .wrap(),
                );
            } else {
                ui.horizontal(|ui| {
                    ui.set_min_height(HIT - 14.0);
                    ui.label(txt("Connect your phone", 13.0, 600.0, t.ink));
                });
                if bad_usb {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Windows found a USB device it cannot use. If that is your phone, unplug it, unlock it and plug it in again. Still nothing? Try another cable and USB port: some cables only charge.",
                            )
                            .color(t.error),
                        )
                        .wrap(),
                    );
                }
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
                    ui.label(muted("USB debugging also needs adb, Google's free tool."));
                }
            }
            ui.add_space(10.0);
            self.connect_help(ui, t, true);
        }

        let files_on = self.pick.files && self.pick.kinds.contains(&true);
        let apps_on = !mtp_only && self.pick.apps;
        let picked = files_on || apps_on || (!mtp_only && self.pick.data);
        let name = match (adb.as_slice(), mtp) {
            ([(model, _)], _) => model.clone(),
            ([_, ..], _) => format!("{} phones", adb.len()),
            (_, Some(name)) => name,
            _ => String::new(),
        };
        // Words only, never a block: a repeat backup copies just what changed, and the backup
        // itself checks the exact room it needs.
        let needs = measured.map(|(apps, _, total)| {
            let bytes = if files_on { total } else { 0 } + if apps_on { apps } else { 0 };
            match self.free.as_ref().and_then(|f| f.1) {
                Some(free) if bytes > free => {
                    format!("Needs up to {}, only {} free", size(bytes), size(free))
                }
                _ => format!("About {}", size(bytes)),
            }
        });
        let (title, note) = match phase {
            Phase::Running => (
                self.status.clone(),
                match self.now.rsplit(['/', '\\']).next() {
                    Some(file) if !file.is_empty() => file.to_string(),
                    _ => "Keep the phone plugged in and unlocked.".into(),
                },
            ),
            Phase::Done(0) => ("Backup finished".into(), "No errors. Report saved.".into()),
            Phase::Done(n) => (finished_with(n), "The list is in the report.".into()),
            Phase::Stopped => (
                self.stopped_at.map_or("Backup stopped".into(), |f| {
                    format!("Stopped at {:.0}%", (f * 100.0).floor())
                }),
                "Copied files are safe.".into(),
            ),
            Phase::Idle if !ready => ("Connect a phone to start".into(), String::new()),
            Phase::Idle if !picked => ("Pick something to back up".into(), String::new()),
            Phase::Idle if adb.len() > 1 => (
                format!("Ready to back up {name}"),
                needs.unwrap_or_default(),
            ),
            Phase::Idle => ("Ready to back up".into(), needs.unwrap_or_default()),
        };
        let note = if !ready && phase == Phase::Stopped {
            "Connect a phone to try again.".into()
        } else {
            note
        };
        let open = match (phase, &self.result) {
            (Phase::Done(_), Some(Ok(done))) => Some(match done.as_slice() {
                [one] => one.root.clone(),
                _ => self.dest.clone(),
            }),
            _ => None,
        };
        Cmd {
            title,
            note,
            label: match phase {
                Phase::Done(_) => "Open folder",
                Phase::Stopped => "Try again",
                _ => "Back up now",
            },
            enabled: open.is_some() || ready && picked,
            plan: None,
            open,
            again: matches!(phase, Phase::Done(_) | Phase::Stopped),
        }
    }

    /// How to get a phone connected: the help page, and Wi-Fi when `wifi`. Without adb, the way
    /// to get it comes first.
    pub(crate) fn connect_help(&mut self, ui: &mut egui::Ui, t: &Tokens, wifi: bool) {
        ui.horizontal_wrapped(|ui| {
            if !self.adb_ok && quiet(ui, t, "Get adb in Settings", true).clicked() {
                self.go(Tab::Settings);
            }
            if quiet(ui, t, "How to turn on USB debugging", true).clicked() {
                self.help_from = self.tab;
                self.go(Tab::Help);
            }
            if wifi && quiet(ui, t, "Connect over Wi-Fi", true).clicked() {
                self.wifi_open = true;
            }
        });
    }

    /// What a backup can hold, as tiles filling the width and any spare height.
    fn tiles(
        &mut self,
        ui: &mut egui::Ui,
        t: &Tokens,
        room: f32,
        phase: Phase,
        mtp_only: bool,
        measured: Option<(u64, [u64; 5], u64)>,
    ) {
        let (cols, each) = tile_grid(ui.available_width(), room, TILES.len());
        let full = ui.available_width();
        let sized = |bytes: u64| match bytes {
            0 => "None found".to_string(),
            n => size(n),
        };
        let stopped = phase == Phase::Stopped;
        let progress = self
            .tiles
            .filter(|_| self.job == Job::Backup && (phase == Phase::Running || stopped));
        for (row, chunk) in TILES.chunks(cols).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = TILE_GAP;
                for (k, (glyph, name, note)) in chunk.iter().enumerate() {
                    let i = row * cols + k;
                    let dim = i >= KINDS.len() && mtp_only;
                    let on = match i {
                        5 => !dim && self.pick.apps,
                        6 => !dim && self.pick.data,
                        k => self.pick.files && self.pick.kinds[k],
                    };
                    let line = match (i, measured, progress) {
                        _ if dim => "Not in this mode".to_string(),
                        _ if on && phase == Phase::Done(0) => "Copied".to_string(),
                        (0..6, _, Some((done, total))) if on => {
                            tile_progress(done[i], total[i], stopped)
                        }
                        // Contacts and messages are saved once the files are in.
                        (6, _, Some((done, total))) if on => match stopped {
                            true => "Not copied".to_string(),
                            false if done == total => "Saving".to_string(),
                            false => "Waiting".to_string(),
                        },
                        (0..5, Some((_, kinds, _)), _) => sized(kinds[i]),
                        (5, Some((apps, ..)), _) => sized(apps),
                        _ => (*note).to_string(),
                    };
                    // A lone tile on the last row takes the whole row.
                    let this = if chunk.len() == 1 {
                        vec2(full, each.y)
                    } else {
                        each
                    };
                    let item = (*glyph, *name, line.as_str());
                    if tile(ui, t, item, on, phase != Phase::Idle, dim, this) {
                        match i {
                            5 => self.pick.apps = !self.pick.apps,
                            6 => self.pick.data = !self.pick.data,
                            k => toggle_kind(&mut self.pick, k),
                        }
                    }
                }
            });
            if (row + 1) * cols < TILES.len() {
                ui.add_space(TILE_GAP - ui.spacing().item_spacing.y);
            }
        }
    }

    /// The first USB-debugging phone's serial and model.
    pub(crate) fn adb_phone(&self) -> Option<(String, String)> {
        self.phones.iter().flatten().find_map(|p| match p {
            Phone::Adb { serial, model, .. } => Some((serial.clone(), model.clone())),
            _ => None,
        })
    }

    /// Reads the sizes on the phone in the background, unless they are known or being read.
    pub(crate) fn measure(&mut self, ctx: &egui::Context) {
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
    pub(crate) fn measured(&mut self) -> Option<(u64, &Tally)> {
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
    pub(crate) fn choose_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
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
    pub(crate) fn backup_result(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let muted = |s: &str| RichText::new(s).color(t.muted);
        match &self.result {
            Some(Err(e)) => {
                ui.add_space(16.0);
                result_box(t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new("Backup stopped")
                            .font(semibold(16.0))
                            .color(t.error),
                    );
                    // Each error says what to do; not every one is the phone's fault.
                    ui.label(e);
                });
            }
            Some(Ok(summaries)) => {
                for s in summaries {
                    // A clean backup has said all it needs in the command bar.
                    let clean = s.errors.is_empty()
                        && s.data.iter().all(|d| d.1)
                        && s.notes.is_empty()
                        && s.full
                        && s.files > 0;
                    if clean {
                        continue;
                    }
                    ui.add_space(16.0);
                    result_box(t).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let clean = problems(std::slice::from_ref(s)) == 0;
                        let title = match (clean, summaries.len() > 1) {
                            (true, false) => "Backup finished".to_string(),
                            (false, false) => "Backup finished with problems".to_string(),
                            (true, true) => format!("{}: backup finished", s.name),
                            (false, true) => format!("{}: backup finished with problems", s.name),
                        };
                        let ink = if clean { t.ink } else { t.warning };
                        ui.label(RichText::new(title).font(semibold(16.0)).color(ink));
                        let in_backup = files(s.files - s.errors.len().min(s.files));
                        if s.pick.files || s.pick.apps {
                            ui.label(if s.files == 0 {
                                "No files were found on the phone.".to_string()
                            } else if s.copied == 0 && s.errors.is_empty() {
                                format!("Nothing new to copy. The backup already holds {in_backup}.")
                            } else {
                                format!("Copied {} that {} new or changed. The backup now holds {in_backup}.", files(s.copied), backup::were(s.copied))
                            });
                        }
                        if !s.errors.is_empty() {
                            ui.label(
                                RichText::new(format!("{} could not be copied. Back up again to retry them. The list is in report.txt.", files(s.errors.len())))
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
                            for note in &s.notes {
                                ui.label(note);
                            }
                        } else {
                            ui.label("Apps, contacts and messages were not saved. Turn on USB debugging to include them.");
                        }
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if quiet(ui, t, "Open backup folder", true).clicked() {
                                let _ = backup::explorer().arg(&s.root).spawn();
                            }
                            if quiet(ui, t, "Open report", true).clicked() {
                                let _ = backup::explorer().arg(s.root.join(backup::REPORT)).spawn();
                            }
                            // Copying shows nothing by itself, so the button says it worked for 2 s.
                            let id = egui::Id::unique("copied").with(&s.root);
                            let now = ui.input(|i| i.time);
                            let copied = ui.ctx().data(|d| d.get_temp::<f64>(id)).is_some_and(|at| now - at < 2.0);
                            let label = if copied { "Copied" } else { "Copy problem list" };
                            if !s.errors.is_empty() && quiet(ui, t, label, true).clicked() {
                                ui.ctx().copy_text(s.errors.join("\n"));
                                ui.ctx().data_mut(|d| d.insert_temp(id, now));
                                ui.ctx().request_repaint_after(Duration::from_secs(2));
                            }
                        });
                    });
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_copy_errors_and_refused_data_as_problems() {
        let summary = |errors: usize, data| Summary {
            name: "Phone".into(),
            root: PathBuf::new(),
            files: 9,
            copied: 9,
            errors: vec!["x".into(); errors],
            data,
            notes: vec![],
            full: true,
            pick: Pick::default(),
        };
        assert_eq!(problems(&[summary(0, vec![("Contacts", true)])]), 0);
        assert_eq!(
            problems(&[
                summary(2, vec![("Contacts", true), ("Messages", false)]),
                summary(1, vec![]),
            ]),
            4
        );
    }

    #[test]
    fn tile_progress_follows_the_backup() {
        assert_eq!(tile_progress(0, 0, false), "Nothing new");
        assert_eq!(tile_progress(0, 5, false), "Waiting");
        assert_eq!(tile_progress(0, 5, true), "Not copied");
        assert_eq!(tile_progress(5, 5, true), "Copied");
        assert!(tile_progress(2_000_000_000, 5_000_000_000, false).contains(" of "));
    }

    #[test]
    fn first_tile_after_files_off_means_only_that_kind() {
        let mut pick = Pick {
            files: false,
            kinds: [true; 5],
            ..Pick::default()
        };
        toggle_kind(&mut pick, 1);
        assert!(pick.files);
        assert_eq!(pick.kinds, [false, true, false, false, false]);
        toggle_kind(&mut pick, 1);
        assert!(pick.files && !pick.kinds.contains(&true));
    }
}

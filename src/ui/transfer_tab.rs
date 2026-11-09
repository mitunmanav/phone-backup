use super::backup_tab::{Cord, Phase, finished_with};
use crate::*;
use egui::Pos2;

const ICON_PHONE: &str = "\u{E8EA}";
const ICON_FOLDER: &str = "\u{E8B7}";

impl App {
    /// How the running or last transfer stands. Its check is a real backup, so it counts as running.
    fn transfer_phase(&self) -> Phase {
        if self.busy.load(Ordering::Relaxed) && self.job == Job::Restore {
            Phase::Running
        } else {
            self.restore_phase()
        }
    }

    /// Phone to phone: which phone to copy from and to, and what to copy.
    pub(crate) fn transfer_tab(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Cmd {
        let busy = self.busy.load(Ordering::Relaxed);
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
        let names: Vec<String> = adb
            .iter()
            .map(|(s, m)| match adb.iter().filter(|p| p.1 == *m).count() {
                1 => m.clone(),
                _ => format!("{m} ({s})"),
            })
            .collect();
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
        let phase = self.transfer_phase();

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

        let top = ui.cursor().min;
        let height = (ui.clip_rect().bottom() - top.y - 14.0).max(300.0);
        let area = Rect::from_min_size(top, vec2(ui.available_width(), height));
        ui.allocate_rect(area, egui::Sense::hover());
        let (rail, list) = if area.width() >= 640.0 {
            let rail = Rect::from_min_size(
                area.min,
                // Capped so a tall window does not stretch the cable into a long empty card.
                vec2((area.width() * 0.34).clamp(282.0, 452.0), height.min(520.0)),
            );
            (
                rail,
                Rect::from_min_max(egui::pos2(rail.right() + 32.0, area.top()), area.max),
            )
        } else {
            // Short windows give the choices below more room.
            let rail = Rect::from_min_size(
                area.min,
                vec2(area.width(), (height * 0.55).clamp(182.0, 222.0)),
            );
            (
                rail,
                Rect::from_min_max(egui::pos2(area.left(), rail.bottom() + 8.0), area.max),
            )
        };
        self.transfer_rail(ui, t, rail, &adb, &names, phase);
        let room = list.height() - HIT - 130.0;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            own_scroll(ui, "transfer", list.height(), |ui| {
                let folder = plan.as_ref().map(|p| p.1.clone());
                self.transfer_body(ui, t, room, phase, ready, folder);
                self.restore_result(ui, t, plan.as_ref());
            });
        });

        let name = |serial: &Option<String>| {
            adb.iter()
                .position(|p| Some(&p.0) == serial.as_ref())
                .map_or(String::new(), |i| names[i].clone())
        };
        let (from_name, to_name) = (name(&self.from), name(&self.to));
        let checked = matches!(self.preview_for(plan.as_ref()), Some(Ok(_)));
        let (title, note): (String, String) = match phase {
            Phase::Running if self.checking.is_some() => (
                format!("Backing up {from_name} and checking…"),
                "Keep both phones plugged in and unlocked.".into(),
            ),
            Phase::Running => (
                format!("Copying to {to_name}…"),
                "Keep both phones plugged in and unlocked.".into(),
            ),
            Phase::Done(n) if n > 0 => (finished_with(n), "The report says what failed.".into()),
            Phase::Done(_) => (
                "Transfer finished".into(),
                "No errors. Report saved.".into(),
            ),
            Phase::Stopped => (
                self.stopped_at.map_or("Transfer stopped".into(), |f| {
                    format!("Stopped at {:.0}%", (f * 100.0).floor())
                }),
                if ready {
                    "Copied files are safe.".into()
                } else {
                    "Connect two phones to try again.".into()
                },
            ),
            Phase::Idle if !ready => ("Connect two phones to start".into(), String::new()),
            Phase::Idle if !picked => ("Pick something to copy".into(), String::new()),
            Phase::Idle if checked => ("Ready to transfer".into(), "Nothing changes yet.".into()),
            Phase::Idle => (
                "Ready to check".into(),
                "Backs up the first phone. Changes nothing on either.".into(),
            ),
        };
        let open = match phase {
            Phase::Done(_) => self.restore_report.clone(),
            _ => None,
        };
        Cmd {
            title,
            note,
            label: match phase {
                Phase::Done(_) => "Open report",
                Phase::Stopped => "Try again",
                _ if checked => "Transfer now",
                _ => "Check transfer",
            },
            enabled: !busy
                && (open.is_some() || (ready && picked && !matches!(phase, Phase::Done(_)))),
            plan: plan.map(|p| (p, !checked)),
            open,
            again: matches!(phase, Phase::Done(_) | Phase::Stopped),
        }
    }

    /// The cable stands upright: one phone above, one below, with the copy falling from the first
    /// to the second.
    fn transfer_rail(
        &mut self,
        ui: &mut egui::Ui,
        t: &Tokens,
        r: Rect,
        adb: &[(String, String)],
        names: &[String],
        phase: Phase,
    ) {
        let ready = adb.len() >= 2;
        let free = ready && phase == Phase::Idle;
        // A faint frame makes the phones and cable one object; Swap sits across its edge, so it
        // belongs to both.
        let frame = Rect::from_min_max(r.min, egui::pos2(r.right() - 32.0, r.bottom()));
        ui.painter().rect(
            frame,
            RADIUS,
            mix(t.bg, t.surface, 0.4),
            Stroke::new(1.0, mix(t.bg, t.line, 0.7)),
            StrokeKind::Inside,
        );
        let r = frame.shrink2(vec2(10.0, 6.0));
        let x = r.left() + 11.0;
        // One phone in: it is the first, so the second's end of the cable hangs loose with a hint
        // under it, which needs a line of room inside the frame.
        let one = !ready && !adb.is_empty();
        let (top, bot) = (
            egui::pos2(x, r.top() + 24.0),
            egui::pos2(x, r.bottom() - if one { 38.0 } else { 22.0 }),
        );
        let (a, b) = (top + vec2(0.0, 34.0), bot - vec2(0.0, 34.0));
        let loose = egui::pos2(x + 24.0, a.y + (b.y - a.y) * 0.42);
        let (plug, seal) = self.cable(
            ui,
            t,
            Cord {
                id: "transfer",
                a,
                b,
                loose,
                sag: 14.0,
                phase,
                connected: ready,
                phone_at_b: one,
                coil: Some(frame),
            },
        );
        let ink = mix(t.muted, t.ink, plug);
        icon(ui, top, ICON_PHONE, 36.0, ink);
        icon(ui, bot, ICON_PHONE, 36.0, mix(ink, t.accent, seal));
        badge(ui, t, bot + vec2(12.0, 17.0), seal);
        let tx = r.left() + 38.0;
        let max = r.width() - 38.0;
        let small = |at: Pos2, s: &str| {
            put(ui, at, Align2::LEFT_TOP, txt(s, 12.0, 400.0, t.muted), max);
        };
        small(egui::pos2(tx, top.y - 19.0), "Copy from");
        small(egui::pos2(tx, bot.y - 19.0), "Copy to");
        let at = |y: f32| egui::pos2(tx, y - 3.0);
        if !ready {
            let first = names.first().map_or("First phone", |n| n.as_str());
            let big = |y: f32, s: &str| {
                put(ui, at(y), Align2::LEFT_TOP, txt(s, 15.0, 600.0, t.ink), max);
            };
            big(top.y, first);
            big(bot.y, "Second phone");
            if !one {
                small(egui::pos2(tx, top.y + 20.0), "Tap Allow on each phone");
            } else {
                small(
                    egui::pos2(tx, bot.y + 20.0),
                    "Plug it in with USB debugging on",
                );
            }
            return;
        }
        let shown = |s: &Option<String>| {
            adb.iter()
                .position(|p| Some(&p.0) == s.as_ref())
                .map_or("", |i| names[i].as_str())
        };
        let first = chooser(
            ui,
            t,
            at(top.y),
            shown(&self.from),
            (15.0, max),
            free,
            "from",
            names,
        );
        let second = chooser(
            ui,
            t,
            at(bot.y),
            shown(&self.to),
            (15.0, max),
            free,
            "to",
            names,
        );
        // Choosing the other phone on one side swaps the two.
        if let Some(n) = first {
            let new = Some(adb[n].0.clone());
            if new == self.to {
                std::mem::swap(&mut self.from, &mut self.to);
            } else {
                self.from = new;
            }
        }
        if let Some(n) = second {
            let new = Some(adb[n].0.clone());
            if new == self.from {
                std::mem::swap(&mut self.from, &mut self.to);
            } else {
                self.to = new;
            }
        }
        let midy = (a.y + b.y) / 2.0;
        if free {
            let swap = Rect::from_center_size(egui::pos2(frame.right(), midy), vec2(72.0, HIT));
            let hit = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(swap)
                        .layout(Layout::top_down(Align::Center)),
                    |ui| quiet(ui, t, "Swap", true),
                )
                .inner;
            if hit.clicked() {
                std::mem::swap(&mut self.from, &mut self.to);
            }
        }
        // A fallen plug hangs further out, so the words move clear of it; so does a tall cable's
        // lean, plus its coil width and a gap.
        let lean = (b.y - a.y) * 0.07 + 4.0;
        let out = if phase == Phase::Stopped { 32.0 } else { 8.0 };
        let mid = (tx + out).max(x + lean + 15.0);
        let words = (
            egui::pos2(mid, midy - 28.0),
            Some(egui::pos2(mid, midy + 8.0)),
            Align2::LEFT_TOP,
        );
        self.mid_words(ui, t, words, 22.0, phase, "Copied", !ready);
    }

    fn transfer_body(
        &mut self,
        ui: &mut egui::Ui,
        t: &Tokens,
        room: f32,
        phase: Phase,
        ready: bool,
        folder: Option<PathBuf>,
    ) {
        let locked = phase != Phase::Idle;
        self.battery_warnings(ui, t);
        if !ready && !locked {
            self.connect_help(ui, t, true);
        }
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, HIT), egui::Sense::hover());
        put(
            ui,
            egui::pos2(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            txt("What to copy", 13.0, 600.0, t.ink),
            w,
        );
        let (files_line, label) = self.picked_words(folder.as_ref());
        self.onto_tiles(ui, t, room, &files_line, (true, true), locked, phase);
        ui.add_space(6.0);
        // The files to choose from exist once the check has backed up the first phone.
        let can = !locked && self.restore_files;
        if quiet(ui, t, &label, can && folder.is_some())
            .on_disabled_hover_text("Check the transfer first. It backs up the first phone, then its files can be chosen.")
            .clicked()
            && let Some(folder) = folder
        {
            self.open_picker(ui.ctx(), folder, false);
        }
        let label = "Back up the second phone first";
        plate(
            ui,
            t,
            ICON_FOLDER,
            label,
            "A safety copy, before anything changes on it.",
            |ui| {
                if !locked && switch(ui, t, self.safety, label) {
                    self.safety = !self.safety;
                }
            },
        );
        for fact in [
            "The first phone is backed up into its own folder, then copied to the second.",
            "Files the second phone already has are skipped, never replaced. Apps come back without their logins and data.",
            "Contacts and messages cannot be copied to a phone yet.",
        ] {
            ui.add(egui::Label::new(txt(fact, 12.0, 400.0, t.muted)).wrap());
        }
    }
}

use crate::*;

impl App {
    pub(crate) fn logo_id(&mut self, ctx: &egui::Context) -> egui::TextureId {
        self.logo
            .get_or_insert_with(|| {
                let (rgba, n) = logo_pixels(false);
                ctx.load_texture(
                    "logo",
                    egui::ColorImage::from_rgba_unmultiplied([n, n], &rgba),
                    egui::TextureOptions::LINEAR,
                )
            })
            .id()
    }

    /// The page's body. Returns what the command bar offers; Settings has none.
    pub(crate) fn content(&mut self, ui: &mut egui::Ui, t: &Tokens) -> Option<Cmd> {
        if self.tab == Tab::Settings {
            self.settings_page(ui, t);
            return None;
        }
        if self.tab == Tab::Tools {
            self.tools_page(ui, t);
            return None;
        }
        if matches!(self.tab, Tab::Privacy | Tab::License) {
            self.doc_page(ui, t);
            return None;
        }
        if self.tab == Tab::Help {
            self.help_page(ui, t);
            return None;
        }
        if self.tab == Tab::Backup {
            let cmd = self.backup_tab(ui, t);
            self.backup_result(ui, t);
            return Some(cmd);
        }
        if self.tab == Tab::Restore {
            return Some(self.restore_tab(ui, t));
        }
        Some(self.transfer_tab(ui, t))
    }

    /// 0 to 1 over `secs` since the page changed, eased out; 1 at once with animations off.
    pub(crate) fn since_switch(&self, ctx: &egui::Context, secs: f32) -> f32 {
        let p = if self.motion {
            self.tab_at.elapsed().as_secs_f32() / secs
        } else {
            1.0
        };
        if p < 1.0 {
            ctx.request_repaint();
        }
        egui::emath::easing::cubic_out(p.min(1.0))
    }

    /// Opens a page, reading the backup folder for the pages that list its backups.
    pub(crate) fn go(&mut self, tab: Tab) {
        // Messages stay in memory only while they are on screen.
        self.reading = None;
        if matches!(tab, Tab::Restore | Tab::Tools) {
            let dest = self.dest.clone();
            self.scan(&dest);
        }
        if tab == Tab::Tools {
            self.scrcpy = tools::scrcpy_path();
        }
        // These two pages share one result: it must not show up as the other's.
        // Help in between counts as the page it was opened from.
        let from = if self.tab == Tab::Help {
            self.help_from
        } else {
            self.tab
        };
        if tab != from && tab != Tab::Help && matches!(from, Tab::Restore | Tab::Transfer) {
            self.restore_msg = None;
        }
        self.tab = tab;
        self.tab_at = Instant::now();
    }

    /// The page's scrolling body, fading in when the page changes, and on Back up, Restore and
    /// Transfer the command bar.
    pub(crate) fn page(&mut self, ui: &mut egui::Ui, t: &Tokens, pad: i8) {
        let rest = ui.max_rect();
        let bar = if self.intro.is_some()
            || matches!(
                self.tab,
                Tab::Settings | Tab::Tools | Tab::Read | Tab::Privacy | Tab::License | Tab::Help
            ) {
            0.0
        } else {
            self.bar_h
        };
        let body = rest.with_max_y(rest.bottom() - bar);
        let fade = self.since_switch(ui.ctx(), 0.15);
        let cmd = ui
            .scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                ui.multiply_opacity(fade);
                // Its lists scroll by themselves, inside the height of the window.
                if self.tab == Tab::Read {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(pad, 14))
                        .show(ui, |ui| {
                            ui.set_max_width(ui.available_width().min(1040.0));
                            self.read_page(ui, t);
                        });
                    return None;
                }
                let mut area = egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .id_salt((self.tab as u8, self.intro.is_some()));
                // Held every frame: the first frames do not know the content height yet.
                if let Some(y) = self.scroll {
                    area = area.vertical_scroll_offset(y);
                }
                area.show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(pad, 14))
                        .show(ui, |ui| {
                            // Says the job in plain words, so nobody has to guess the page.
                            let head = match self.tab {
                                Tab::Backup => "Copy this phone to your PC",
                                Tab::Restore => "Put a backup back on a phone",
                                Tab::Transfer => "Copy one phone straight to another",
                                Tab::Tools => "Small jobs for your backups",
                                Tab::Settings => "Settings",
                                _ => "",
                            };
                            if self.intro.is_none() && !head.is_empty() {
                                let size = if ui.available_width() < 560.0 {
                                    17.0
                                } else {
                                    20.0
                                };
                                ui.add(egui::Label::new(txt(head, size, 600.0, t.ink)).truncate());
                            }
                            match self.intro {
                                Some(step) => {
                                    self.intro_page(ui, t, step);
                                    None
                                }
                                None => self.content(ui, t),
                            }
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

    /// The pages: a side rail when `rail`, a strip across the top when not. The page marker is the
    /// app's own cable, which swings across and plugs into the page you chose.
    pub(crate) fn nav(&mut self, ui: &mut egui::Ui, t: &Tokens, area: Rect, rail: bool) {
        const PHONE: &str = "\u{E8EA}";
        const FOLDER: &str = "\u{E8B7}";
        const NAV: [(&str, Tab, &str, &str); 5] = [
            ("Back up", Tab::Backup, PHONE, FOLDER),
            ("Restore", Tab::Restore, FOLDER, PHONE),
            ("Transfer", Tab::Transfer, PHONE, PHONE),
            ("Tools", Tab::Tools, "\u{E90F}", ""),
            ("Settings", Tab::Settings, "\u{E713}", ""),
        ];
        ui.painter().rect_filled(area, 0, t.surface);
        if rail {
            ui.painter()
                .vline(area.right(), area.y_range(), Stroke::new(1.0, t.line));
            let logo = self.logo_id(ui.ctx());
            let at = Rect::from_min_size(area.min + vec2(18.0, 16.0), egui::Vec2::splat(28.0));
            ui.painter().image(
                logo,
                at,
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            put(
                ui,
                egui::pos2(at.right() + 10.0, at.center().y),
                Align2::LEFT_CENTER,
                txt("Phone Backup", 13.0, 600.0, t.ink),
                area.width() - 66.0,
            );
        } else {
            ui.painter()
                .hline(area.x_range(), area.bottom(), Stroke::new(1.0, t.line));
        }
        // The page cannot change while a job runs: its progress and Stop are on it.
        let busy = self.busy.load(Ordering::Relaxed);
        let mut lit = area;
        let mut focused = false;
        for (i, (name, tab, a, b)) in NAV.iter().enumerate() {
            let rect = if rail {
                let h = 50.0;
                // Settings sits apart at the foot: it is not a job.
                let top = if i == 4 {
                    area.bottom() - h - 10.0
                } else {
                    area.top() + 56.0 + i as f32 * (h + 2.0)
                };
                Rect::from_min_size(
                    egui::pos2(area.left() + 6.0, top),
                    vec2(area.width() - 22.0, h),
                )
            } else {
                let w = area.width() / 5.0;
                Rect::from_min_size(area.min + vec2(i as f32 * w, 0.0), vec2(w, area.height()))
            };
            let on = self.tab == *tab
                || (*tab == Tab::Settings && matches!(self.tab, Tab::Privacy | Tab::License))
                || (*tab == Tab::Tools && self.tab == Tab::Read)
                || (*tab == self.help_from && self.tab == Tab::Help);
            let sense = if busy {
                egui::Sense::hover()
            } else {
                egui::Sense::click()
            };
            let r = ui.interact(rect, egui::Id::unique(("nav", i)), sense);
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, !busy, on, *name));
            focused |= r.has_focus();
            let hover = if busy {
                0.0
            } else {
                ui.ctx()
                    .animate_bool_with_time(r.id.with("h"), r.hovered(), 0.1)
            };
            if on {
                lit = rect;
            }
            if hover > 0.0 {
                ui.painter()
                    .rect_filled(rect, 6, t.hover.gamma_multiply(hover));
            }
            ring(ui, &r, rect, 6, t);
            let ink = if on {
                t.accent
            } else {
                mix(t.muted, t.ink, hover)
            };
            let (size, y, first, second) = if rail {
                (
                    17.0,
                    rect.top() + 17.0,
                    rect.left() + 18.0,
                    rect.left() + 52.0,
                )
            } else {
                let off = if b.is_empty() { 0.0 } else { 12.0 };
                (
                    15.0,
                    rect.top() + 16.0,
                    rect.center().x - off,
                    rect.center().x + 12.0,
                )
            };
            icon(ui, egui::pos2(first, y), a, size, ink);
            if !b.is_empty() {
                icon(ui, egui::pos2(second, y), b, size, ink);
                let mid = (first + second) / 2.0;
                // While this page's job runs, a file crosses between its two pictures.
                if on && busy && self.motion {
                    let reach = (second - first) * 0.3;
                    let x = ((ui.input(|i| i.time) as f32 * 0.9).fract() * 2.0 - 1.0) * reach;
                    ui.painter().rect_filled(
                        Rect::from_center_size(egui::pos2(mid + x, y), vec2(5.0, 6.0)),
                        1,
                        t.accent,
                    );
                    ui.ctx().request_repaint_after(Duration::from_millis(33));
                } else {
                    icon(ui, egui::pos2(mid, y), "\u{E76C}", 8.0, ink);
                }
            }
            let weight = if on { 600.0 } else { 400.0 };
            if rail {
                let text = if on {
                    t.ink
                } else {
                    mix(t.muted, t.ink, hover)
                };
                put(
                    ui,
                    egui::pos2(rect.left() + 10.0, rect.top() + 29.0),
                    Align2::LEFT_TOP,
                    txt(name, 13.0, weight, text),
                    rect.width() - 34.0,
                );
                // Every page has a socket the cable can plug into.
                ui.painter().circle_stroke(
                    egui::pos2(rect.right() - 10.0, rect.center().y),
                    4.0,
                    Stroke::new(1.0, t.border),
                );
            } else {
                let text = if on { t.ink } else { t.muted };
                put(
                    ui,
                    egui::pos2(rect.center().x, rect.top() + 27.0),
                    Align2::CENTER_TOP,
                    txt(name, 12.0, weight, text),
                    rect.width() - 2.0,
                );
            }
            if r.clicked() && !on {
                self.go(*tab);
            }
            if !busy {
                r.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }
        let ctx = ui.ctx().clone();
        // Moving by keyboard, the marker goes straight there.
        let still = !self.motion || focused;
        let glide = |id: &str, v: f32| {
            if still {
                v
            } else {
                spring(&ctx, egui::Id::unique(id), v, 0.4)
            }
        };
        let wire = Stroke::new(2.0, t.accent);
        let p = ui.painter();
        if rail {
            let target = lit.center().y;
            let y = glide("nav-y", target);
            let pull = ((y - target).abs() / 18.0).min(1.0) * 7.0;
            let socket = lit.right() - 10.0;
            let gutter = area.right() - 7.0;
            let start = egui::pos2(gutter, area.top() + 44.0);
            let end = egui::pos2(gutter, y - 7.0);
            // Straight: a loose rope here bowed over the sockets and never pulled tight again.
            let mut line = vec![start, end];
            let head = egui::pos2(socket + 13.0 + pull, y);
            // A short rounded bend from the rail's edge into the plug.
            for k in 1..=6 {
                let s = k as f32 / 6.0;
                line.push(
                    end + (egui::pos2(gutter, y) - end) * 2.0 * s * (1.0 - s)
                        + (head - end) * s * s,
                );
            }
            p.add(egui::Shape::line(line, wire));
            p.line_segment([head, head - vec2(7.0, 0.0)], Stroke::new(7.0, t.accent));
            p.line_segment(
                [head - vec2(7.0, 0.0), head - vec2(11.0, 0.0)],
                Stroke::new(3.0, t.border),
            );
            if pull < 0.5 {
                p.circle_filled(egui::pos2(socket, y), 3.0, t.accent);
            }
        } else {
            let target = lit.center().x;
            let x = glide("nav-x", target);
            let pull = ((x - target).abs() / 18.0).min(1.0) * 4.0;
            let base = area.bottom() - 1.5;
            let (start, end) = (egui::pos2(area.left(), base), egui::pos2(x, base));
            // Straight along the strip's foot: a loose rope here floated up over the page names.
            p.line_segment([start, end], wire);
            let tip = egui::pos2(x, base - 7.0 + pull);
            p.line_segment([egui::pos2(x, base), tip], Stroke::new(6.0, t.accent));
            p.line_segment([tip, tip - vec2(0.0, 3.0)], Stroke::new(3.0, t.border));
        }
    }

    /// The foot of the page: what will happen on the left, the main button (Stop while a job
    /// runs) on the right.
    pub(crate) fn command_bar(&mut self, ui: &mut egui::Ui, t: &Tokens, cmd: Cmd, pad: i8) {
        let shown = egui::Frame::new()
            .fill(t.bg)
            .inner_margin(egui::Margin::symmetric(pad, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let busy = self.busy.load(Ordering::Relaxed);
                    if busy {
                        let stopping = backup::STOP.load(Ordering::Relaxed);
                        let label = if stopping { "Stopping…" } else { "Stop" };
                        if quiet(ui, t, label, !stopping && self.job != Job::Delete).clicked() {
                            // A stray click must not end a long job: ask first.
                            self.confirm_close = true;
                            self.stop_only = true;
                        }
                        // Only copying to or from a phone waits between batches.
                        if matches!(self.job, Job::Backup | Job::Restore) && !stopping {
                            let label = if self.paused_at.is_some() {
                                "Resume"
                            } else {
                                "Pause"
                            };
                            if quiet(ui, t, label, true).clicked() {
                                match self.paused_at.take() {
                                    // Time paused does not count towards the time left.
                                    Some(at) => self.progress_from += at.elapsed(),
                                    None => self.paused_at = Some(Instant::now()),
                                }
                                backup::PAUSE.store(self.paused_at.is_some(), Ordering::Relaxed);
                            }
                        }
                    } else if primary(ui, t, cmd.label, cmd.enabled) {
                        match (cmd.open, cmd.plan) {
                            (Some(path), _) => {
                                let _ = backup::explorer().arg(path).spawn();
                            }
                            (_, plan) if self.tab == Tab::Transfer => self.transfer(ui.ctx(), plan),
                            (_, Some((plan, check))) => self.restore(ui.ctx(), plan, check),
                            _ => self.start(ui.ctx()),
                        }
                    }
                    // A finished or stopped job must not be the end of the road.
                    if cmd.again && !busy {
                        ui.add_space(4.0);
                        if link(ui, t, "Start again") {
                            if self.tab == Tab::Backup {
                                self.result = None;
                            } else {
                                self.restore_msg = None;
                                self.preview = None;
                            }
                        }
                    }
                    ui.add_space(4.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // Level with the button's centre, with or without a note.
                        ui.add_space(if cmd.note.is_empty() { 13.0 } else { 5.0 });
                        // One line each while a job runs: its words change with every file, and
                        // the bar must not jump as they do.
                        let line = |text: RichText| {
                            let label = egui::Label::new(text);
                            if busy { label.truncate() } else { label.wrap() }
                        };
                        ui.add(line(txt(&cmd.title, 13.0, 600.0, t.ink)));
                        if !cmd.note.is_empty() {
                            ui.add(line(txt(&cmd.note, 12.0, 400.0, t.muted)));
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
}

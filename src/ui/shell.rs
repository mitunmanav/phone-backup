use crate::*;

impl App {
    /// The phone beside what to do with it (stacked when narrow), then progress and results.
    /// Returns what the command bar offers; Settings has none.
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
        if self.tab == Tab::Backup {
            let cmd = self.backup_tab(ui, t);
            self.backup_result(ui, t);
            return Some(cmd);
        }
        let job = |app: &mut Self, ui: &mut egui::Ui| match app.tab {
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
        self.restore_result(ui, t, cmd.plan.as_ref().map(|p| &p.0));
        Some(cmd)
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
        if matches!(tab, Tab::Restore | Tab::Tools) {
            let dest = self.dest.clone();
            self.scan(&dest);
        }
        if tab == Tab::Tools {
            self.scrcpy = tools::scrcpy_path();
        }
        self.tab = tab;
        self.tab_at = Instant::now();
        self.pill.1 = self.pill.0;
    }

    /// The page's scrolling body, fading in when the page changes, and on Back up, Restore and
    /// Transfer the command bar.
    pub(crate) fn page(&mut self, ui: &mut egui::Ui, t: &Tokens, pad: i8) {
        let rest = ui.max_rect();
        let bar = if self.intro.is_some()
            || matches!(
                self.tab,
                Tab::Settings | Tab::Tools | Tab::Privacy | Tab::License
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
                let mut area = egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .id_salt((self.tab as u8, self.intro.is_some()));
                // Held every frame: the first frames do not know the content height yet.
                if let Some(y) = self.scroll {
                    area = area.vertical_scroll_offset(y);
                }
                area.show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(pad, 16))
                        .show(ui, |ui| {
                            // Back up runs edge to edge, so its cable spans the tabs and the bar.
                            if self.tab != Tab::Backup || self.intro.is_some() {
                                ui.set_max_width(ui.available_width().min(1040.0));
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

    /// The app's name and the page tabs, with an accent pill that slides to the current tab.
    /// The intro shows the name only.
    pub(crate) fn top_bar(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        ui.horizontal(|ui| {
            ui.set_min_height(34.0);
            // The tabs need all the room on narrow windows.
            if ui.available_width() >= 560.0 || self.intro.is_some() {
                ui.label(RichText::new("Phone Backup").font(semibold(14.0)));
            }
            if self.intro.is_some() {
                return;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                // The page cannot change while a job runs: its progress and Stop are on it.
                ui.add_enabled_ui(!self.busy.load(Ordering::Relaxed), |ui| {
                    let mut current = None;
                    // Right to left, so the last tab comes first.
                    for (tab, label) in [
                        (Tab::Settings, "Settings"),
                        (Tab::Tools, "Tools"),
                        (Tab::Transfer, "Transfer"),
                        (Tab::Restore, "Restore"),
                        (Tab::Backup, "Back up"),
                    ] {
                        let on = self.tab == tab
                            || (tab == Tab::Settings
                                && matches!(self.tab, Tab::Privacy | Tab::License));
                        let r = nav_item(ui, t, label, on);
                        if on {
                            current = Some(r.rect);
                        }
                        if r.clicked() && self.tab != tab {
                            self.go(tab);
                        }
                    }
                    if let Some(rect) = current {
                        let to = rect.center().x;
                        // 0 until the tabs are first drawn: start in place, not at the left edge.
                        let from = if self.pill.1 == 0.0 { to } else { self.pill.1 };
                        let x = from + (to - from) * self.since_switch(ui.ctx(), 0.25);
                        self.pill = (x, from);
                        let pill = Rect::from_center_size(
                            egui::pos2(x, rect.max.y - 1.5),
                            vec2(16.0, 3.0),
                        );
                        ui.painter().rect_filled(pill, 2, t.accent);
                    }
                });
            });
        });
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

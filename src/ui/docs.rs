use super::device::BRANDS;
use crate::*;

impl App {
    /// A way back. The page's own title says where you are.
    fn trail(&mut self, ui: &mut egui::Ui, t: &Tokens, back: &str, to: Tab) {
        if link(ui, t, &format!("‹ {back}")) {
            self.go(to);
        }
    }

    /// Big picture and one plain line, for pages you read.
    fn doc_head(&self, ui: &mut egui::Ui, t: &Tokens, glyph: &str, title: &str, lede: &str) {
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(56.0), egui::Sense::hover());
            icon(ui, r.center(), glyph, 44.0, t.accent);
            ui.vertical(|ui| {
                ui.label(txt(title, 20.0, 600.0, t.ink));
                ui.add(egui::Label::new(txt(lede, 13.0, 400.0, t.muted)).wrap());
            });
        });
        ui.add_space(14.0);
    }

    /// How to turn on USB debugging for each phone maker, and what to try when no phone shows up.
    pub(crate) fn help_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        const USB: &str = "\u{E88E}";
        let back = self.help_from;
        let word = match back {
            Tab::Settings => "Settings",
            Tab::Restore => "Restore",
            Tab::Transfer => "Transfer",
            _ => "Back up",
        };
        self.trail(ui, t, word, back);
        self.doc_head(
            ui,
            t,
            USB,
            "Turn on USB debugging",
            "A switch on the phone that lets this PC read apps, contacts and messages. Pick your phone's brand.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            for (i, (name, _)) in BRANDS.iter().enumerate() {
                if chip(ui, t, "", name, self.brand == i, 0.0) {
                    self.brand = i;
                }
            }
        });
        ui.add_space(12.0);
        let steps = BRANDS[self.brand].1;
        info_cards(
            ui,
            t,
            &[
                ("\u{E8EA}", "Step 1", steps[0]),
                ("\u{E713}", "Step 2", steps[1]),
                (
                    USB,
                    "Step 3",
                    "Plug the phone in and tap Allow when it asks.",
                ),
            ],
        );
        ui.add_space(10.0);
        ui.label(txt("Phone not showing up?", 15.0, 600.0, t.ink));
        ui.add_space(8.0);
        info_cards(
            ui,
            t,
            &[
                (
                    USB,
                    "Try another cable",
                    "Some only charge. Plug it straight into the computer, not a hub.",
                ),
                (
                    "\u{E72E}",
                    "Unlock the phone",
                    "Keep the screen on while it connects.",
                ),
                (
                    "\u{E8B7}",
                    "Choose File transfer",
                    "Tap the USB notification on the phone and choose File transfer.",
                ),
                (
                    "\u{E897}",
                    "No question on the phone?",
                    "In Developer options, tap Revoke USB debugging authorizations, then plug it in again.",
                ),
                (
                    "\u{E896}",
                    "USB driver",
                    "Some phones need one on Windows. Get it from the phone maker's website.",
                ),
            ],
        );
    }

    /// Privacy or License, opened from Settings: a trail back, a heading, then short sections.
    pub(crate) fn doc_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        ui.set_max_width(ui.available_width().min(600.0));
        let (title, lede, parts): (&str, &str, &[(&str, &str)]) = if self.tab == Tab::Privacy {
            (
                "Privacy",
                "Phone Backup keeps everything on your computer.",
                &[
                    (
                        "What it reads",
                        "The phone you connect, to show what is on it. It copies only what you choose, and only when you start a backup, restore or tool.",
                    ),
                    (
                        "The helper on your phone",
                        "When a backup includes contacts, call history, messages and the calendar, the app puts a small helper app on the phone to read them, and removes it when the backup ends. If the phone is unplugged first, the next backup removes it. It sends data only to your computer, over the USB debugging connection.",
                    ),
                    (
                        "Where your backups go",
                        "Into the backup folder you choose, as ordinary files. They are not encrypted, so anyone who can open that folder can read them. Keep it on a drive only you use.",
                    ),
                    (
                        "The internet",
                        "No account, ads or tracking. The app goes online only when you click Get adb, to download it from Google, or Check for updates, to ask GitHub for the newest version. It sends none of your data; those sites see your internet address, as any website does. Wi-Fi pairing talks only to your phone, on your own network. Links open in your browser, and only when you click them.",
                    ),
                    (
                        "Diagnostics",
                        "Save diagnostics writes a text file where you choose. Nothing is sent. Read it before you share it.",
                    ),
                    (
                        "Removing your data",
                        "Deleting the app does not delete your backups: delete the backup folder to remove them. The app's settings are kept in your Windows user folder (AppData).",
                    ),
                ],
            )
        } else {
            (
                "License",
                "Phone Backup is free software under the GNU General Public License, version 3 or later. These are its only terms.",
                &[
                    (
                        "You may",
                        "Use it for anything, at home or at work. Read and change its source code. Share copies, changed or not.",
                    ),
                    (
                        "If you share it",
                        "Include the source code or say where to get it, keep the same license, and mark what you changed.",
                    ),
                    (
                        "No warranty",
                        "It comes as is, without any warranty, and the authors are not liable for lost data or other damage. Keep a second copy of anything you cannot lose.",
                    ),
                    (
                        "Other tools",
                        "adb comes from Google under its own terms, which you accept before it downloads. Screen mirroring uses scrcpy, under the Apache License 2.0.",
                    ),
                ],
            )
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui.link("Settings").clicked() {
                self.go(Tab::Settings);
            }
            ui.label(RichText::new("›").color(t.muted));
            ui.label(title);
        });
        ui.add_space(4.0);
        ui.label(RichText::new(title).font(semibold(20.0)));
        ui.add(egui::Label::new(RichText::new(lede).color(t.muted)).wrap());
        for (head, body) in parts {
            ui.add_space(12.0);
            ui.label(RichText::new(*head).font(semibold(14.0)));
            ui.add(egui::Label::new(*body).wrap());
        }
        if self.tab == Tab::License {
            ui.add_space(12.0);
            ui.hyperlink_to(
                "Read the full license text",
                "https://www.gnu.org/licenses/gpl-3.0.html",
            );
        }
    }

    /// The first-run steps: what the app does, adb, USB debugging and the backup folder. Each
    /// step fades in; the first one's shelf of backups rises once.
    pub(crate) fn intro_page(&mut self, ui: &mut egui::Ui, t: &Tokens, step: usize) {
        const LAST: usize = 3;
        let muted = |s: &str| RichText::new(s).color(t.muted);
        let big = |s: &str| {
            RichText::new(s)
                .font(semibold(28.0))
                .extra_letter_spacing(-0.5)
        };
        ui.set_max_width(ui.available_width().min(560.0));
        ui.horizontal(|ui| {
            for i in 0..=LAST {
                let w = if i == step { 32.0 } else { 16.0 };
                let (r, _) = ui.allocate_exact_size(vec2(w, 4.0), egui::Sense::hover());
                let fill = if i <= step { t.accent } else { t.line };
                ui.painter().rect_filled(r, 2, fill);
            }
            ui.add_space(8.0);
            ui.label(muted(&format!("Step {} of {}", step + 1, LAST + 1)));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.link("Skip intro").clicked() {
                    self.intro = None;
                    self.go(Tab::Backup);
                }
            });
        });
        ui.add_space(24.0);
        match step {
            0 => {
                // Five spines on a shelf, rising one after another; the newest is accent.
                let (shelf, _) = ui.allocate_exact_size(vec2(160.0, 96.0), egui::Sense::hover());
                let spent = if self.motion {
                    self.tab_at.elapsed().as_secs_f32()
                } else {
                    9.0
                };
                for (i, h) in [56.0, 74.0, 66.0, 84.0, 90.0].into_iter().enumerate() {
                    let p = ((spent - 0.12 - i as f32 * 0.07) / 0.5).clamp(0.0, 1.0);
                    if p < 1.0 {
                        ui.ctx().request_repaint();
                    }
                    let e = egui::emath::easing::cubic_out(p);
                    let x = shelf.min.x + 4.0 + i as f32 * 26.0;
                    let r = Rect::from_min_max(
                        egui::pos2(x, shelf.max.y - 3.0 - h + 32.0 * (1.0 - e)),
                        egui::pos2(x + 22.0, shelf.max.y - 3.0),
                    );
                    let (fill, edge) = if i == 4 {
                        (t.accent, t.accent)
                    } else {
                        (t.surface, t.border)
                    };
                    let p = ui.painter().with_clip_rect(shelf);
                    p.rect(
                        r,
                        egui::CornerRadius {
                            nw: 3,
                            ne: 3,
                            ..Default::default()
                        },
                        fill.gamma_multiply(e),
                        Stroke::new(1.0, edge.gamma_multiply(e)),
                        StrokeKind::Inside,
                    );
                }
                ui.painter().hline(
                    shelf.x_range(),
                    shelf.max.y - 1.5,
                    Stroke::new(3.0, t.border),
                );
                ui.add_space(20.0);
                ui.add(egui::Label::new(big("Back up your Android phone to this PC")).wrap());
                ui.add(egui::Label::new("Photos, videos and files, and with USB debugging also apps, contacts, messages and the calendar. Next time, only what changed is copied.").wrap());
                ui.add(
                    egui::Label::new(muted(
                        "No account and no cloud. Your backups stay on this computer.",
                    ))
                    .wrap(),
                );
            }
            1 => {
                ui.add(egui::Label::new(big("Apps and messages need adb")).wrap());
                ui.add(egui::Label::new("Photos, videos and files back up without it. For apps, contacts, messages and the calendar, the app uses adb, Google's free tool for Android phones.").wrap());
                ui.add_space(4.0);
                if self.adb_ok {
                    result_box(t).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label("adb is ready on this computer.");
                    });
                } else {
                    self.adb_missing(ui, t);
                    if ui.button("Check again").clicked() {
                        self.adb_ok = backup::adb_installed();
                    }
                }
            }
            2 => {
                ui.add(egui::Label::new(big("Connect your phone")).wrap());
                steps(
                    ui,
                    t,
                    &[
                        "Plug it into this computer with a USB cable.",
                        "Unlock it, tap the USB notification and choose File transfer.",
                        "For apps, contacts and messages, also turn on USB debugging:",
                    ],
                );
                ui.add_space(4.0);
                self.brand_steps(ui, t);
                ui.add(
                    egui::Label::new(muted(
                        "Rather not use a cable? Connect over Wi-Fi from the Back up or Settings page later.",
                    ))
                    .wrap(),
                );
            }
            _ => {
                ui.add(egui::Label::new(big("Where should backups go?")).wrap());
                row(
                    ui,
                    t,
                    "Backup folder",
                    "A first backup is about as big as what is on the phone.",
                    |ui| self.folder_row(ui, t),
                );
                ui.add(egui::Label::new(muted("Backups are ordinary files and are not encrypted. Privacy and the license are in Settings.")).wrap());
            }
        }
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            if step > 0 && ui.button("Back").clicked() {
                self.intro = Some(step - 1);
                self.tab_at = Instant::now();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match step {
                    0 => "Get started",
                    1 if !self.adb_ok => "Set up adb later",
                    LAST => "Start using Phone Backup",
                    _ => "Next",
                };
                if primary(ui, t, label, true) {
                    if step == LAST {
                        self.intro = None;
                        self.go(Tab::Backup);
                    } else {
                        self.intro = Some(step + 1);
                        self.tab_at = Instant::now();
                    }
                }
            });
        });
    }
}

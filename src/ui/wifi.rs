use crate::*;

/// "192.168.1.5:37145" or "phone.local:5555": a host, a colon and a port number.
pub(crate) fn valid_addr(addr: &str) -> bool {
    addr.rsplit_once(':').is_some_and(|(host, port)| {
        !host.is_empty() && !host.contains(' ') && port.parse::<u16>().is_ok_and(|p| p > 0)
    })
}

/// The QR code with its 4-module quiet border, black on white in both themes so phone cameras
/// read it.
pub(crate) fn qr_image(ui: &mut egui::Ui, qr: &qrcodegen::QrCode) {
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

impl App {
    pub(crate) fn run_wifi(&mut self, ctx: &egui::Context, args: Vec<String>) {
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
    pub(crate) fn show_qr(&mut self, ctx: &egui::Context) {
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

    pub(crate) fn stop_qr(&mut self) {
        if self.qr.take().is_some() {
            backup::QR_WAIT.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn wifi_dialog(&mut self, ctx: &egui::Context, t: &Tokens) {
        if dialog(ctx, "wifi", "Connect over Wi-Fi", 400.0, "Close", |ui| {
            self.wifi(ui, t)
        }) {
            self.wifi_open = false;
            self.stop_qr();
        }
    }

    pub(crate) fn wifi(&mut self, ui: &mut egui::Ui, t: &Tokens) {
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
}

#[cfg(test)]
mod tests {
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
}

use crate::export::Book;
use crate::*;

/// How many messages of a conversation are drawn at first, and how many more each click adds.
const PAGE: usize = 200;
const ROW: f32 = 48.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Part {
    Messages,
    Contacts,
    Calls,
    Calendar,
}

/// The backup open on the Read page.
pub(crate) struct Reading {
    name: String,
    book: Book,
    pub(crate) part: Part,
    query: String,
    /// What the search leaves of the current part: rows, or conversations on Messages.
    shown: Vec<usize>,
    /// The open conversation, and what the search leaves of its messages.
    open: Option<usize>,
    open_shown: Vec<usize>,
    /// How many of the open conversation's newest messages are drawn.
    limit: usize,
    /// Attachment previews, made on a worker when a conversation first opens. None: not a picture.
    previews: HashMap<PathBuf, Option<egui::TextureHandle>>,
    previews_asked: HashSet<usize>,
    preview_tx: mpsc::Sender<(PathBuf, Option<egui::ColorImage>)>,
    preview_rx: mpsc::Receiver<(PathBuf, Option<egui::ColorImage>)>,
}

impl Reading {
    pub(crate) fn new(name: String, book: Book) -> Self {
        let (preview_tx, preview_rx) = mpsc::channel();
        let mut r = Self {
            name,
            book,
            part: Part::Messages,
            query: String::new(),
            shown: Vec::new(),
            open: None,
            open_shown: Vec::new(),
            limit: PAGE,
            previews: HashMap::new(),
            previews_asked: HashSet::new(),
            preview_tx,
            preview_rx,
        };
        r.refilter();
        r
    }

    fn rows(&self) -> &[[String; 2]] {
        match self.part {
            Part::Contacts => &self.book.contacts,
            Part::Calls => &self.book.calls,
            Part::Calendar => &self.book.events,
            Part::Messages => &[],
        }
    }

    /// Worked out when the part, search or conversation changes, not on every frame.
    pub(crate) fn refilter(&mut self) {
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let hit = |parts: [&str; 2]| {
            words.is_empty() || {
                let text = parts.join(" ").to_lowercase();
                words.iter().all(|w| text.contains(w))
            }
        };
        self.shown = if self.part == Part::Messages {
            let threads = self.book.threads.iter().enumerate();
            threads
                .filter(|(_, t)| t.messages.iter().any(|m| hit([&t.who, &m.text])))
                .map(|(i, _)| i)
                .collect()
        } else {
            let rows = self.rows().iter().enumerate();
            rows.filter(|(_, r)| hit([&r[0], &r[1]]))
                .map(|(i, _)| i)
                .collect()
        };
        // On the other parts `shown` holds rows, not conversations.
        if self.part == Part::Messages && self.open.is_some_and(|i| !self.shown.contains(&i)) {
            self.open = None;
        }
        self.open_shown = self.open.map_or(Vec::new(), |i| {
            let t = &self.book.threads[i];
            let all = t.messages.iter().enumerate();
            all.filter(|(_, m)| hit([&t.who, &m.text]))
                .map(|(i, _)| i)
                .collect()
        });
    }

    fn open(&mut self, thread: usize) {
        self.open = Some(thread);
        self.limit = PAGE;
        self.refilter();
    }

    fn thread_list(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("threads")
            .auto_shrink(false)
            .show_rows(ui, ROW, self.shown.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for &i in &self.shown[range] {
                    let th = &self.book.threads[i];
                    let preview = th.messages.last().map_or(String::new(), |m| {
                        let day = m.when.get(..10).unwrap_or(&m.when);
                        let first = m.text.lines().next().unwrap_or("Picture message");
                        format!("{day}  {first}")
                    });
                    if pick_row(ui, t, &th.who, &preview, self.open == Some(i)).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        if let Some(i) = clicked {
            self.open(i);
        }
    }

    /// One conversation, newest at the bottom.
    fn thread(&mut self, ui: &mut egui::Ui, t: &Tokens, index: usize) {
        self.load_previews(ui.ctx(), index);
        let th = &self.book.threads[index];
        ui.add(egui::Label::new(RichText::new(&th.who).font(semibold(14.0))).truncate());
        // shortcut: draws the newest messages and adds more by the click, because message heights
        // vary and show_rows needs them equal; measure each height if long chats feel slow.
        egui::ScrollArea::vertical()
            .id_salt(("thread", index))
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let from = self.open_shown.len().saturating_sub(self.limit);
                if from > 0
                    && ui
                        .button(format!("Show earlier ({})", count(from)))
                        .clicked()
                {
                    self.limit += PAGE;
                }
                let width = ui.available_width();
                for &i in &self.open_shown[from..] {
                    let said = &th.messages[i];
                    let side = if said.me { Align::Max } else { Align::Min };
                    let fill = if said.me { t.info } else { t.surface };
                    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(side), |ui| {
                        egui::Frame::new()
                            .fill(fill)
                            .stroke(Stroke::new(1.0, t.line))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(12, 6))
                            .show(ui, |ui| {
                                // Stays on its side's layout: the bubble then hugs what is inside.
                                ui.set_max_width((width * 0.8 - 24.0).max(120.0));
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let from = if said.me { "Me" } else { "Them" };
                                ui.label(
                                    RichText::new(format!("{from}, {}", said.when))
                                        .color(t.muted)
                                        .size(12.0),
                                );
                                if !said.text.is_empty() {
                                    let text = egui::Label::new(&said.text);
                                    ui.add(text.wrap().halign(Align::Min));
                                }
                                for (name, path) in &said.files {
                                    attachment(ui, &self.previews, name, path.as_deref());
                                }
                            });
                    });
                }
            });
    }

    /// Takes in finished previews, and starts making a conversation's the first time it opens.
    fn load_previews(&mut self, ctx: &egui::Context, index: usize) {
        while let Ok((path, image)) = self.preview_rx.try_recv() {
            let texture = image.map(|img| {
                ctx.load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR)
            });
            self.previews.insert(path, texture);
        }
        if !self.previews_asked.insert(index) {
            return;
        }
        // Newest first: those are on screen. The worker ends once the page has closed.
        let messages = self.book.threads[index].messages.iter().rev();
        let paths: Vec<PathBuf> = messages
            .flat_map(|m| m.files.iter().filter_map(|f| f.1.clone()))
            .collect();
        let (tx, ctx) = (self.preview_tx.clone(), ctx.clone());
        thread::spawn(move || {
            for path in paths {
                let image = thumbnail(&path);
                if tx.send((path, image)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
    }

    fn messages(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let wide = ui.available_width() >= 640.0;
        if wide
            && self.open.is_none()
            && let Some(&first) = self.shown.first()
        {
            self.open(first);
        }
        let area = ui.available_rect_before_wrap();
        if wide {
            let split = area.min.x + 280.0;
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(area.with_max_x(split - 8.0)),
                |ui| self.thread_list(ui, t),
            );
            if let Some(i) = self.open {
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(area.with_min_x(split + 8.0)),
                    |ui| self.thread(ui, t, i),
                );
            }
        } else if let Some(i) = self.open {
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                if ui.link("All conversations").clicked() {
                    self.open = None;
                    self.refilter();
                    return;
                }
                self.thread(ui, t, i);
            });
        } else {
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                self.thread_list(ui, t)
            });
        }
        ui.advance_cursor_after_rect(area);
    }

    fn list(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let (rows, shown) = (self.rows(), &self.shown);
        egui::ScrollArea::vertical()
            .id_salt(self.part as u8)
            .auto_shrink(false)
            .show_rows(ui, ROW, shown.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let width = ui.available_width();
                for &i in &shown[range] {
                    let [title, details] = &rows[i];
                    let inner = ui.allocate_ui_with_layout(
                        vec2(width, ROW),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_size(vec2(width, ROW));
                            ui.spacing_mut().interact_size.y = 20.0;
                            ui.add_space(6.0);
                            ui.add(egui::Label::new(title.as_str()).truncate());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(details.as_str()).color(t.muted).size(12.0),
                                )
                                .truncate(),
                            );
                        },
                    );
                    ui.painter().hline(
                        inner.response.rect.x_range(),
                        inner.response.rect.bottom() - 0.5,
                        Stroke::new(1.0, t.line),
                    );
                }
            });
    }
}

/// A picture message's attachment: a preview if it is a picture, and its name. Clicking shows the
/// file in its folder rather than opening it: the sender chose its type and name.
fn attachment(
    ui: &mut egui::Ui,
    previews: &HashMap<PathBuf, Option<egui::TextureHandle>>,
    name: &str,
    path: Option<&Path>,
) {
    let Some(path) = path else {
        ui.add(egui::Label::new(name).wrap());
        return;
    };
    let mut clicked = false;
    let sealed = backup::is_sealed(path);
    let hint = if sealed { "Open" } else { "Show in folder" };
    if let Some(Some(texture)) = previews.get(path) {
        let image = egui::Image::new(texture)
            .corner_radius(4)
            .sense(egui::Sense::click());
        clicked = ui
            .add(image)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(hint)
            .clicked();
    }
    clicked |= ui.link(name).on_hover_text(hint).clicked();
    // A sealed file means nothing in Explorer, so a plain copy opens instead.
    if clicked && sealed {
        if let Ok(copy) = backup::open_copy(path) {
            let _ = backup::explorer().arg(copy).spawn();
        }
    } else if clicked {
        let mut explorer = backup::explorer();
        // Explorer reads this argument itself and does not accept it quoted as a whole.
        #[cfg(windows)]
        std::os::windows::process::CommandExt::raw_arg(
            &mut explorer,
            format!("/select,\"{}\"", path.display()),
        );
        let _ = explorer.spawn();
    }
}

/// A conversation in the list: its name and last message, and a bar and fill when open.
fn pick_row(ui: &mut egui::Ui, t: &Tokens, title: &str, note: &str, on: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, title));
    if !ui.is_rect_visible(rect) {
        return r;
    }
    if on || r.hovered() {
        ui.painter().rect_filled(rect, 4, t.hover);
    }
    if on {
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 12.0), vec2(3.0, ROW - 24.0));
        ui.painter().rect_filled(bar, 2, t.accent);
    }
    let text = |text: RichText| {
        egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            rect.width() - 24.0,
            TextStyle::Body,
        )
    };
    let title = text(RichText::new(title).color(t.ink));
    let note = text(RichText::new(note).color(t.muted).size(12.0));
    ui.painter()
        .galley(rect.min + vec2(12.0, 6.0), title, t.ink);
    ui.painter()
        .galley(rect.min + vec2(12.0, 26.0), note, t.muted);
    if r.has_focus() {
        ui.painter()
            .rect_stroke(rect, 4, Stroke::new(2.0, t.ink), StrokeKind::Inside);
    }
    r
}

impl App {
    /// A saved backup's messages, contacts, calls and calendar, opened from Tools.
    pub(crate) fn read_page(&mut self, ui: &mut egui::Ui, t: &Tokens) {
        let Some(r) = &mut self.reading else { return };
        let mut back = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            back = ui.link("Tools").clicked();
            ui.label(RichText::new("›").color(t.muted));
            ui.add(egui::Label::new(&r.name).truncate());
        });
        ui.horizontal_wrapped(|ui| {
            let parts = [
                (
                    Part::Messages,
                    "Messages",
                    r.book.threads.iter().map(|t| t.messages.len()).sum(),
                ),
                (Part::Contacts, "Contacts", r.book.contacts.len()),
                (Part::Calls, "Calls", r.book.calls.len()),
                (Part::Calendar, "Calendar", r.book.events.len()),
            ];
            for (part, label, n) in parts {
                let text = format!("{label} {}", count(n));
                if ui.selectable_label(r.part == part, text).clicked() && r.part != part {
                    r.part = part;
                    r.refilter();
                }
            }
        });
        let search = egui::TextEdit::singleline(&mut r.query)
            .hint_text("Search")
            .desired_width(f32::INFINITY);
        if ui.add(search).changed() {
            r.refilter();
        }
        let empty = match r.part {
            Part::Messages if r.book.threads.is_empty() => "This backup has no messages.",
            Part::Contacts if r.book.contacts.is_empty() => "This backup has no contacts.",
            Part::Calls if r.book.calls.is_empty() => "This backup has no call history.",
            Part::Calendar if r.book.events.is_empty() => "This backup has no calendar.",
            _ if r.shown.is_empty() => "Nothing matches your search.",
            _ => "",
        };
        if !empty.is_empty() {
            ui.label(empty);
        } else if r.part == Part::Messages {
            r.messages(ui, t);
        } else {
            r.list(ui, t);
        }
        if back {
            self.go(Tab::Tools);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::Thread;

    #[test]
    fn search_narrows_the_list_and_the_open_conversation() {
        let thread = |who: &str, texts: &[&str]| Thread {
            who: who.into(),
            messages: texts
                .iter()
                .map(|s| crate::export::Said {
                    when: String::new(),
                    me: false,
                    text: s.to_string(),
                    files: Vec::new(),
                })
                .collect(),
        };
        let mut r = Reading::new(
            String::new(),
            Book {
                threads: vec![
                    thread("Ann", &["Lunch today", "See you"]),
                    thread("Bob", &["Invoice"]),
                ],
                contacts: vec![["Ann".into(), "555".into()], ["Bob".into(), "777".into()]],
                calls: Vec::new(),
                events: Vec::new(),
            },
        );
        assert_eq!(r.shown, [0, 1]);
        r.open(0);
        assert_eq!(r.open_shown, [0, 1]);

        // Every word must match, in any letter case.
        r.query = "LUNCH today".into();
        r.refilter();
        assert_eq!(r.shown, [0]);
        assert_eq!(r.open_shown, [0]);

        // A conversation's name matches all its messages.
        r.query = "ann".into();
        r.refilter();
        assert_eq!(r.open_shown, [0, 1]);

        // The open conversation closes when the search leaves it out.
        r.query = "invoice".into();
        r.refilter();
        assert_eq!(r.shown, [1]);
        assert_eq!(r.open, None);
        assert!(r.open_shown.is_empty());

        // Searching another part leaves the open conversation alone.
        r.query.clear();
        r.refilter();
        r.open(0);
        r.part = Part::Contacts;
        r.query = "777".into();
        r.refilter();
        assert_eq!(r.shown, [1]);
        assert_eq!(r.open, Some(0));
    }
}

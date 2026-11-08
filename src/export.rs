//! Turns a backup's contacts, call history, messages and calendar into files other programs
//! open (CSV, HTML, TXT, ICS), and sorts its photos and videos into year and month folders.
//! Reads the backup; writes only inside its `exports` folder.

use crate::backup::{self, Msg, count, files};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::UNIX_EPOCH;

/// The folder inside a backup that exports go to.
pub const EXPORTS: &str = "exports";

/// Writes every export the backup has data for into `<root>/exports` and says what was made.
pub fn export(root: &Path, report: &dyn Fn(Msg)) -> Result<String, String> {
    let out = root.join(EXPORTS);
    fs::create_dir_all(&out).map_err(|e| format!("Cannot create {}: {e}", out.display()))?;
    let read = |name: &str| fs::read_to_string(root.join("data").join(name)).ok();
    let write = |name: &str, text: String| {
        fs::write(out.join(name), text).map_err(|e| format!("Cannot write {name}: {e}"))
    };
    let mut made = Vec::new();

    report(Msg::Status(
        "Exporting contacts, calls and messages…".into(),
    ));
    let contacts = read("contacts.txt").map(|t| people(&t)).unwrap_or_default();
    if !contacts.is_empty() {
        write("contacts.csv", contacts_csv(&contacts))?;
        write("contacts.html", contacts_html(&contacts))?;
        made.push("contacts");
    }
    let names = names(&contacts);
    // Like contacts, data with no rows makes no file.
    let calls = read("call_log.txt")
        .map(|t| calls(&t, &names))
        .unwrap_or_default();
    if !calls.is_empty() {
        write("calls.csv", calls_csv(&calls))?;
        write("calls.html", calls_html(&calls))?;
        made.push("call history");
    }
    let messages = read("sms.txt")
        .map(|t| messages(&t, &names))
        .unwrap_or_default();
    if !messages.is_empty() {
        write("messages.csv", messages_csv(&messages))?;
        write("messages.html", messages_html(&messages))?;
        write("messages.txt", messages_txt(&messages))?;
        made.push("messages");
    }
    // Backups from before the calendar was saved have no calendar.txt.
    let events = read("calendar.txt")
        .map(|t| rows(&t, &EVENT_KEYS))
        .unwrap_or_default();
    if !events.is_empty() {
        write("calendar.ics", ics(&events))?;
        made.push("calendar");
    }

    let photos = sort_photos(root, &out.join("photos"), report)?;
    if backup::STOP.load(Ordering::Relaxed) {
        return Err(format!(
            "You stopped the export. {} sorted so far are in the exports folder.",
            files(photos)
        ));
    }
    if photos > 0 {
        made.push("photos and videos");
    }
    match made.len() {
        0 => Err(
            "This backup has no contacts, calls, messages, calendar or photos to export.".into(),
        ),
        _ => Ok(format!(
            "Exported {} to {}.",
            made.join(", "),
            out.display()
        )),
    }
}

/// One conversation on the Read page, oldest message first.
pub struct Thread {
    pub who: String,
    pub messages: Vec<Said>,
}

/// A picture message's attachments: name, and the saved file if it can be found.
type Files = Vec<(String, Option<PathBuf>)>;

/// One text or picture message on the Read page.
pub struct Said {
    pub when: String,
    /// Sent by the phone's owner.
    pub me: bool,
    pub text: String,
    pub files: Files,
}

/// What the Read page shows. Contacts, calls and events are [title, details] rows.
pub struct Book {
    pub threads: Vec<Thread>,
    pub contacts: Vec<[String; 2]>,
    pub calls: Vec<[String; 2]>,
    pub events: Vec<[String; 2]>,
}

/// Reads a backup's messages, contacts, calls and calendar for the Read page. Newest come first,
/// contacts by name. Only reads.
pub fn book(root: &Path) -> Result<Book, String> {
    let read = |name: &str| fs::read_to_string(root.join("data").join(name)).unwrap_or_default();
    let people = people(&read("contacts.txt"));
    let names = names(&people);

    let mut all: Vec<(Message, Files)> = messages(&read("sms.txt"), &names)
        .into_iter()
        .map(|m| (m, Vec::new()))
        .collect();
    all.extend(picture_messages(root, &read("mms.txt"), &names));
    all.sort_by_key(|(m, _)| (number_key(&m.1), m.0));
    let mut threads: Vec<(i64, Thread)> = all
        .chunk_by(|a, b| number_key(&a.0.1) == number_key(&b.0.1))
        .map(|chat| {
            let last = &chat[chat.len() - 1].0;
            let who = match who(&last.1, &last.2) {
                w if w.is_empty() => "Unknown number".to_string(),
                w => w,
            };
            let messages = chat
                .iter()
                .map(|((ms, _, _, me, text), files)| Said {
                    when: when(&ms.to_string()),
                    me: *me,
                    text: text.clone(),
                    files: files.clone(),
                })
                .collect();
            (last.0, Thread { who, messages })
        })
        .collect();
    threads.sort_by_key(|t| std::cmp::Reverse(t.0));
    let threads: Vec<Thread> = threads.into_iter().map(|t| t.1).collect();

    let mut contacts: Vec<[String; 2]> = contact_rows(&people)
        .map(|c| {
            let details = [c[1].as_str(), c[2].as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("; ");
            if c[0].is_empty() {
                [details, String::new()]
            } else {
                [c[0].clone(), details]
            }
        })
        .collect();
    contacts.sort_by_key(|c| c[0].to_lowercase());

    let mut calls = calls(&read("call_log.txt"), &names);
    calls.sort_by_key(|c| std::cmp::Reverse(c[2].parse::<i64>().unwrap_or(0)));
    let calls: Vec<[String; 2]> = call_rows(&calls)
        .map(|c| {
            let title = match who(&c[1], &c[2]) {
                w if w.is_empty() => "Private number".to_string(),
                w => w,
            };
            [title, format!("{}, {}, {}", c[0], c[3], c[4])]
        })
        .collect();

    let mut events = rows(&read("calendar.txt"), &EVENT_KEYS);
    events.sort_by_key(|e| std::cmp::Reverse(e[1].parse::<i64>().unwrap_or(0)));
    let events: Vec<[String; 2]> = events.iter().map(|e| event_row(e)).collect();

    if threads.is_empty() && contacts.is_empty() && calls.is_empty() && events.is_empty() {
        return Err("This backup has no messages, contacts, calls or calendar to read.".into());
    }
    Ok(Book {
        threads,
        contacts,
        calls,
        events,
    })
}

/// Picture messages from mms.txt, which holds three lists. Attachments are saved as
/// `data/mms/<part id>_<name>`; backups from before part ids were saved give names without files.
fn picture_messages(
    root: &Path,
    text: &str,
    names: &BTreeMap<String, String>,
) -> Vec<(Message, Files)> {
    // adb on Windows ends each line with \r\n.
    let text = text.replace("\r\n", "\n");
    let Some((list, rest)) = text.split_once("\nParts:\n") else {
        return Vec::new();
    };
    let Some((parts, addresses)) = rest.rsplit_once("\nAddresses:\n") else {
        return Vec::new();
    };
    let mut part_rows = rows(parts.trim_end(), &["_id", "mid", "ct", "cl", "text"]);
    if part_rows.is_empty() {
        part_rows = rows(parts.trim_end(), &["mid", "ct", "cl", "text"]);
        part_rows
            .iter_mut()
            .for_each(|p| p.insert(0, String::new()));
    }
    let address_rows = rows(addresses, &["msg_id", "address", "type"]);
    let saved: HashMap<String, PathBuf> = fs::read_dir(root.join("data").join("mms"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            Some((name.split_once('_')?.0.to_string(), e.path()))
        })
        .collect();

    rows(list.trim_end(), &["_id", "date", "msg_box", "sub"])
        .iter()
        .map(|m| {
            // Box 1 is received; the others are from the owner. These dates are in seconds.
            let me = m[2] != "1";
            let ms = m[1].parse::<i64>().unwrap_or(0).saturating_mul(1000);
            let parts: Vec<_> = part_rows.iter().filter(|p| p[1] == m[0]).collect();
            let text: Vec<&str> = parts
                .iter()
                .filter(|p| p[2] == "text/plain")
                .map(|p| p[4].as_str())
                .collect();
            let text = match text.join("\n") {
                t if t.is_empty() => m[3].clone(),
                t => t,
            };
            let files = parts
                .iter()
                .filter(|p| !["", "text/plain", "application/smil"].contains(&p[2].as_str()))
                .map(|p| {
                    let name = if p[3].is_empty() { "Attachment" } else { &p[3] };
                    (name.to_string(), saved.get(&p[0]).cloned())
                })
                .collect();
            // Address type 137 is the sender, 151 a recipient; "No result found." can follow it.
            // shortcut: a group message files under one person, until the page shows group chats.
            let want = if me { "151" } else { "137" };
            let people = || {
                address_rows
                    .iter()
                    .filter(|a| a[0] == m[0] && !a[1].is_empty() && a[1] != "insert-address-token")
            };
            let number = people()
                .find(|a| a[2].starts_with(want))
                .or(people().next())
                .map_or(String::new(), |a| a[1].clone());
            let name = names.get(&number_key(&number)).cloned().unwrap_or_default();
            ((ms, number, name, me, text), files)
        })
        .collect()
}

/// [title, "2026-10-31 14:05, place, Repeats, description"], on one line.
fn event_row(e: &[String]) -> [String; 2] {
    let [title, start, _, all_day, place, about, rrule] = e else {
        return Default::default();
    };
    // All-day events are stored as UTC midnights, so local time could show the day before.
    let date = start
        .parse()
        .ok()
        .and_then(|ms| time(ms, false))
        .filter(|_| all_day == "1");
    let start = date.map_or(when(start), |[y, m, d, ..]| {
        format!("{y}-{m:02}-{d:02}, all day")
    });
    let about = about.split_whitespace().collect::<Vec<_>>().join(" ");
    let repeats = if rrule.is_empty() { "" } else { "Repeats" };
    let details = [start.as_str(), place, repeats, &about]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let title = if title.is_empty() { "No title" } else { title };
    [title.to_string(), details]
}

/// `content query` output as one list of values per row, in `keys` order. "Row: 0 a=1, b=x, y"
/// with keys a, b gives ["1", "x, y"]. The last value may run over several lines. NULL is "".
pub fn rows(text: &str, keys: &[&str]) -> Vec<Vec<String>> {
    let mut records: Vec<String> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Row: ") {
            records.push(rest.split_once(' ').map_or("", |r| r.1).to_string());
        } else if let Some(last) = records.last_mut() {
            last.push('\n');
            last.push_str(line);
        }
    }
    records
        .iter()
        .filter_map(|record| {
            let mut rest = record.strip_prefix(keys.first()?)?.strip_prefix('=')?;
            let mut values = Vec::new();
            for key in &keys[1..] {
                let (value, next) = rest.split_once(&format!(", {key}="))?;
                values.push(value);
                rest = next;
            }
            values.push(rest);
            Some(
                values
                    .into_iter()
                    .map(|v| if v == "NULL" { "" } else { v }.to_string())
                    .collect(),
            )
        })
        .collect()
}

/// A contact: (name, [("TEL" or "EMAIL", value)]), each value once.
pub type Person = (String, Vec<(&'static str, String)>);

/// Contacts from contacts.txt.
pub fn people(text: &str) -> Vec<Person> {
    let mut by_id: BTreeMap<u64, Person> = BTreeMap::new();
    for row in rows(text, &["contact_id", "mimetype", "display_name", "data1"]) {
        let [id, mime, name, value] = &row[..] else {
            continue;
        };
        let kind = match mime.as_str() {
            "vnd.android.cursor.item/phone_v2" => "TEL",
            "vnd.android.cursor.item/email_v2" => "EMAIL",
            _ => continue,
        };
        let Ok(id) = id.parse() else { continue };
        if value.is_empty() {
            continue;
        }
        let entry = by_id
            .entry(id)
            .or_insert_with(|| (name.clone(), Vec::new()));
        let field = (kind, value.clone());
        if !entry.1.contains(&field) {
            entry.1.push(field);
        }
    }
    by_id.into_values().collect()
}

/// The last ten digits, so "+91 98765-43210" and "09876543210" match. A sender with no digits,
/// such as "VM-HDFCBK", is its own name: each stays a separate conversation.
fn number_key(number: &str) -> String {
    let digits: Vec<char> = number.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return number.to_lowercase();
    }
    digits[digits.len().saturating_sub(10)..].iter().collect()
}

/// Contact names by number: messages and calls show the name where the number matches one.
fn names(contacts: &[Person]) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for (name, fields) in contacts {
        for (kind, value) in fields {
            if *kind == "TEL" && !name.is_empty() {
                names.entry(number_key(value)).or_insert(name.clone());
            }
        }
    }
    names
}

/// Calls from call_log.txt: number, name (from contacts if the phone gave none), date, duration, type.
fn calls(text: &str, names: &BTreeMap<String, String>) -> Vec<Vec<String>> {
    let mut calls = rows(text, &["number", "name", "date", "duration", "type"]);
    for c in calls.iter_mut().filter(|c| c[1].is_empty()) {
        c[1] = names.get(&number_key(&c[0])).cloned().unwrap_or_default();
    }
    calls
}

const EVENT_KEYS: [&str; 7] = [
    "title",
    "dtstart",
    "dtend",
    "allDay",
    "eventLocation",
    "description",
    "rrule",
];

/// One text message: (phone time in ms, number, contact name, sent by the phone's owner, text).
type Message = (i64, String, String, bool, String);

/// Messages from sms.txt, grouped by number and oldest first.
fn messages(text: &str, names: &BTreeMap<String, String>) -> Vec<Message> {
    let mut out: Vec<Message> = rows(text, &["address", "date", "type", "body"])
        .into_iter()
        .map(|r| {
            let name = names.get(&number_key(&r[0])).cloned().unwrap_or_default();
            // type 1 is received; 2 sent, 3 draft, 4 outbox, 5 failed, 6 queued are all from the owner.
            (
                r[1].parse().unwrap_or(0),
                r[0].clone(),
                name,
                r[2] != "1",
                r[3].clone(),
            )
        })
        .collect();
    out.sort_by_key(|m| (number_key(&m.1), m.0));
    out
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV with a byte order mark so Excel reads names in any language correctly.
fn csv(head: &[&str], rows: impl Iterator<Item = Vec<String>>) -> String {
    let mut out = String::from("\u{feff}");
    out += &head.join(",");
    out += "\r\n";
    for row in rows {
        out += &row
            .iter()
            .map(|f| csv_field(f))
            .collect::<Vec<_>>()
            .join(",");
        out += "\r\n";
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A whole page that opens offline and prints cleanly (the browser's Print can save it as PDF).
fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
body {{ font: 15px/1.45 "Segoe UI", system-ui, sans-serif; margin: 24px auto; max-width: 900px; padding: 0 16px; color: #1b1b1b; background: #fff; }}
h1 {{ font-size: 24px; }} h2 {{ font-size: 17px; margin: 28px 0 8px; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ text-align: left; vertical-align: top; padding: 6px 8px; border-bottom: 1px solid #ddd; overflow-wrap: anywhere; }}
th {{ background: #f3f3f3; }}
.msg {{ margin: 6px 0; padding: 8px 12px; border-radius: 8px; background: #f3f3f3; max-width: 75%; white-space: pre-wrap; overflow-wrap: anywhere; }}
.me {{ margin-left: auto; background: #dbeafe; }}
.when {{ display: block; font-size: 12px; color: #616161; }}
input {{ font: inherit; width: 100%; box-sizing: border-box; padding: 8px 10px; margin-bottom: 8px; border: 1px solid #8a8a8a; border-radius: 6px; }}
@media print {{ input {{ display: none; }} body {{ margin: 0; max-width: none; }} section, tr, .msg {{ break-inside: avoid; }} .msg {{ border: 1px solid #ccc; }} }}
</style></head><body>
<h1>{title}</h1>
{body}
</body></html>
"#
    )
}

fn table(head: &[&str], rows: impl Iterator<Item = Vec<String>>) -> String {
    let cell = |tag: &str, s: &str| format!("<{tag}>{}</{tag}>", html_escape(s));
    let mut out = String::from("<table><tr>");
    out += &head.iter().map(|h| cell("th", h)).collect::<String>();
    out += "</tr>\n";
    for row in rows {
        out += "<tr>";
        out += &row.iter().map(|f| cell("td", f)).collect::<String>();
        out += "</tr>\n";
    }
    out + "</table>"
}

fn contact_rows(contacts: &[Person]) -> impl Iterator<Item = Vec<String>> {
    contacts.iter().map(|(name, fields)| {
        let all = |kind: &str| {
            fields
                .iter()
                .filter(|f| f.0 == kind)
                .map(|f| f.1.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        };
        vec![name.clone(), all("TEL"), all("EMAIL")]
    })
}

const CONTACT_HEAD: [&str; 3] = ["Name", "Phone numbers", "Emails"];

fn contacts_csv(contacts: &[Person]) -> String {
    csv(&CONTACT_HEAD, contact_rows(contacts))
}

fn contacts_html(contacts: &[Person]) -> String {
    page("Contacts", &table(&CONTACT_HEAD, contact_rows(contacts)))
}

const CALL_HEAD: [&str; 5] = ["Date", "Number", "Name", "Type", "Duration"];

fn call_rows(calls: &[Vec<String>]) -> impl Iterator<Item = Vec<String>> {
    calls.iter().map(|c| {
        let kind = match c[4].as_str() {
            "1" => "Incoming",
            "2" => "Outgoing",
            "3" => "Missed",
            "4" => "Voicemail",
            "5" => "Rejected",
            "6" => "Blocked",
            other => other,
        };
        let secs: u64 = c[3].parse().unwrap_or(0);
        vec![
            when(&c[2]),
            c[0].clone(),
            c[1].clone(),
            kind.to_string(),
            format!("{}:{:02}", secs / 60, secs % 60),
        ]
    })
}

fn calls_csv(calls: &[Vec<String>]) -> String {
    csv(&CALL_HEAD, call_rows(calls))
}

fn calls_html(calls: &[Vec<String>]) -> String {
    page("Call history", &table(&CALL_HEAD, call_rows(calls)))
}

fn messages_csv(messages: &[Message]) -> String {
    csv(
        &["Date", "Number", "Name", "Direction", "Text"],
        messages.iter().map(|(ms, number, name, me, text)| {
            vec![
                when(&ms.to_string()),
                number.clone(),
                name.clone(),
                if *me { "Sent" } else { "Received" }.into(),
                text.clone(),
            ]
        }),
    )
}

/// "Ann (+1 555)", or the number alone.
fn who(number: &str, name: &str) -> String {
    match name {
        "" => number.to_string(),
        _ => format!("{name} ({number})"),
    }
}

/// One section per conversation, with a search box that hides messages without the words.
fn messages_html(messages: &[Message]) -> String {
    let mut body = String::from(
        r#"<input id="q" type="search" placeholder="Search messages" aria-label="Search messages">"#,
    );
    let mut last = None;
    for (ms, number, name, me, text) in messages {
        if last != Some(number_key(number)) {
            if last.is_some() {
                body += "</section>\n";
            }
            body += &format!("<section><h2>{}</h2>\n", html_escape(&who(number, name)));
            last = Some(number_key(number));
        }
        body += &format!(
            "<div class=\"msg{}\"><span class=\"when\">{}</span>{}</div>\n",
            if *me { " me" } else { "" },
            html_escape(&when(&ms.to_string())),
            html_escape(text)
        );
    }
    if last.is_some() {
        body += "</section>\n";
    }
    body += r#"<script>
document.getElementById("q").addEventListener("input", function () {
  var words = this.value.toLowerCase().split(/\s+/).filter(Boolean);
  document.querySelectorAll("section").forEach(function (s) {
    var name = s.querySelector("h2").textContent.toLowerCase(), any = false;
    s.querySelectorAll(".msg").forEach(function (m) {
      var text = name + " " + m.textContent.toLowerCase();
      var hit = words.every(function (w) { return text.indexOf(w) >= 0; });
      m.hidden = !hit;
      any = any || hit;
    });
    s.hidden = !any;
  });
});
</script>"#;
    page("Messages", &body)
}

fn messages_txt(messages: &[Message]) -> String {
    let mut out = String::new();
    let mut last = None;
    for (ms, number, name, me, text) in messages {
        if last != Some(number_key(number)) {
            out += &format!("\r\n== {} ==\r\n", who(number, name));
            last = Some(number_key(number));
        }
        let from = if *me { "Me" } else { "Them" };
        out += &format!(
            "{} {from}: {}\r\n",
            when(&ms.to_string()),
            text.replace('\n', "\r\n    ")
        );
    }
    out.trim_start().to_string()
}

/// A calendar file Google Calendar, Outlook and phones can import.
fn ics(events: &[Vec<String>]) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Phone Backup//EN\r\n");
    let stamp = utc(std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64));
    for (i, e) in events.iter().enumerate() {
        let [title, start, end, all_day, place, about, rrule] = &e[..] else {
            continue;
        };
        let Ok(start_ms) = start.parse::<i64>() else {
            continue;
        };
        // Repeating events have no end time on Android; a duration of zero is close enough.
        let end_ms = end.parse::<i64>().unwrap_or(start_ms);
        let mut lines = vec![
            "BEGIN:VEVENT".to_string(),
            format!("UID:{start_ms}-{i}@phone-backup"),
            format!("DTSTAMP:{stamp}"),
        ];
        // All-day events are stored as UTC midnights.
        if all_day == "1" {
            lines.push(format!("DTSTART;VALUE=DATE:{}", &utc(start_ms)[..8]));
            lines.push(format!(
                "DTEND;VALUE=DATE:{}",
                &utc(end_ms.max(start_ms + 86_400_000))[..8]
            ));
        } else {
            lines.push(format!("DTSTART:{}", utc(start_ms)));
            lines.push(format!("DTEND:{}", utc(end_ms)));
        }
        lines.push(format!("SUMMARY:{}", ics_text(title)));
        if !place.is_empty() {
            lines.push(format!("LOCATION:{}", ics_text(place)));
        }
        if !about.is_empty() {
            lines.push(format!("DESCRIPTION:{}", ics_text(about)));
        }
        if !rrule.is_empty() {
            lines.push(format!("RRULE:{rrule}"));
        }
        lines.push("END:VEVENT".into());
        for line in lines {
            out += &fold(&line);
        }
    }
    out + "END:VCALENDAR\r\n"
}

fn ics_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
}

/// ICS lines longer than 75 bytes continue on the next line after a space, never mid-letter.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut len = 0;
    for c in line.chars() {
        if len + c.len_utf8() > 75 {
            out += "\r\n ";
            len = 1;
        }
        out.push(c);
        len += c.len_utf8();
    }
    out + "\r\n"
}

/// Phone time in ms as "2026-10-31 14:05" in this computer's time zone; the raw text if it is not a time.
fn when(ms: &str) -> String {
    ms.parse()
        .ok()
        .and_then(|ms| time(ms, true))
        .map_or(ms.to_string(), |[y, mo, d, h, mi, _]| {
            format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}")
        })
}

/// Phone time in ms as "20260131T140500Z".
fn utc(ms: i64) -> String {
    time(ms, false).map_or(String::new(), |[y, mo, d, h, mi, s]| {
        format!("{y}{mo:02}{d:02}T{h:02}{mi:02}{s:02}Z")
    })
}

/// [year, month, day, hour, minute, second] of a Unix time in ms, in local time or UTC.
#[cfg(windows)]
fn time(ms: i64, local: bool) -> Option<[u16; 6]> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    // FILETIME counts 100 ns steps from 1601.
    let t = u64::try_from(ms.checked_add(11_644_473_600_000)?.checked_mul(10_000)?).ok()?;
    let ft = FILETIME {
        dwLowDateTime: t as u32,
        dwHighDateTime: (t >> 32) as u32,
    };
    // SAFETY: both calls only read and fill the structs they are given; null means the current time zone.
    let s = unsafe {
        let mut utc = std::mem::zeroed();
        if FileTimeToSystemTime(&ft, &mut utc) == 0 {
            return None;
        }
        let mut s = utc;
        if local && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut s) == 0 {
            return None;
        }
        s
    };
    Some([s.wYear, s.wMonth, s.wDay, s.wHour, s.wMinute, s.wSecond])
}

#[cfg(not(windows))]
fn time(_: i64, _: bool) -> Option<[u16; 6]> {
    None
}

const MEDIA: [&str; 14] = [
    "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "dng", "mp4", "mov", "3gp", "mkv", "webm",
    "avi",
];

/// Puts each photo and video of the backup in `<to>/<year>/<month>/`, by the date the phone
/// gave the file. Uses hard links: no extra space, and the backup's own files stay as they are.
/// Returns how many files are sorted.
fn sort_photos(root: &Path, to: &Path, report: &dyn Fn(Msg)) -> Result<usize, String> {
    let all = backup::backup_files(root).unwrap_or_default();
    let media: Vec<_> = all
        .iter()
        .map(|(p, ..)| p)
        .filter(|p| {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
            ext.is_some_and(|e| MEDIA.contains(&e.as_str()))
                // Hidden folders hold thumbnails and app caches.
                && !p.strip_prefix(root).unwrap_or(p).iter().any(|c| c.to_string_lossy().starts_with('.'))
        })
        .collect();
    let mut taken = HashSet::new();
    for (i, path) in media.iter().enumerate() {
        if backup::STOP.load(Ordering::Relaxed) {
            return Ok(i);
        }
        if i % 100 == 0 {
            report(Msg::Status(format!(
                "Sorting photos and videos: {} of {}…",
                count(i),
                count(media.len())
            )));
        }
        let ms = fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64);
        let dir = match time(ms, true) {
            Some([y, m, ..]) => to.join(y.to_string()).join(format!("{m:02}")),
            None => to.join("Unknown date"),
        };
        // shortcut: names are given in path order, so a same-named file added by a later backup
        // can shift which file gets " (2)"; fine while exports are rebuilt by hand when needed.
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();
        let mut target = dir.join(path.file_name().unwrap_or_default());
        let mut n = 1;
        while !taken.insert(target.to_string_lossy().to_lowercase()) {
            n += 1;
            target = dir.join(format!("{stem} ({n}).{ext}"));
        }
        if target.exists() {
            continue;
        }
        fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        // Drives without hard links (FAT, exFAT, network shares) get a copy instead.
        if fs::hard_link(path, &target).is_err() {
            fs::copy(path, &target).map_err(|e| format!("Cannot copy {}: {e}", path.display()))?;
        }
    }
    Ok(media.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NASTY: &str = "Hi, \"you\"\nline two 😀 <script>alert(1)</script> & done";

    #[test]
    fn reads_content_query_rows() {
        let text = "Row: 0 address=+1 555, date=1700000000000, type=1, body=Hi, date=soon\nsecond line\n\
                    Row: 1 address=NULL, date=1, type=2, body=x";
        assert_eq!(
            rows(text, &["address", "date", "type", "body"]),
            [
                ["+1 555", "1700000000000", "1", "Hi, date=soon\nsecond line"],
                ["", "1", "2", "x"],
            ]
        );
        assert!(rows("No result found.", &["a"]).is_empty());
    }

    #[test]
    fn escapes_csv_fields() {
        assert_eq!(
            csv(
                &["A", "B"],
                [vec!["plain".into(), NASTY.into()]].into_iter()
            ),
            "\u{feff}A,B\r\nplain,\"Hi, \"\"you\"\"\nline two 😀 <script>alert(1)</script> & done\"\r\n"
        );
    }

    #[test]
    fn escapes_html() {
        let names = BTreeMap::new();
        let text = format!("Row: 0 address=<b>, date=1700000000000, type=2, body={NASTY}");
        let html = messages_html(&messages(&text, &names));
        assert!(!html.contains("<script>alert"));
        assert!(!html.contains("<b>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; done"));
        assert!(html.contains("😀"));
        assert!(html.contains("&quot;you&quot;"));
        // The page's own search script is the only one.
        assert_eq!(html.matches("<script>").count(), 1);
    }

    #[test]
    fn matches_names_and_groups_messages() {
        let contacts = people(
            "Row: 0 contact_id=1, mimetype=vnd.android.cursor.item/phone_v2, display_name=Ann, data1=+91 98765-43210",
        );
        let mut names = BTreeMap::new();
        names.insert(number_key(&contacts[0].1[0].1), contacts[0].0.clone());
        let text = "Row: 0 address=09876543210, date=2, type=1, body=b\n\
                    Row: 1 address=112, date=1, type=1, body=c\n\
                    Row: 2 address=+919876543210, date=1, type=2, body=a";
        let txt = messages_txt(&messages(text, &names));
        assert_eq!(
            txt,
            format!(
                "== 112 ==\r\n{} Them: c\r\n\r\n== Ann (+919876543210) ==\r\n{} Me: a\r\n{} Them: b\r\n",
                when("1"),
                when("1"),
                when("2")
            )
        );
    }

    #[test]
    fn writes_calls_and_contacts() {
        let calls = rows(
            "Row: 0 number=+1 555, name=Lee, Ann, date=0, duration=75, type=3",
            &["number", "name", "date", "duration", "type"],
        );
        assert_eq!(
            calls_csv(&calls),
            format!(
                "\u{feff}Date,Number,Name,Type,Duration\r\n{},+1 555,\"Lee, Ann\",Missed,1:15\r\n",
                when("0")
            )
        );
        let contacts = people(
            "Row: 0 contact_id=1, mimetype=vnd.android.cursor.item/phone_v2, display_name=<i>Ann</i>, data1=1\n\
             Row: 1 contact_id=1, mimetype=vnd.android.cursor.item/email_v2, display_name=<i>Ann</i>, data1=a@x.com",
        );
        assert!(
            contacts_html(&contacts)
                .contains("<td>&lt;i&gt;Ann&lt;/i&gt;</td><td>1</td><td>a@x.com</td>")
        );
    }

    #[test]
    fn writes_calendar() {
        let events = rows(
            "Row: 0 title=Dentist, Dr. Lee; room 2, dtstart=1767225600000, dtend=1767229200000, allDay=0, eventLocation=NULL, description=Bring 😀 card\nand form, rrule=NULL\n\
             Row: 1 title=Holiday, dtstart=1767225600000, dtend=NULL, allDay=1, eventLocation=Home, description=NULL, rrule=FREQ=YEARLY",
            &[
                "title",
                "dtstart",
                "dtend",
                "allDay",
                "eventLocation",
                "description",
                "rrule",
            ],
        );
        let ics = ics(&events);
        assert!(ics.contains("DTSTART:20260101T000000Z\r\nDTEND:20260101T010000Z\r\n"));
        assert!(ics.contains("SUMMARY:Dentist\\, Dr. Lee\\; room 2\r\n"));
        assert!(ics.contains("DESCRIPTION:Bring 😀 card\\nand form\r\n"));
        assert!(ics.contains("DTSTART;VALUE=DATE:20260101\r\nDTEND;VALUE=DATE:20260102\r\n"));
        assert!(ics.contains("RRULE:FREQ=YEARLY\r\n"));
        assert_eq!(ics.matches("BEGIN:VEVENT").count(), 2);
        let long = fold(&"é".repeat(60));
        assert!(long.split("\r\n").all(|l| l.len() <= 75));
        assert_eq!(long.replace("\r\n ", ""), format!("{}\r\n", "é".repeat(60)));
    }

    #[test]
    fn exports_a_whole_backup() {
        let root = std::env::temp_dir().join(format!("pb-export-all-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("data");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(
            data.join("contacts.txt"),
            "Row: 0 contact_id=1, mimetype=vnd.android.cursor.item/phone_v2, display_name=Ann \"A\" Lee, data1=+1 555 0100\n",
        )
        .unwrap();
        fs::write(
            data.join("call_log.txt"),
            "Row: 0 number=+15550100, name=NULL, date=1767225600000, duration=61, type=1\n",
        )
        .unwrap();
        fs::write(
            data.join("sms.txt"),
            format!("Row: 0 address=+15550100, date=1767225600000, type=1, body={NASTY}\nRow: 1 address=+15550100, date=1767225660000, type=2, body=Ok 👍\n"),
        )
        .unwrap();
        // The phone answers "No result found." for an empty calendar.
        fs::write(data.join("calendar.txt"), "No result found.\n").unwrap();
        let said = export(&root, &|_| {}).unwrap();
        assert!(said.starts_with("Exported contacts, call history, messages to "));
        let out = root.join(EXPORTS);
        for name in [
            "contacts.csv",
            "contacts.html",
            "calls.csv",
            "calls.html",
            "messages.csv",
            "messages.html",
            "messages.txt",
        ] {
            assert!(out.join(name).is_file(), "{name}");
        }
        // An empty calendar makes no calendar file.
        assert!(!out.join("calendar.ics").exists());
        assert!(
            fs::read_to_string(out.join("messages.txt"))
                .unwrap()
                .contains("== Ann \"A\" Lee (+15550100) ==")
        );
        assert!(
            fs::read_to_string(out.join("calls.csv"))
                .unwrap()
                .contains(",+15550100,\"Ann \"\"A\"\" Lee\",Incoming,1:01")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_a_backup_for_the_read_page() {
        let root = std::env::temp_dir().join(format!("pb-book-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("data");
        fs::create_dir_all(&data).unwrap();
        assert!(book(&root).is_err());
        fs::write(
            data.join("contacts.txt"),
            "Row: 0 contact_id=1, mimetype=vnd.android.cursor.item/phone_v2, display_name=zed, data1=+91 98765 43210\n\
             Row: 1 contact_id=2, mimetype=vnd.android.cursor.item/email_v2, display_name=Ann, data1=a@b.c\n",
        )
        .unwrap();
        fs::write(
            data.join("sms.txt"),
            "Row: 0 address=VM-HDFCBK, date=3000, type=1, body=Paid\n\
             Row: 1 address=AX-AMAZON, date=1000, type=1, body=Sale\n\
             Row: 2 address=+919876543210, date=2000, type=2, body=Hi\n\
             Row: 3 address=09876543210, date=1500, type=1, body=Hello\nthere\n",
        )
        .unwrap();
        fs::write(
            data.join("call_log.txt"),
            "Row: 0 number=NULL, name=NULL, date=1000, duration=0, type=3\n\
             Row: 1 number=+919876543210, name=NULL, date=2000, duration=61, type=1\n",
        )
        .unwrap();
        let start = 1_767_225_600_000i64;
        fs::write(
            data.join("calendar.txt"),
            format!("Row: 0 title=Old, dtstart=1000, dtend=NULL, allDay=0, eventLocation=NULL, description=NULL, rrule=NULL\n\
                     Row: 1 title=NULL, dtstart={start}, dtend=NULL, allDay=1, eventLocation=Home, description=Line\nmore, rrule=FREQ=YEARLY\n"),
        )
        .unwrap();
        let b = book(&root).unwrap();
        // Senders with no digits stay apart, and the newest conversation comes first.
        let who: Vec<_> = b.threads.iter().map(|t| t.who.as_str()).collect();
        assert_eq!(who, ["VM-HDFCBK", "zed (+919876543210)", "AX-AMAZON"]);
        // The same number written two ways is one conversation, oldest message first.
        let chat = &b.threads[1].messages;
        assert_eq!((chat[0].me, chat[0].text.as_str()), (false, "Hello\nthere"));
        assert_eq!((chat[1].me, chat[1].text.as_str()), (true, "Hi"));
        assert_eq!(b.contacts[0], ["Ann", "a@b.c"]);
        assert_eq!(b.contacts[1], ["zed", "+91 98765 43210"]);
        assert!(b.calls[0][0].starts_with("zed"));
        assert!(b.calls[0][1].ends_with(", Incoming, 1:01"));
        assert_eq!(b.calls[1][0], "Private number");
        assert_eq!(b.events[0][0], "No title");
        assert!(!b.events[0][1].contains('\n'));
        assert!(b.events[0][1].ends_with(", Home, Repeats, Line more"));
        assert_eq!(b.events[1][0], "Old");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_picture_messages() {
        let root = std::env::temp_dir().join(format!("pb-book-mms-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("data");
        fs::create_dir_all(data.join("mms")).unwrap();
        fs::write(data.join("mms").join("9_pic.jpg"), "x").unwrap();
        fs::write(
            data.join("sms.txt"),
            "Row: 0 address=+919876543210, date=2000000, type=1, body=Later\n",
        )
        .unwrap();
        let mms = "Row: 0 _id=1, date=1000, msg_box=1, sub=NULL\n\
                   Row: 1 _id=2, date=3000, msg_box=2, sub=Trip\n\
                   \nParts:\n\
                   Row: 0 _id=8, mid=1, ct=application/smil, cl=NULL, text=<smil/>\n\
                   Row: 1 _id=9, mid=1, ct=image/jpeg, cl=pic.jpg, text=NULL\n\
                   Row: 2 _id=10, mid=1, ct=text/plain, cl=NULL, text=Two\nlines\n\
                   Row: 3 _id=11, mid=2, ct=video/mp4, cl=NULL, text=NULL\n\
                   \nAddresses:\n\
                   Row: 0 msg_id=1, address=+919876543210, type=137\n\
                   Row: 1 msg_id=1, address=+1000, type=151\n\
                   No result found.\n\
                   Row: 0 msg_id=2, address=insert-address-token, type=137\n\
                   Row: 1 msg_id=2, address=+919876543210, type=151\n";
        fs::write(data.join("mms.txt"), mms.replace('\n', "\r\n")).unwrap();
        let b = book(&root).unwrap();
        // Dates in seconds sort among text messages in ms, in one conversation with the texts.
        assert_eq!(b.threads.len(), 1);
        let chat = &b.threads[0].messages;
        assert_eq!(chat.len(), 3);
        assert_eq!(chat[0].text, "Two\nlines");
        assert!(!chat[0].me);
        assert_eq!(chat[0].files.len(), 1);
        assert_eq!(chat[0].files[0].0, "pic.jpg");
        assert!(chat[0].files[0].1.as_ref().unwrap().ends_with("9_pic.jpg"));
        assert_eq!(chat[1].text, "Later");
        // No text: the subject shows. Not saved: the name shows without a file.
        assert_eq!(chat[2].text, "Trip");
        assert!(chat[2].me);
        assert_eq!(chat[2].files, [("Attachment".to_string(), None)]);

        // A backup from before part ids were saved still reads, without the file.
        let old = mms
            .replace("_id=8, mid", "mid")
            .replace("_id=9, mid", "mid");
        let old = old
            .replace("_id=10, mid", "mid")
            .replace("_id=11, mid", "mid");
        fs::write(data.join("mms.txt"), old).unwrap();
        let b = book(&root).unwrap();
        assert_eq!(
            b.threads[0].messages[0].files,
            [("pic.jpg".to_string(), None)]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sorts_photos_by_month_without_touching_the_backup() {
        let root = std::env::temp_dir().join(format!("pb-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let camera = root.join("files").join("DCIM").join("Camera");
        fs::create_dir_all(camera.join(".thumbnails")).unwrap();
        fs::create_dir_all(root.join("files").join("Pictures")).unwrap();
        fs::write(camera.join("a.jpg"), "one").unwrap();
        fs::write(root.join("files").join("Pictures").join("A.JPG"), "two").unwrap();
        fs::write(camera.join(".thumbnails").join("t.jpg"), "x").unwrap();
        fs::write(camera.join("notes.txt"), "x").unwrap();
        let to = root.join(EXPORTS).join("photos");
        assert_eq!(sort_photos(&root, &to, &|_| {}).unwrap(), 2);
        // Run twice: nothing is added the second time.
        assert_eq!(sort_photos(&root, &to, &|_| {}).unwrap(), 2);
        // Both files were written just now, so they share one year and one month folder.
        let only = |dir: &Path| {
            let all: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            assert_eq!(all.len(), 1);
            all[0].clone()
        };
        let mut names: Vec<String> = fs::read_dir(only(&only(&to)))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["A (2).JPG", "a.jpg"]);
        assert_eq!(fs::read_to_string(camera.join("a.jpg")).unwrap(), "one");
        let _ = fs::remove_dir_all(&root);
    }
}

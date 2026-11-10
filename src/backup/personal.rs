//! Puts a backup's contacts, call history, calendar and messages back on a phone through the
//! helper app. Nothing on the phone is changed or removed; only what it lacks is added.
//! See docs/plans/01-personal-data-restore.md.

use super::*;
use crate::export::{EVENT_KEYS, mms_files, mms_tables, number_key, rows};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// What can come back, in the order the Restore page shows it.
pub const PERSONAL: [&str; 4] = ["Contacts", "Call history", "Calendar", "Messages"];

/// The helper's kinds, each with its line's name and which of `PERSONAL` it belongs to.
const KINDS: [(&str, &str, usize); 5] = [
    ("contacts", "Contacts", 0),
    ("calllog", "Call history", 1),
    ("calendar", "Calendar", 2),
    ("sms", "Messages", 3),
    ("mms", "Picture messages", 3),
];

/// Which of `PERSONAL` the backup in `folder` holds.
pub fn has_personal(folder: &Path) -> [bool; 4] {
    let data = folder.join("data");
    let rows = |name: &str| {
        let mut start = [0u8; 4];
        fs::File::open(data.join(name))
            .and_then(|mut f| f.read_exact(&mut start))
            .is_ok_and(|_| &start == b"Row:")
    };
    [
        rows("contacts.txt"),
        rows("call_log.txt"),
        rows("calendar.txt"),
        rows("sms.txt") || rows("mms.txt"),
    ]
}

/// The saved items of one helper kind, in the shape the helper adds them.
pub(super) fn saved_items(folder: &Path, kind: &str) -> Vec<Value> {
    let data = folder.join("data");
    let read = |name: &str| fs::read_to_string(data.join(name)).ok();
    // The helper's JSON holds every field; backups from before it, or from phones that refused
    // it, only have the text. A JSON left by an earlier backup is stale once the text has a
    // different number of rows.
    let items = |name: &str, keys: &[&str]| -> Vec<Value> {
        let text = read(&format!("{name}.txt")).unwrap_or_default();
        let text_rows = rows(&text, keys);
        if let Some(Ok(items)) =
            read(&format!("{name}.json")).map(|j| serde_json::from_str::<Vec<Value>>(&j))
            && items.len() == text_rows.len()
        {
            return items;
        }
        text_rows
            .into_iter()
            .map(|row| {
                let fields = keys.iter().zip(row).filter(|(_, v)| !v.is_empty());
                Value::Object(fields.map(|(k, v)| (k.to_string(), v.into())).collect())
            })
            .collect()
    };
    match kind {
        "contacts" => contacts(&items(
            "contacts",
            &["contact_id", "mimetype", "display_name", "data1"],
        )),
        "calllog" => items("call_log", &["number", "name", "date", "duration", "type"]),
        "calendar" => items("calendar", &EVENT_KEYS),
        "sms" => items("sms", &["address", "date", "type", "body"]),
        "mms" => read("mms.txt").map_or_else(Vec::new, |t| mms_items(folder, &t)),
        _ => Vec::new(),
    }
}

/// The kinds of contact detail put back. Others belong to apps such as WhatsApp, which add
/// their own again; photos and groups are not in backups.
const CONTACT_KINDS: [&str; 12] = [
    "name",
    "phone_v2",
    "email_v2",
    "postal-address_v2",
    "organization",
    "im",
    "nickname",
    "note",
    "website",
    "contact_event",
    "relation",
    "sip_address",
];

/// Contact data rows, one per detail, as one item per contact: {"name", "rows": [{"mimetype",
/// "data1"…}]}. Linked copies of a contact become one, each detail once.
pub(super) fn contacts(data_rows: &[Value]) -> Vec<Value> {
    type Details = Vec<Map<String, Value>>;
    let mut order = Vec::new();
    let mut by_id: HashMap<&str, (String, Details)> = HashMap::new();
    for row in data_rows {
        let id = row["contact_id"].as_str().unwrap_or("");
        let entry = by_id.entry(id).or_insert_with(|| {
            order.push(id);
            (str_of(row, "display_name").to_string(), Vec::new())
        });
        let mime = str_of(row, "mimetype");
        let known = mime
            .strip_prefix("vnd.android.cursor.item/")
            .is_some_and(|k| CONTACT_KINDS.contains(&k));
        if !known {
            continue;
        }
        let detail: Map<String, Value> = row
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, v)| {
                v.is_string() && (k.as_str() == "mimetype" || k.starts_with("data"))
                    || k.starts_with("is_") && v.as_str() != Some("0")
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !entry.1.contains(&detail) {
            entry.1.push(detail);
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            let (name, mut details) = by_id.remove(id)?;
            let named = details
                .iter()
                .any(|d| d["mimetype"] == "vnd.android.cursor.item/name");
            if !named && !name.is_empty() && !names_a_detail(&name, &details) {
                details.insert(
                    0,
                    Map::from_iter([
                        ("mimetype".into(), "vnd.android.cursor.item/name".into()),
                        ("data1".into(), name.clone().into()),
                    ]),
                );
            }
            (!details.is_empty()).then(|| json!({"name": name, "rows": details}))
        })
        .collect()
}

/// Picture messages from mms.txt: {"date", "msg_box", "sub", "addresses": [{"address",
/// "type"}], "parts": [{"ct", "cl", "text", "file"}]}. "file" names the attachment in data/mms;
/// a part whose file was not saved is left out.
pub(super) fn mms_items(folder: &Path, text: &str) -> Vec<Value> {
    let Some((list, parts, addresses)) = mms_tables(text) else {
        return Vec::new();
    };
    let saved = mms_files(folder);
    list.iter()
        .map(|m| {
            let addresses: Vec<Value> = addresses
                .iter()
                .filter(|a| a[0] == m[0] && !a[1].is_empty())
                .map(|a| json!({"address": a[1], "type": a[2].trim()}))
                .collect();
            let parts: Vec<Value> = parts
                .iter()
                .filter(|p| p[1] == m[0] && !p[2].is_empty())
                .filter_map(|p| {
                    let mut part = json!({"ct": p[2], "cl": p[3], "text": p[4]});
                    if !["text/plain", "application/smil"].contains(&p[2].as_str()) {
                        let file = saved.get(&p[0])?.file_name()?.to_string_lossy();
                        part["file"] = file.into_owned().into();
                    }
                    Some(part)
                })
                .collect();
            json!({"date": m[1], "msg_box": m[2], "sub": m[3], "addresses": addresses, "parts": parts})
        })
        .collect()
}

/// A phone shows a contact with no name by its number or email; that is not a name to save.
fn names_a_detail(name: &str, details: &[Map<String, Value>]) -> bool {
    details.iter().any(|d| {
        let value = d.get("data1").and_then(Value::as_str).unwrap_or("");
        match d.get("mimetype").and_then(Value::as_str) {
            Some("vnd.android.cursor.item/phone_v2") => number_key(value) == number_key(name),
            Some("vnd.android.cursor.item/email_v2") => value.eq_ignore_ascii_case(name),
            _ => false,
        }
    })
}

fn str_of<'a>(item: &'a Value, key: &str) -> &'a str {
    item[key].as_str().unwrap_or("").trim()
}

/// What makes an item the same as one already on the phone. The phone's own items give the same
/// key, so restoring twice adds nothing twice.
pub(super) fn key(kind: &str, item: &Value) -> String {
    let s = |k| str_of(item, k);
    match kind {
        "contacts" => {
            let details: Vec<Map<String, Value>> = item["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d.as_object().cloned())
                .collect();
            let name = match s("name") {
                n if names_a_detail(n, &details) => String::new(),
                n => n.to_lowercase(),
            };
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            let details = item["rows"].as_array().into_iter().flatten();
            let of = |mime: &str, norm: fn(&str) -> String| {
                let mut v: Vec<String> = details
                    .clone()
                    .filter(|d| d["mimetype"] == mime)
                    .map(|d| norm(str_of(d, "data1")))
                    .filter(|v| !v.is_empty())
                    .collect();
                v.sort();
                v.dedup();
                v.join(",")
            };
            let numbers = of("vnd.android.cursor.item/phone_v2", number_key);
            let emails = of("vnd.android.cursor.item/email_v2", str::to_lowercase);
            format!("{name}|{numbers}|{emails}")
        }
        "calllog" => format!(
            "{}|{}|{}",
            number_key(s("number")),
            s("date"),
            s("duration")
        ),
        "calendar" => format!("{}|{}|{}", s("title"), s("dtstart"), s("rrule")),
        "sms" => format!("{}|{}|{}", number_key(s("address")), s("date"), s("body")),
        "mms" => format!("{}|{}", s("date"), s("msg_box")),
        _ => String::new(),
    }
}

/// The saved items the phone does not have yet: not on it, and each once.
pub(super) fn missing<'a>(kind: &str, saved: &'a [Value], on_phone: &[Value]) -> Vec<&'a Value> {
    let mut seen: HashSet<String> = on_phone.iter().map(|i| key(kind, i)).collect();
    saved.iter().filter(|i| seen.insert(key(kind, i))).collect()
}

/// One kind's line in the check or the report.
pub(super) fn line(name: &str, saved: usize, added: usize, failed: usize, check: bool) -> String {
    let already = saved.saturating_sub(added + failed);
    let mut said = match (check, added + failed) {
        (true, 0) => format!("{name}: all {} are already on the phone.", count(saved)),
        (true, n) => format!(
            "{name}: will add {} of {}. The other {} {} already on the phone.",
            count(n),
            count(saved),
            count(already),
            if already == 1 { "is" } else { "are" }
        ),
        (false, _) => format!(
            "{name}: added {} of {}. {} {} already on the phone.",
            count(added),
            count(saved),
            count(already),
            were(already)
        ),
    };
    if !check && failed > 0 {
        said += &format!(" {} could not be added.", count(failed));
    }
    said
}

const DISCONNECTED_DATA: &str = "The phone was disconnected. What was added so far stays on the phone; plug it back in and restore again to add the rest.";
const STOPPED_DATA: &str = "You stopped the restore. What was added so far stays on the phone; restore again to add the rest.";

/// Adds the chosen `which` of `PERSONAL` from the backup in `folder` to the phone. With `check`,
/// changes nothing and says what a restore would add. Also returns how many items failed.
pub fn restore_personal(
    serial: &str,
    folder: &Path,
    which: [bool; 4],
    check: bool,
    report: &dyn Fn(Msg),
) -> Result<(String, usize), String> {
    let saved: Vec<(&str, &str, Vec<Value>)> = KINDS
        .iter()
        .filter(|(.., k)| which[*k])
        .map(|(kind, name, _)| (*kind, *name, saved_items(folder, kind)))
        .filter(|(.., items)| !items.is_empty())
        .collect();
    if saved.is_empty() {
        return Ok((String::new(), 0));
    }
    report(Msg::Status(
        "Starting the helper on the phone. Allow it if the phone asks…".into(),
    ));
    // Files and apps may still come back; the line says why these did not.
    let mut helper = match helper::start(serial) {
        Ok(helper) => helper,
        Err(e) if check => {
            return Ok((
                format!("Contacts, calls, calendar and messages cannot be restored: {e}"),
                0,
            ));
        }
        Err(e) => {
            return Ok((
                format!("Contacts, calls, calendar and messages were not restored: {e}"),
                1,
            ));
        }
    };
    let mut todo = Vec::new();
    for (kind, name, items) in &saved {
        report(Msg::Status(format!(
            "Checking {} on the phone…",
            name.to_lowercase()
        )));
        let on_phone = helper.items(kind).ok_or_else(|| {
            if connected(serial) {
                format!(
                    "The phone did not show its {}. Unlock the phone, then try again.",
                    name.to_lowercase()
                )
            } else {
                DISCONNECTED_DATA.to_string()
            }
        })?;
        todo.push((*kind, *name, items.len(), missing(kind, items, &on_phone)));
    }
    if check {
        let mut said: Vec<String> = todo
            .iter()
            .map(|(_, name, saved, new)| line(name, *saved, new.len(), 0, true))
            .collect();
        let adds = |kinds: &[&str]| {
            todo.iter()
                .any(|(k, .., new)| kinds.contains(k) && !new.is_empty())
        };
        if adds(&["calendar"]) {
            said.push(
                "Events go into a new calendar on the phone, \"Restored by Phone Backup\".".into(),
            );
        }
        if adds(&["sms", "mms"]) {
            said.push("For messages, the phone asks to make Phone Backup Helper its messages app for a few minutes. You switch back at the end.".into());
        }
        return Ok((said.join("\n"), 0));
    }

    let total: usize = todo.iter().map(|(.., new)| new.len()).sum();
    let mut done = 0;
    let mut said = Vec::new();
    let mut failed_all = 0;
    // The messages app before the helper took over, once it has.
    let mut switched: Option<String> = None;
    let mut result = Ok(());
    for (kind, name, saved, new) in &todo {
        if new.is_empty() {
            said.push(line(name, *saved, 0, 0, false));
            continue;
        }
        if matches!(*kind, "sms" | "mms") && switched.is_none() {
            match take_sms_role(&helper, report) {
                Ok(previous) => switched = Some(previous),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        if *kind == "mms" {
            report(Msg::Status("Copying picture message attachments…".into()));
            push_attachments(serial, folder, &helper.inbox(), new);
        }
        report(Msg::Status(format!("Adding {}…", name.to_lowercase())));
        let (batches, too_big) = helper::batches(kind, new);
        let (mut added, mut failed) = (0, too_big);
        for batch in batches {
            report(Msg::Progress {
                done,
                total,
                now: name.to_string(),
            });
            if hold() {
                result = Err(STOPPED_DATA.to_string());
                break;
            }
            match helper.insert(kind, &batch) {
                Some((a, f)) => {
                    added += a;
                    failed += f;
                }
                None => {
                    result = Err(DISCONNECTED_DATA.to_string());
                    break;
                }
            }
            done += batch.len();
        }
        failed_all += failed;
        said.push(line(name, *saved, added, failed, false));
        if result.is_err() {
            break;
        }
    }
    report(Msg::Progress {
        done,
        total,
        now: String::new(),
    });
    if let Some(previous) = switched
        && let Some(note) = give_back_sms_role(&mut helper, &previous, report)
    {
        said.push(note);
    }
    match result {
        Ok(()) => Ok((said.join("\n"), failed_all)),
        Err(e) => Err(said.into_iter().chain([e]).collect::<Vec<_>>().join("\n")),
    }
}

/// Makes the helper the phone's messages app, which Android requires to add messages. Returns
/// the name of the app that was, to switch back to.
fn take_sms_role(helper: &helper::Helper, report: &dyn Fn(Msg)) -> Result<String, String> {
    let lost = "The phone stopped answering. Messages were not restored; plug it back in and restore again.";
    let (default, _, previous) = helper.sms_role().ok_or(lost)?;
    if default {
        return Ok(String::new());
    }
    report(Msg::Status(
        "On the phone, choose Phone Backup Helper as the messages app for now. You switch back at the end.".into(),
    ));
    helper.ask_sms_role();
    let end = std::time::Instant::now() + Duration::from_secs(180);
    while std::time::Instant::now() < end {
        if stopped() {
            return Err(STOPPED_DATA.into());
        }
        std::thread::sleep(Duration::from_secs(1));
        if helper.sms_role().ok_or(lost)?.0 {
            return Ok(previous);
        }
    }
    Err("Messages were not restored: the phone did not make Phone Backup Helper its messages app. Restore again and choose it when the phone asks.".into())
}

/// Opens the phone's default apps screen and waits for the user to choose `previous` (the
/// earlier messages app's name) again. If they do not, the helper stays on the phone, since
/// removing the messages app leaves some phones with none, and the returned note says so.
fn give_back_sms_role(
    helper: &mut helper::Helper,
    previous: &str,
    report: &dyn Fn(Msg),
) -> Option<String> {
    if previous.is_empty() {
        return None;
    }
    report(Msg::Status(format!(
        "On the phone, choose {previous} as the messages app again."
    )));
    helper.open_default_apps();
    let end = std::time::Instant::now() + Duration::from_secs(180);
    while std::time::Instant::now() < end && !stopped() {
        std::thread::sleep(Duration::from_secs(1));
        match helper.sms_role() {
            Some((false, ..)) => return None,
            Some(_) => {}
            None => break,
        }
    }
    helper.keep();
    Some(format!(
        "Phone Backup Helper is still the phone's messages app, so it stays on the phone. In Settings, under Default apps, choose {previous} again; then you can uninstall Phone Backup Helper."
    ))
}

/// Pushes the attachments of `items` (picture messages) to where the helper reads them.
/// A file that does not arrive leaves its part out, and the helper counts that message failed.
fn push_attachments(serial: &str, folder: &Path, inbox: &str, items: &[&Value]) {
    let mms = folder.join("data").join("mms");
    let files: Vec<PathBuf> = items
        .iter()
        .flat_map(|m| m["parts"].as_array().into_iter().flatten())
        .filter_map(|p| p["file"].as_str())
        .map(|f| mms.join(f))
        .collect();
    shell_each(serial, "mkdir -p", &[inbox]);
    for batch in files.chunks(100) {
        let mut c = adb();
        c.args(["-s", serial, "push"])
            .args(batch)
            .arg(format!("{inbox}/"));
        run(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: &str = "vnd.android.cursor.item/phone_v2";
    const EMAIL: &str = "vnd.android.cursor.item/email_v2";
    const NAME: &str = "vnd.android.cursor.item/name";

    fn folder(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pb-personal-{}-{}",
            std::process::id(),
            helper_token()
        ));
        fs::create_dir_all(dir.join("data").join("mms")).unwrap();
        for (name, text) in files {
            fs::write(dir.join("data").join(name), text).unwrap();
        }
        dir
    }

    fn helper_token() -> u64 {
        use std::hash::BuildHasher;
        std::collections::hash_map::RandomState::new().hash_one(0u8)
    }

    #[test]
    fn linked_contacts_become_one_with_each_detail_once() {
        let rows = json!([
            {"contact_id":"7","raw_contact_id":"1","mimetype":NAME,"display_name":"Ann Lee","data1":"Ann Lee","data2":"Ann","data3":"Lee"},
            {"contact_id":"7","raw_contact_id":"1","mimetype":PHONE,"display_name":"Ann Lee","data1":"+91 98765 43210","data2":"2","is_primary":"1"},
            {"contact_id":"7","raw_contact_id":"2","mimetype":PHONE,"display_name":"Ann Lee","data1":"+91 98765 43210","data2":"2","is_primary":"1"},
            {"contact_id":"7","raw_contact_id":"2","mimetype":"vnd.android.cursor.item/vnd.com.whatsapp.profile","data1":"x"},
            {"contact_id":"8","mimetype":EMAIL,"display_name":"bo@x.org","data1":"Bo@X.org"},
            {"contact_id":"9","mimetype":"vnd.android.cursor.item/group_membership","data1":"3"},
        ]);
        let people = contacts(rows.as_array().unwrap());
        assert_eq!(people.len(), 2, "{people:?}");
        let ann = people[0]["rows"].as_array().unwrap();
        assert_eq!(ann.len(), 2, "{ann:?}");
        assert_eq!(ann[0]["data2"], "Ann");
        assert!(ann[1].get("raw_contact_id").is_none());
        // The phone showed Bo by email; that is not a name to put back.
        assert_eq!(people[1]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(people[1]["rows"][0]["mimetype"], EMAIL);
    }

    #[test]
    fn keys_match_what_the_phone_lists_after_a_restore() {
        let saved = contacts(json!([
            {"contact_id":"1","mimetype":PHONE,"display_name":"Ann  Lee","data1":"098765 43210"},
            {"contact_id":"1","mimetype":EMAIL,"display_name":"Ann  Lee","data1":"A@B.c"},
            {"contact_id":"2","mimetype":PHONE,"display_name":"Bo","data1":"1"},
        ]).as_array().unwrap());
        // The phone formats the number its own way and has its own ids.
        let phone = contacts(json!([
            {"contact_id":"40","mimetype":NAME,"display_name":"Ann Lee","data1":"Ann Lee"},
            {"contact_id":"40","mimetype":EMAIL,"display_name":"Ann Lee","data1":"a@b.c"},
            {"contact_id":"40","mimetype":PHONE,"display_name":"Ann Lee","data1":"+91 98765-43210"},
        ]).as_array().unwrap());
        let new = missing("contacts", &saved, &phone);
        assert_eq!(new.len(), 1);
        assert_eq!(new[0]["name"], "Bo");
        // A contact with no name comes back as it was, and the phone then shows its number.
        let no_name = contacts(json!([
            {"contact_id":"3","mimetype":PHONE,"display_name":"+1 555 0100","data1":"+1 555 0100"},
        ]).as_array().unwrap());
        assert_eq!(
            no_name[0]["rows"].as_array().unwrap().len(),
            1,
            "no name row made"
        );
        let phone = contacts(
            json!([
                {"contact_id":"41","mimetype":PHONE,"display_name":"+1 555-0100","data1":"+1 555-0100"},
            ])
            .as_array()
            .unwrap(),
        );
        assert!(missing("contacts", &no_name, &phone).is_empty());

        let sms = json!([
            {"address":"+91 1234567890","date":"5","body":"hi"},
            {"address":"+91 1234567890","date":"5","body":"hi"},
            {"address":"1234567890","date":"6","body":"hi"},
        ]);
        let on_phone = json!([{"address":"01234567890","date":"5","body":"hi","read":"1"}]);
        let new = missing("sms", sms.as_array().unwrap(), on_phone.as_array().unwrap());
        assert_eq!(new, [&sms[2]], "on the phone, and twice in the backup");

        let calls = json!([{"number":"1","date":"9","duration":"3"},{"number":"1","date":"9","duration":"4"}]);
        let on_phone = json!([{"number":"1","date":"9","duration":"3","type":"1"}]);
        assert_eq!(
            missing(
                "calllog",
                calls.as_array().unwrap(),
                on_phone.as_array().unwrap()
            )
            .len(),
            1
        );
        // A repeating event comes back without its end, so the end is not in its key.
        let events = json!([{"title":"Gym","dtstart":"100","dtend":"200","rrule":"FREQ=WEEKLY"}]);
        let on_phone =
            json!([{"title":"Gym","dtstart":"100","duration":"P100S","rrule":"FREQ=WEEKLY"}]);
        assert!(
            missing(
                "calendar",
                events.as_array().unwrap(),
                on_phone.as_array().unwrap()
            )
            .is_empty()
        );
    }

    #[test]
    fn older_backups_restore_from_their_text() {
        let dir = folder(&[
            (
                "contacts.txt",
                &format!(
                    "Row: 0 contact_id=3, mimetype={PHONE}, display_name=Ann, Jr, data1=12\nRow: 1 contact_id=3, mimetype={EMAIL}, display_name=Ann, Jr, data1=a@b.c\n"
                ),
            ),
            (
                "sms.txt",
                "Row: 0 address=12, date=5, type=1, body=two\nlines\n",
            ),
            (
                "call_log.txt",
                "Row: 0 number=12, name=NULL, date=5, duration=3, type=2\n",
            ),
            (
                "calendar.txt",
                "Row: 0 title=Dentist, dtstart=1, dtend=2, allDay=0, eventLocation=NULL, description=NULL, rrule=NULL\n",
            ),
        ]);
        let people = saved_items(&dir, "contacts");
        assert_eq!(people.len(), 1);
        assert_eq!(people[0]["name"], "Ann, Jr");
        assert_eq!(people[0]["rows"].as_array().unwrap().len(), 3);
        assert_eq!(
            saved_items(&dir, "sms"),
            [json!({"address":"12","date":"5","type":"1","body":"two\nlines"})]
        );
        assert_eq!(
            saved_items(&dir, "calllog"),
            [json!({"number":"12","date":"5","duration":"3","type":"2"})]
        );
        assert_eq!(saved_items(&dir, "calendar")[0]["title"], "Dentist");
        assert_eq!(has_personal(&dir), [true; 4]);

        // The helper's JSON wins while it matches the text; a stale one from an earlier backup does not.
        let json = r#"[{"address":"12","date":"5","type":"1","body":"two\nlines","read":"1"}]"#;
        fs::write(dir.join("data").join("sms.json"), json).unwrap();
        assert_eq!(saved_items(&dir, "sms")[0]["read"], "1");
        fs::write(dir.join("data").join("sms.json"), "[]").unwrap();
        assert!(saved_items(&dir, "sms")[0].get("read").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn picture_messages_carry_their_people_and_saved_files() {
        let mms = "Row: 0 _id=4, date=100, msg_box=1, sub=NULL\n\nParts:\nRow: 0 _id=10, mid=4, ct=application/smil, cl=smil.xml, text=<smil/>\nRow: 1 _id=11, mid=4, ct=text/plain, cl=text_0.txt, text=Look\nRow: 2 _id=12, mid=4, ct=image/jpeg, cl=a.jpg, text=NULL\nRow: 3 _id=13, mid=4, ct=image/jpeg, cl=b.jpg, text=NULL\n\nAddresses:\nRow: 0 msg_id=4, address=+911, type=137\nRow: 0 msg_id=4, address=insert-address-token, type=151\n";
        let dir = folder(&[("mms.txt", mms)]);
        fs::write(dir.join("data").join("mms").join("12_a.jpg"), "jpg").unwrap();
        let items = saved_items(&dir, "mms");
        assert_eq!(items.len(), 1);
        let m = &items[0];
        assert_eq!(
            (m["date"].as_str(), m["msg_box"].as_str()),
            (Some("100"), Some("1"))
        );
        assert_eq!(m["addresses"].as_array().unwrap().len(), 2);
        let parts = m["parts"].as_array().unwrap();
        // b.jpg was not saved, so it cannot come back.
        assert_eq!(parts.len(), 3, "{parts:?}");
        assert_eq!(parts[2]["file"], "12_a.jpg");
        assert_eq!(key("mms", m), "100|1");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Contacts, calls and an event go onto a phone once: the second restore adds nothing.
    /// Messages need a tap on the phone, so they are tried by hand. Emulator only:
    /// `PB_E2E_SERIAL=emulator-5554 cargo test e2e_personal -- --ignored`.
    #[test]
    #[ignore]
    fn e2e_personal_restore_adds_nothing_twice() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let stamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let dir = folder(&[
            (
                "contacts.txt",
                &format!(
                    "Row: 0 contact_id=1, mimetype={PHONE}, display_name=E2E restore {stamp}, data1=+1 555 {}\n",
                    stamp % 10_000
                ),
            ),
            (
                "call_log.txt",
                &format!(
                    "Row: 0 number=+1555{}, name=NULL, date={stamp}, duration=42, type=1\n",
                    stamp % 10_000
                ),
            ),
            (
                "calendar.txt",
                &format!(
                    "Row: 0 title=E2E restore {stamp}, dtstart={stamp}, dtend={}, allDay=0, eventLocation=NULL, description=NULL, rrule=FREQ=WEEKLY\n",
                    stamp + 3_600_000
                ),
            ),
        ]);
        let which = [true, true, true, false];
        let quiet = |_| {};
        let (said, _) = restore_personal(&serial, &dir, which, true, &quiet).unwrap();
        assert_eq!(said.matches("will add 1 of 1").count(), 3, "{said}");
        let (said, failed) = restore_personal(&serial, &dir, which, false, &quiet).unwrap();
        assert_eq!(
            (said.matches("added 1 of 1").count(), failed),
            (3, 0),
            "{said}"
        );
        let (said, _) = restore_personal(&serial, &dir, which, false, &quiet).unwrap();
        assert_eq!(said.matches("added 0 of 1").count(), 3, "{said}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lines_say_what_was_added() {
        assert_eq!(
            line("Contacts", 812, 0, 0, true),
            "Contacts: all 812 are already on the phone."
        );
        assert_eq!(
            line("Messages", 10_240, 1_204, 0, true),
            "Messages: will add 1,204 of 10,240. The other 9,036 are already on the phone."
        );
        assert_eq!(
            line("Call history", 10, 6, 1, false),
            "Call history: added 6 of 10. 3 were already on the phone. 1 could not be added."
        );
    }
}

//! Runs the Android helper app for one job: installed fresh, then removed again.
//! See docs/android-design.md section 3.2.

use super::*;
use serde_json::Value;
use std::hash::BuildHasher;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Instant;

/// Empty in a build made without the Android tools; see build.rs.
const APK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/helper.apk"));
const PACKAGE: &str = "org.phonebackup.companion";
/// The port the helper listens on, on the phone.
const PORT: u16 = 47621;

/// An open connection to the helper. Dropping it removes the helper from the phone.
pub struct Helper {
    socket: TcpStream,
    _installed: Installed,
}

/// Removes the helper when dropped, so every way out of a job cleans up. A phone unplugged by then
/// keeps it until its next job, which starts by removing the old copy.
struct Installed {
    serial: String,
    /// The port on this computer; 0 until adb has chosen one.
    port: u16,
}

impl Drop for Installed {
    fn drop(&mut self) {
        let serial = self.serial.as_str();
        if self.port != 0 {
            let forward = format!("tcp:{}", self.port);
            let _ = logged(adb().args(["-s", serial, "forward", "--remove", &forward]));
        }
        let _ = logged(adb().args(["-s", serial, "uninstall", PACKAGE]));
    }
}

/// Installs the helper, starts it and connects to it.
pub fn start(serial: &str) -> Result<Helper, String> {
    start_with(serial, &token())
}

fn start_with(serial: &str, token: &str) -> Result<Helper, String> {
    if APK.is_empty() {
        return Err("This copy of the app was built without its phone helper.".into());
    }
    let mut installed = Installed {
        serial: serial.to_string(),
        port: 0,
    };
    let _ = logged(adb().args(["-s", serial, "uninstall", PACKAGE]));
    let file = std::env::temp_dir().join(format!("phone-backup-helper-{token}.apk"));
    fs::write(&file, APK).map_err(|e| format!("Cannot prepare the phone helper: {e}."))?;
    let mut install = adb();
    install.args(["-s", serial, "install", "-g"]).arg(&file);
    let out = run(install);
    let _ = fs::remove_file(&file);
    if let Some(e) = interrupted(serial) {
        return Err(e.into());
    }
    if !String::from_utf8_lossy(&out).contains("Success") {
        return Err("The phone refused the helper app. On some phones, turn on \"Install via USB\" in Developer options, then try again.".into());
    }

    let sdk = shell(serial, "getprop ro.build.version.sdk");
    let am = match sdk.trim().parse::<u32>() {
        Ok(26..) => "start-foreground-service",
        _ => "startservice",
    };
    let start = format!("am {am} -n {PACKAGE}/.BridgeService --es token {token}");
    let _ = logged(adb().args(["-s", serial, "shell", &start]));

    let forward = format!("tcp:{PORT}");
    let out = logged(adb().args(["-s", serial, "forward", "tcp:0", &forward]))
        .map_err(|_| NO_ADB.to_string())?;
    installed.port = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .map_err(|_| "adb could not open a connection to the phone. Try again.".to_string())?;
    Ok(Helper {
        socket: hello(installed.port, token)?,
        _installed: installed,
    })
}

/// 128 bits for the helper to tell this app from other apps on the phone. RandomState's keys come
/// from the operating system's random source.
fn token() -> String {
    let word = || std::collections::hash_map::RandomState::new().hash_one(0u8);
    format!("{:016x}{:016x}", word(), word())
}

/// Connects and proves the token. Retries because the helper needs a moment to start listening;
/// until then adb accepts the connection and closes it.
fn hello(port: u16, token: &str) -> Result<TcpStream, String> {
    let end = Instant::now() + Duration::from_secs(15);
    while Instant::now() < end {
        if let Ok(socket) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
            let request = format!(r#"{{"id":1,"op":"hello","token":"{token}"}}"#);
            if ask(&socket, &request).contains(r#""ok":true"#) {
                return Ok(socket);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err("The helper app on the phone did not answer. Unlock the phone, then try again.".into())
}

/// Sends one line and returns the one line that answers it; empty when the connection ends.
fn ask(mut socket: &TcpStream, request: &str) -> String {
    let mut reply = String::new();
    if writeln!(socket, "{request}").is_ok() {
        let _ = BufReader::new(socket).read_line(&mut reply);
    }
    reply
}

impl Helper {
    /// What goes in this data file, as (`content query` text, so the old method's readers keep
    /// working; the helper's full JSON). None when the helper does not read that file, or unless
    /// the whole list arrived: a part must never replace the last full copy.
    pub fn read(&self, file: &str) -> Option<(String, String)> {
        let (kind, keys): (&str, &[&str]) = match file {
            "contacts.txt" => (
                "contacts",
                &["contact_id", "mimetype", "display_name", "data1"],
            ),
            "sms.txt" => ("sms", &["address", "date", "type", "body"]),
            "call_log.txt" => ("calllog", &["number", "name", "date", "duration", "type"]),
            _ => return None,
        };
        let items = list(&self.socket, kind)?;
        let json: Vec<String> = items.iter().map(Value::to_string).collect();
        Some((
            text_rows(&items, keys),
            format!("[\n{}\n]\n", json.join(",\n")),
        ))
    }
}

/// The helper's answer, unless the old method sees more rows: a phone can hide rows from an
/// ordinary app without saying so, and a backup must not shrink because of it.
pub fn fuller(old: String, helper: Option<(String, String)>) -> (String, Option<String>) {
    let rows = |text: &str| text.lines().filter(|l| l.starts_with("Row: ")).count();
    match helper {
        Some((text, json)) if rows(&text) >= rows(&old) => (text, Some(json)),
        _ => (old, None),
    }
}

/// What the user must hear about one data file, given what `content query` gave (`old`), what is
/// being saved (`out`), whether it was saved, and why the helper did not start, if it did not.
pub fn note(file: &str, old: &str, out: &str, saved: bool, why: Option<&str>) -> Option<String> {
    let rows = |text: &str| text.starts_with("Row:");
    match (file, why) {
        ("contacts.txt" | "sms.txt" | "call_log.txt", Some(why)) if !rows(out) => {
            Some(why.to_string())
        }
        // Only `content query` can show what the phone hid from the helper.
        ("sms.txt", _) if saved && !rows(old) => Some(
            "Some messages may be missing. Android can hide messages from backups, such as recent one-time codes, without saying so.".into(),
        ),
        _ => None,
    }
}

/// Reads one list to its end line, whose count must match. None on an error, Stop, a cut or an
/// empty list: a phone can hide data by answering "nothing" without an error, so the old method
/// has to confirm it. The id keeps late lines of an earlier list out of this one.
fn list(mut socket: &TcpStream, kind: &str) -> Option<Vec<Value>> {
    // A big list takes the phone a while to start sending.
    socket
        .set_read_timeout(Some(Duration::from_secs(60)))
        .ok()?;
    writeln!(socket, r#"{{"id":"{kind}","op":"list","kind":"{kind}"}}"#).ok()?;
    let mut lines = BufReader::new(socket);
    let mut items = Vec::new();
    let mut line = String::new();
    while !stopped() {
        line.clear();
        lines.read_line(&mut line).ok()?;
        let mut reply: Value = serde_json::from_str(&line).ok()?;
        if reply["id"] != kind {
            return None;
        }
        if reply["end"] == true {
            return (reply["count"] == items.len() && !items.is_empty()).then_some(items);
        }
        items.push(reply.get_mut("item")?.take());
    }
    None
}

/// The helper's rows in the text form `content query` prints.
fn text_rows(items: &[Value], keys: &[&str]) -> String {
    let mut out = String::new();
    for (n, item) in items.iter().enumerate() {
        let fields: Vec<String> = keys
            .iter()
            .map(|key| format!("{key}={}", item[*key].as_str().unwrap_or("NULL")))
            .collect();
        out += &format!("Row: {n} {}\n", fields.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The helper's safety rules, against a real phone or emulator. Needs the helper built first.
    /// Run with `PB_E2E_SERIAL=emulator-5554 cargo test e2e_helper -- --ignored`.
    #[test]
    #[ignore]
    fn e2e_helper_session() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let shell = |cmd: &str| {
            let out = logged(adb().args(["-s", &serial, "shell", cmd])).unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        let token = token();
        let Helper {
            socket,
            _installed: installed,
        } = start_with(&serial, &token).unwrap();
        let port = installed.port;
        assert!(!shell(&format!("dumpsys package {PACKAGE}")).contains("DEBUGGABLE"));
        // The helper serves one connection at a time.
        drop(socket);
        let connect = || {
            let socket = TcpStream::connect(("127.0.0.1", port)).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
        };

        let wrong = connect();
        let refused = ask(&wrong, r#"{"id":1,"op":"hello","token":"wrong"}"#);
        assert!(refused.contains("bad_token"), "{refused}");
        drop(wrong);

        // Past the line limit the connection closes and the service stays up.
        let long = connect();
        assert_eq!(ask(&long, &"x".repeat(70_000)), "");
        drop(long);

        let right = hello(port, &token).unwrap();
        let reply = ask(&right, r#"{"id":2,"op":"set_default_sms"}"#);
        assert!(reply.contains("unknown_op"), "{reply}");

        // Every contact comes as one line, then a last line with the count.
        let mut lines = BufReader::new(&right);
        writeln!(&right, r#"{{"id":3,"op":"list","kind":"contacts"}}"#).unwrap();
        let mut items = 0;
        let count = loop {
            let mut line = String::new();
            lines.read_line(&mut line).unwrap();
            if line.contains(r#""end":true"#) {
                break line;
            }
            assert!(line.contains(r#""item":"#), "{line}");
            items += 1;
        };
        assert!(count.contains(&format!(r#""count":{items}"#)), "{count}");

        let bye = ask(&right, r#"{"id":4,"op":"bye"}"#);
        assert!(bye.contains(r#""ok":true"#), "{bye}");
        std::thread::sleep(Duration::from_secs(1));
        let services = shell(&format!("dumpsys activity services {PACKAGE}"));
        assert!(!services.contains("BridgeService"), "{services}");

        drop(installed);
        assert_eq!(shell(&format!("pm list packages {PACKAGE}")), "");
        let forwards = logged(adb().args(["-s", &serial, "forward", "--list"])).unwrap();
        let forwards = String::from_utf8_lossy(&forwards.stdout).into_owned();
        assert!(!forwards.contains(&format!("tcp:{port} ")), "{forwards}");
    }

    /// What the helper reads must match what the old method reads, where a phone allows both.
    #[test]
    #[ignore]
    fn e2e_helper_contacts_match_the_old_method() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let old = shell(
            &serial,
            "content query --uri content://com.android.contacts/data --projection contact_id:mimetype:display_name:data1",
        );
        let (text, json) = start(&serial).unwrap().read("contacts.txt").unwrap();
        assert!(!crate::export::people(&old).is_empty());
        // The two methods may list a contact's fields in a different order.
        let sorted = |rows: &str| {
            let mut people = crate::export::people(rows);
            people.iter_mut().for_each(|p| p.1.sort());
            people
        };
        assert_eq!(sorted(&text), sorted(&old));
        let items: Vec<Value> = serde_json::from_str(&json).unwrap();
        // The old method folds linked copies of a contact into one row; the helper keeps each.
        assert!(items.len() >= old.matches("Row: ").count());
    }

    /// The seed's incoming message reaches the helper. Android may hide its shell-written one from
    /// the helper; the backup must keep that one anyway. Emulator only: see docs/e2e-seed.sh.
    #[test]
    #[ignore]
    fn e2e_helper_messages_lose_nothing() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let old = shell(
            &serial,
            "content query --uri content://sms --projection address:date:type:body",
        );
        let (text, json) = start(&serial).unwrap().read("sms.txt").unwrap();
        let keys = ["address", "date", "type", "body"];
        let old_rows = crate::export::rows(&old, &keys);
        let rows = crate::export::rows(&text, &keys);
        assert!(rows.iter().any(|m| m[3] == "E2E incoming"), "{text}");
        assert!(rows.iter().all(|m| old_rows.contains(m)), "{text}\n{old}");
        let items: Vec<Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(items.len(), rows.len());

        let (kept, _) = fuller(old, Some((text, json)));
        assert!(kept.contains("E2E hello, 日本"), "{kept}");
        assert!(kept.contains("E2E incoming"), "{kept}");
    }

    /// What the helper reads must match what the old method reads, where a phone allows both.
    #[test]
    #[ignore]
    fn e2e_helper_calls_match_the_old_method() {
        let serial = std::env::var("PB_E2E_SERIAL").expect("PB_E2E_SERIAL");
        let old = shell(
            &serial,
            "content query --uri content://call_log/calls --projection number:name:date:duration:type",
        );
        let (text, json) = start(&serial).unwrap().read("call_log.txt").unwrap();
        let sorted = |rows: &str| {
            let mut calls =
                crate::export::rows(rows, &["number", "name", "date", "duration", "type"]);
            calls.sort();
            calls
        };
        assert_eq!(sorted(&text), sorted(&old));
        assert!(
            sorted(&text).iter().any(|c| c[1] == "E2E, caller"),
            "{text}"
        );
        let items: Vec<Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(items.len(), sorted(&text).len());
    }

    #[test]
    fn the_fuller_answer_wins() {
        let helper = |text: &str| Some((text.to_string(), "json".to_string()));
        let two = "Row: 0 a=1\nRow: 1 a=2\n";
        let one = "Row: 0 a=1\n";
        let refused = "Error: permission denied\n";
        let none = "No result found.\n";
        for old in [refused, none, one, two] {
            assert_eq!(
                fuller(old.to_string(), helper(two)),
                (two.to_string(), Some("json".to_string()))
            );
        }
        assert_eq!(
            fuller(two.to_string(), helper(one)),
            (two.to_string(), None)
        );
        assert_eq!(fuller(one.to_string(), None), (one.to_string(), None));
    }

    #[test]
    fn notes_say_only_what_the_user_can_miss() {
        let row = "Row: 0 a=1\n";
        let none = "No result found.\n";
        let refused = "Error: permission denied\n";
        let hidden = note("sms.txt", refused, row, true, None).unwrap();
        assert!(hidden.starts_with("Some messages may be missing"));
        assert_eq!(note("sms.txt", none, none, true, None), Some(hidden));
        assert_eq!(note("sms.txt", row, row, true, None), None);
        assert_eq!(note("sms.txt", refused, refused, false, None), None);
        assert_eq!(note("contacts.txt", refused, row, true, None), None);

        let why = Some("Why.");
        for file in ["sms.txt", "contacts.txt", "call_log.txt"] {
            assert_eq!(note(file, refused, refused, false, why).unwrap(), "Why.");
        }
        assert_eq!(note("contacts.txt", row, row, true, why), None);
        assert_eq!(note("calendar.txt", refused, refused, false, why), None);
    }

    /// A list from a fake helper that sends these lines and then hangs up.
    fn list_from(sent: &'static str) -> Option<Vec<Value>> {
        let server = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(server.local_addr().unwrap()).unwrap();
        std::thread::spawn(move || {
            let (mut phone, _) = server.accept().unwrap();
            // Hanging up with the request unread would throw away what was sent.
            BufReader::new(&phone)
                .read_line(&mut String::new())
                .unwrap();
            phone.write_all(sent.as_bytes()).unwrap();
        });
        list(&client, "contacts")
    }

    #[test]
    fn only_a_whole_list_counts() {
        let two = "{\"id\":\"contacts\",\"item\":{\"a\":\"1\"}}\n{\"id\":\"contacts\",\"item\":{\"a\":\"2\"}}\n";
        let whole = format!("{two}{{\"id\":\"contacts\",\"end\":true,\"count\":2}}\n");
        assert_eq!(list_from(whole.leak()).unwrap().len(), 2);
        let short = format!("{two}{{\"id\":\"contacts\",\"end\":true,\"count\":3}}\n");
        assert!(list_from(short.leak()).is_none());
        assert!(list_from(two).is_none(), "no end line");
        assert!(
            list_from("{\"id\":\"contacts\",\"ok\":false,\"error\":\"permission_denied\"}\n")
                .is_none()
        );
        assert!(
            list_from("{\"id\":\"contacts\",\"item\":{\"a\":\"1").is_none(),
            "cut mid-line"
        );
        assert!(
            list_from("{\"id\":\"contacts\",\"end\":true,\"count\":0}\n").is_none(),
            "empty may mean hidden"
        );
        assert!(
            list_from("{\"id\":\"sms\",\"item\":{}}\n{\"id\":\"sms\",\"end\":true,\"count\":1}\n")
                .is_none(),
            "another list's lines"
        );
    }

    #[test]
    fn message_rows_read_back_like_the_old_method() {
        let items = serde_json::json!([
            {"address":"+91 1","date":"5","type":"1","body":"a, b\nsecond line 日本"},
            {"date":"6","type":"2","body":"no number","subject":"not in the text form"},
        ]);
        let keys = ["address", "date", "type", "body"];
        assert_eq!(
            crate::export::rows(&text_rows(items.as_array().unwrap(), &keys), &keys),
            [
                ["+91 1", "5", "1", "a, b\nsecond line 日本"],
                ["", "6", "2", "no number"],
            ]
        );
    }

    #[test]
    fn contact_rows_read_back_like_the_old_method() {
        let phone = "vnd.android.cursor.item/phone_v2";
        let email = "vnd.android.cursor.item/email_v2";
        let items = serde_json::json!([
            {"contact_id":"7","mimetype":phone,"display_name":"Ann, \"Jr\" 日本","data1":"+91 1\n2"},
            {"contact_id":"7","mimetype":email,"display_name":"Ann, \"Jr\" 日本","data1":"a@b.c"},
            {"contact_id":"8","mimetype":phone,"data1":"3"},
            {"contact_id":"9","mimetype":"vnd.android.cursor.item/name","display_name":"Skip"},
        ]);
        let keys = ["contact_id", "mimetype", "display_name", "data1"];
        let people = crate::export::people(&text_rows(items.as_array().unwrap(), &keys));
        assert_eq!(
            people,
            [
                (
                    "Ann, \"Jr\" 日本".to_string(),
                    vec![
                        ("TEL", "+91 1\n2".to_string()),
                        ("EMAIL", "a@b.c".to_string())
                    ],
                ),
                (String::new(), vec![("TEL", "3".to_string())]),
            ]
        );
    }

    #[test]
    fn tokens_are_long_and_differ() {
        assert_eq!(token().len(), 32);
        assert_ne!(token(), token());
    }
}

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
    /// Contacts as (`content query` text, so the old method's readers keep working; the helper's
    /// full JSON). None unless the whole list arrived: a part must never replace the last full copy.
    pub fn contacts(&self) -> Option<(String, String)> {
        let items = list(&self.socket, "contacts")?;
        let json: Vec<String> = items.iter().map(Value::to_string).collect();
        Some((
            contact_rows(&items),
            format!("[\n{}\n]\n", json.join(",\n")),
        ))
    }
}

/// Reads one list to its end line, whose count must match. None on an error, Stop or a cut.
fn list(mut socket: &TcpStream, kind: &str) -> Option<Vec<Value>> {
    // A big list takes the phone a while to start sending.
    socket
        .set_read_timeout(Some(Duration::from_secs(60)))
        .ok()?;
    writeln!(socket, r#"{{"id":2,"op":"list","kind":"{kind}"}}"#).ok()?;
    let mut lines = BufReader::new(socket);
    let mut items = Vec::new();
    let mut line = String::new();
    while !stopped() {
        line.clear();
        lines.read_line(&mut line).ok()?;
        let mut reply: Value = serde_json::from_str(&line).ok()?;
        if reply["end"] == true {
            return (reply["count"] == items.len()).then_some(items);
        }
        items.push(reply.get_mut("item")?.take());
    }
    None
}

/// The helper's rows in the text form `content query` prints.
fn contact_rows(items: &[Value]) -> String {
    if items.is_empty() {
        return "No result found.\n".into();
    }
    let mut out = String::new();
    for (n, item) in items.iter().enumerate() {
        let get = |key: &str| item[key].as_str().unwrap_or("NULL").to_string();
        let (id, mime, name, value) = (
            get("contact_id"),
            get("mimetype"),
            get("display_name"),
            get("data1"),
        );
        out += &format!(
            "Row: {n} contact_id={id}, mimetype={mime}, display_name={name}, data1={value}\n"
        );
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
        let (text, json) = start(&serial).unwrap().contacts().unwrap();
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
        let two = "{\"item\":{\"a\":\"1\"}}\n{\"item\":{\"a\":\"2\"}}\n";
        let whole = format!("{two}{{\"end\":true,\"count\":2}}\n");
        assert_eq!(list_from(whole.leak()).unwrap().len(), 2);
        let short = format!("{two}{{\"end\":true,\"count\":3}}\n");
        assert!(list_from(short.leak()).is_none());
        assert!(list_from(two).is_none(), "no end line");
        assert!(list_from("{\"id\":2,\"ok\":false,\"error\":\"permission_denied\"}\n").is_none());
        assert!(list_from("{\"item\":{\"a\":\"1").is_none(), "cut mid-line");
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
        let people = crate::export::people(&contact_rows(items.as_array().unwrap()));
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
        assert!(contact_rows(&[]).starts_with("No result"));
    }

    #[test]
    fn tokens_are_long_and_differ() {
        assert_eq!(token().len(), 32);
        assert_ne!(token(), token());
    }
}

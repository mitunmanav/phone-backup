//! Runs the Android helper app for one job: installed fresh, then removed again.
//! See docs/android-design.md section 3.2.

use super::*;
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

    #[test]
    fn tokens_are_long_and_differ() {
        assert_eq!(token().len(), 32);
        assert_ne!(token(), token());
    }
}

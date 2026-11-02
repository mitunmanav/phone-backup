//! Puts the helper APK where src/backup/helper.rs embeds it from. Without a built APK (no Android
//! tools on this computer) the file is empty: the app still builds, only the helper is missing.

use std::path::Path;

fn main() {
    // Not the APK itself: cargo counts a missing watched file as changed and would rebuild every time.
    println!("cargo::rerun-if-changed=companion");
    println!("cargo::rerun-if-changed=build.rs");
    let out = std::env::var("OUT_DIR").unwrap();
    let embedded = Path::new(&out).join("helper.apk");
    let apk = "companion/build/outputs/apk/release/companion-release.apk";
    if std::fs::copy(apk, &embedded).is_err() {
        std::fs::write(&embedded, b"").unwrap();
    }
}

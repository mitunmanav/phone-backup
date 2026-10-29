# Phone Backup

[![CI](https://github.com/mitunmanav/phone-backup/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/mitunmanav/phone-backup/actions/workflows/ci.yml)

Phone Backup is a free Windows app for backing up an Android phone over USB, Wi-Fi, or MTP. It needs no account or cloud service; backups stay on your computer.

Website: <https://mitunmanav.github.io/phone-backup/>

## Features

- Back up photos, videos, and files from phone storage and SD cards.
- Save installed apps as APK files. App logins and private app data are not included.
- Back up contacts, call history, and messages over USB on phones that allow it. Contacts are also saved as `contacts.vcf`.
- Restore photos and files, all of them or only the ones you pick from a list with previews, and reinstall apps from a backup.
- Keep a separate folder for each phone; later backups copy only new or changed files.

## Download

Download `PhoneBackup-<version>-windows-x64.exe` from [GitHub Releases](https://github.com/mitunmanav/phone-backup/releases). It needs no installation. Windows may warn that the app is not code-signed; choose **More info**, then **Run anyway**. For ADB features, download Google's [Android Platform Tools](https://developer.android.com/tools/releases/platform-tools) and place `adb.exe` next to the app. MTP file backups do not need ADB.

## Connect a phone

- **MTP:** Unlock the phone, connect it by USB, then choose **File transfer** in the phone's USB notification. This supports photos and files.
- **USB debugging:** Enable USB debugging in Developer options. This is needed for apps, contacts, call history, and messages. Accept the authorization prompt on the phone.
- **Wi-Fi:** Pair through Android's Wireless debugging screen, then connect using the address shown by the phone.

Some Android versions and manufacturers restrict access to personal data over ADB. The app reports when a selected data type is unavailable.

## Build from source

Install the stable Rust toolchain with [rustup](https://rustup.rs/), then run these commands from the repository root on Windows:

```powershell
cargo build --locked --release
```

The executable is `target/release/phone-backup.exe`.

## Developer checks

Run these before opening a pull request:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The GitHub Actions CI workflow runs these checks and builds a release binary on Windows for every pull request. When a new version in `Cargo.toml` reaches `main`, the Release workflow runs the checks again, tags that version and publishes the executable with a SHA-256 checksum. How versions are bumped is in [CONTRIBUTING.md](CONTRIBUTING.md).

## Contributing and security

See [CONTRIBUTING.md](https://github.com/mitunmanav/phone-backup/blob/main/CONTRIBUTING.md) for development and pull request guidance. Report security issues privately using the process in [SECURITY.md](https://github.com/mitunmanav/phone-backup/blob/main/SECURITY.md).

The app is written in Rust with [egui](https://github.com/emilk/egui). See the [Android expansion plan](https://github.com/mitunmanav/phone-backup/blob/main/docs/android-design.md) and [roadmap](https://github.com/mitunmanav/phone-backup/blob/main/docs/PLAN.md).

## License

[GPL-3.0-or-later](LICENSE). You may use, share, and modify the app; distributed copies and modified versions must remain under the same license.

# Phone Backup

[![CI](https://github.com/mitunmanav/phone-backup/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/mitunmanav/phone-backup/actions/workflows/ci.yml)

Phone Backup is a free Windows app for backing up an Android phone over USB, Wi-Fi, or MTP. It needs no account or cloud service; backups stay on your computer.

Website: <https://mitunmanav.github.io/phone-backup/>

## Features

- Back up photos, videos, and files from phone storage and SD cards. Choose single folders (for example only DCIM), types (photos, videos, music, documents), and skip files over a size, with the size of each choice shown first. Caches, picture previews, WhatsApp stickers, and WhatsApp's copies of sent media are skipped.
- Save installed apps as APK files. App logins and private app data are not included.
- Back up contacts, call history, messages, picture messages (MMS) with their attachments, and the calendar over USB on phones that allow it. Contacts are also saved as `contacts.vcf`.
- Export a backup's contacts, call history and messages as CSV and as pages that open offline and print (messages also as text, with a search box on the page), the calendar as an `.ics` file, and its photos and videos sorted into year and month folders. Exports go to the backup's `exports` folder; the backup itself is not changed.
- Restore photos and files, all of them or only the ones you pick from a list with previews, and reinstall apps from a backup. The list marks large files, duplicates, files over 3 years old, and empty files, so you can untick them in one click.
- Copy one phone to another in one go: the app backs up the first phone, shows what will change, optionally backs up the second phone first, then restores to it. Both phones need USB debugging.
- See each backup's size and date; open, rename or delete it (deleting moves it to the Recycle Bin), or copy its files into another backup without replacing anything.
- Keep a separate folder for each phone; later backups copy only new or changed files.
- Tools for a backup: find duplicate files, save HEIC photos as JPG with their date and location, merge duplicate contacts into a new file, and delete chosen files from the phone once their backup copy matches. If [scrcpy](https://github.com/Genymobile/scrcpy) is next to the app or on PATH, open the phone's screen from the Tools page.
- Save a report after every backup and restore: what was copied and what failed. Optionally play a sound when a job ends.
- Save a SHA-256 checksum of every file (`checksums.sha256`, which `sha256sum -c` also reads), and check a backup with **Verify backup** on the Restore page.
- Warn when the phone's battery is under 20% and not charging, keep the computer awake while a job runs, and show the time left.
- Save a diagnostics file under Settings to attach to a bug report after reading it.
- Choose the backup folder and a light or dark theme (or follow Windows) under Settings, the last tab at the top. Settings also opens the app's privacy notice and license, and shows the first-run steps again (what the app does, adb, USB debugging and the backup folder).

## Download

Download `PhoneBackup-<version>-windows-x64.exe` from [GitHub Releases](https://github.com/mitunmanav/phone-backup/releases). It needs no installation. Windows may warn that the app is not code-signed; choose **More info**, then **Run anyway**. For ADB features, download Google's [Android Platform Tools](https://developer.android.com/tools/releases/platform-tools) and place `adb.exe` next to the app. MTP file backups do not need ADB.

## Connect a phone

- **MTP:** Unlock the phone, connect it by USB, then choose **File transfer** in the phone's USB notification. This supports photos and files.
- **USB debugging:** Enable USB debugging in Developer options. This is needed for apps, contacts, call history, messages, the calendar, and choosing folders by size. Accept the authorization prompt on the phone. The phone card shows the steps for Samsung, Xiaomi, OnePlus, Vivo, Pixel and other phones, and what to try when the phone does not show up.
- **Wi-Fi:** On the phone, open Wireless debugging and tap **Pair device with QR code**, then scan the code the app shows; the app pairs and connects. You can also type the pairing code and the address the phone shows.

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

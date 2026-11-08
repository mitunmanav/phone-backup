<div align="center">

# Phone Backup

**Back up, restore and move your Android phone with a free Windows app.**<br>
No account, no cloud. Your backup is an ordinary folder on your PC.

[![Latest release](https://img.shields.io/github/v/release/mitunmanav/phone-backup?label=release)](https://github.com/mitunmanav/phone-backup/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/mitunmanav/phone-backup/total)](https://github.com/mitunmanav/phone-backup/releases)
[![CI](https://github.com/mitunmanav/phone-backup/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/mitunmanav/phone-backup/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/github/license/mitunmanav/phone-backup)](LICENSE)

[**Download**](https://phonebackup.oddlybuilt.in/download) | [Website](https://phonebackup.oddlybuilt.in) | [Guide](https://phonebackup.oddlybuilt.in/guide) | [FAQ](https://phonebackup.oddlybuilt.in/faq)

<picture>
  <source srcset="docs/app-dark.png" media="(prefers-color-scheme: dark)">
  <img src="docs/app-light.png" width="1100" alt="Phone Backup copying a phone to the PC: a cable from the phone to the backup folder, lit as far as the copy has got, above tiles for each kind of file.">
</picture>

</div>

## What it does

| | |
|---|---|
| **Back up** | Photos, videos, files, SD cards and installed apps, plus contacts, call history, messages and the calendar. Pick whole types or single folders; later backups copy only what changed, and you can pause and carry on later. |
| **Restore** | Put files and apps back on any Android phone, all of them or picked from a list with previews. Files the phone already has are never replaced. **Verify** on the Restore page checks every file against its SHA-256 checksum. |
| **Phone to phone** | Copy one phone straight to another, after seeing what will change. |
| **Read and export** | Read a backup's messages, contacts, calls and calendar in the app. Export them as spreadsheets, printable pages, `contacts.vcf` and `.ics`, and sort photos by month. |
| **Tools** | Find duplicates, turn HEIC photos into JPG, merge duplicate contacts, free up phone space once files are safely backed up, and open the phone's screen with [scrcpy](https://github.com/Genymobile/scrcpy). |
| **Connect your way** | USB file transfer, USB debugging, or Wi-Fi by scanning a QR code. The app shows the steps for your phone's brand and can fetch Google's adb for you. |

## Get started

1. [Download](https://phonebackup.oddlybuilt.in/download) `PhoneBackup-<version>-windows-x64.exe` and double-click it. Nothing to install. If Windows warns that the app is not code-signed, choose **More info**, then **Run anyway**.
2. Plug in your phone, unlock it, and choose **File transfer** in its USB notification. For apps, contacts and messages, turn on USB debugging; the app shows how.
3. Press **Back up now**.

The [guide](https://phonebackup.oddlybuilt.in/guide) covers Wi-Fi, restoring and moving to a new phone. Some phones do not allow reading personal data over USB; the app says when that happens.

## Privacy

The app goes online only when you click **Get adb** (to download it from Google) or **Check for updates** (to ask GitHub). It never sends your data: no account, ads or tracking. To read contacts, calls, messages and the calendar, it puts a small open-source helper app on the phone and removes it afterwards. Backups are not encrypted, so keep them on a drive only you use. Details are in the [FAQ](https://phonebackup.oddlybuilt.in/faq#privacy).

## Build from source

Install the stable Rust toolchain with [rustup](https://rustup.rs/), then run this from the repository root on Windows:

```powershell
cargo build --locked --release
```

The executable is `target/release/phone-backup.exe`. The app is written in Rust with [egui](https://github.com/emilk/egui); the phone helper in `companion/` is Kotlin.

## Developer checks

Run these before opening a pull request:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

CI runs the same checks and a release build on Windows for every pull request. When a new version in `Cargo.toml` reaches `main`, the Release workflow tags it and publishes the executable with a SHA-256 checksum.

## Contributing

Bug reports and pull requests are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). Report security problems privately as described in [SECURITY.md](SECURITY.md). Plans are in the [roadmap](docs/PLAN.md) and the [Android helper design](docs/android-design.md).

## License

[GPL-3.0-or-later](LICENSE). Use, share and change it freely; shared copies and changed versions must stay under the same license.

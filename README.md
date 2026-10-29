# Phone Backup

A free Windows app that backs up an Android phone to your computer over a USB cable or Wi-Fi. No account, no cloud: everything stays on your computer.

Website: https://mitunmanav.github.io/phone-backup/

## What it backs up

- Photos, videos and files from the phone's storage and SD card
- Installed apps (without their logins or data)
- Contacts (also saved as `contacts.vcf`), call history and messages, on phones that allow it over USB

It can also restore photos and files to a phone and reinstall apps from a backup. Each phone gets its own folder, and later backups copy only new or changed files.

## How to connect

- **File transfer (MTP):** plug in the phone, unlock it, tap the USB notification and choose File transfer. Photos and files only.
- **USB debugging:** needed for apps, contacts and messages. Put `adb.exe` from Google's [Platform Tools](https://developer.android.com/tools/releases/platform-tools) next to the app.
- **Wi-Fi:** use Wireless debugging on the phone and pair with the code it shows.

## Build from source

Needs [Rust](https://rustup.rs/) (edition 2024).

```
cargo build --release
```

The app is written in Rust with [egui](https://github.com/emilk/egui). See [docs/PLAN.md](docs/PLAN.md) for the roadmap.

## License

[GPL-3.0-or-later](LICENSE). You may use, share and change it; copies and changed versions must stay open source under the same license.

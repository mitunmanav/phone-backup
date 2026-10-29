# Hardening and feature plan

The Edge cases and Features tables are done; v1 and the nice-to-have list are next. The Android companion plan is in [android-design.md](android-design.md).

Each item lands as its own pull request and is merged into `main` after CI passes.

## Edge cases

| # | Problem today | Fix | Branch |
|---|---|---|---|
| E1 | Phone unplugged before or during the file listing: the listing comes back empty and the app reports "Backup finished" with 0 files. | Check `adb get-state` after listing and after copying; report "The phone was disconnected" instead of a false success. | `fix/disconnect` |
| E2 | `adb` not on PATH (normal for non-developers): ADB phones are never seen and the screen keeps saying "turn on USB debugging", which cannot help. | Look for adb next to the app, then on PATH, then in the default Android SDK folder. Tell the user when adb is missing. | `fix/adb-discovery` |
| E3 | adb state `authorizing` is ignored, so the screen says "Connect your phone" while the phone shows the Allow prompt. | Treat `authorizing` like `unauthorized`. | `fix/adb-discovery` |
| E4 | One phone on USB and Wi-Fi at once shows as "2 phones" and is backed up twice. | When several ADB devices are listed, drop entries with a repeated `ro.serialno`. | `fix/duplicate-phone` |
| E5 | Windows ignores letter case, so `IMG.jpg` and `img.jpg` (or `a:b` and `a_b` after renaming) overwrite each other and are re-copied and reported as errors on every run. | Give later clashing files a ` (2)` suffix. | `fix/name-clash` |
| E6 | Local paths over 260 characters: `adb pull` (not long-path aware) fails for deep folders. | Pull those files to a short temporary name, then move them into place from Rust, which handles long paths. | `fix/long-paths` |
| E7 | Computer disk fills up mid-backup; every remaining file fails with a vague error. | Check free space before copying and stop with "needs X, Y free". | `fix/disk-space` |
| E8 | No way to stop a backup; closing the window leaves `adb pull` running in the background. | Stop button; closing during a backup asks first; the running `adb` process is killed. | `feature/stop` |
| E9 | Wi-Fi address check only looks for `:`; `1.2.3.4:` passes. Pair/Connect can be clicked repeatedly while adb is still working. | Require a host and a numeric port; disable the buttons while working. | `fix/wifi-input` |
| E10 | Progress only moves once per folder, so a 10,000-photo Camera folder sits at one number for many minutes. | Copy in batches of at most 200 files and report after each batch. | `fix/progress` |
| E11 | Phone with no readable files says "All 0 files were already backed up". | Say "No files were found on the phone." | `fix/disconnect` |

Checked and not a bug: if one file in a multi-file `adb pull` fails (deleted mid-backup), adb carries on with the rest and removes the partial file (AOSP `file_sync_client.cpp`, `do_sync_pull`).

## Features

| # | Feature | Branch |
|---|---|---|
| F1 | Choose what to back up: files, apps, contacts/calls/messages. Remembered between runs. | `feature/choose-data` |
| F2 | Contacts also saved as `contacts.vcf`, which any phone or Google Contacts can import. | `feature/vcard` |
| F3 | Restore apps from a backup folder to the connected phone (`adb install-multiple`). | `feature/restore-apps` |
| F4 | Taskbar flashes when a backup finishes while the window is in the background. | `feature/stop` |

## v1 features

Ordered from cheapest to largest. Items 1 to 4 come first.

| # | Feature | Size |
|---|---|---|
| V1 | Before a restore, list what will be added, skipped, or cannot move and why. | Small |
| V2 | Save a report after every backup or restore: what was copied and what failed. | Small |
| V3 | `manifest.json` with a SHA-256 hash per file, a "Verify backup" check, and a phone battery check before starting. | Small |
| V4 | Back up the calendar and MMS messages. | Small |
| V5 | Export contacts and messages as CSV, the calendar as ICS, and messages as a self-contained HTML page that opens offline. | Medium |
| V6 | Wi-Fi pairing by QR code instead of typing the pairing code. | Medium |
| V7 | Phone to phone: back up one phone and restore it to another in one flow. | Medium |
| V8 | Tools: HEIC to JPG, duplicate finder, launch scrcpy if it is installed. | Medium |
| V9 | Optional password-protected backups (AES-256-GCM, Argon2id key). | Medium |
| V10 | Read WhatsApp backups with the user's own key: view, search and export. | Large |
| V11 | Restore contacts, messages, call history and calendar to a phone. Messages and call history need a small companion Android app, so this may slip to v1.5. | Large |
| V12 | Import backups made by other phone-transfer apps. | Large |

## Nice-to-have features

Do these first: time left, keep awake, battery warning, delete a backup, pick single folders, and a safety copy before a restore.

| Area | Features |
|---|---|
| During a backup | Time left ("about 12 min"); keep the computer from sleeping; warn when the phone battery is under 20%; show the folder being copied; progress on the taskbar icon. |
| Backup list | Size and date of each backup; delete an old backup (asks first); rename a backup; browse its contents in the app. |
| Choosing | Pick single folders (for example only DCIM); skip junk such as cache, stickers and sent-media copies; show the size of each choice before starting; skip files over a chosen size. |
| Restore | Pick files from a list with thumbnails; safety copy of anything that would be replaced; re-check files after copying. |
| Contacts and messages | Merge duplicate contacts with undo; search messages inside a backup; print or save messages as PDF. |
| Photos | HEIC to JPG keeping date and location; sort into year and month folders on export. |
| Connecting | USB debugging steps per brand (Samsung, Xiaomi and others); ignore USB devices that are not phones; a "Phone not showing up?" helper. |
| Trust and help | "Export diagnostics": a local zip of logs the user checks before sharing; a plain list of what the app never does (no internet, no ads, no auto-start). |
| Comfort | Keyboard shortcuts; screen-reader labels; large text support; drop a backup folder on the window to open it. |
| After a job | Save the report as a file; a "Copy error list" button; optional sound or notification when done. |

## Out of scope

Scheduled backups, iPhone support, and app private data (Android blocks it without root). Never: starting with Windows, reminder pop-ups, ads, accounts or telemetry.

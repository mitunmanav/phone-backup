# 13 iPhone to Android

**Branch:** `plan/13-iphone-to-android` · **Depends on:** 01 (restore personal data) · **Size:** large

## Why

Moving from iPhone to Android is a core selling point of MobileTrans, Dr.Fone, AnyDroid,
Syncios, EaseUS, Smart Switch, Clone Phone and HiSuite. Google's own setup copies some of
it, but only during setup and only with a cable. PLAN.md lists "iPhone backups and iPhone
to Android" as an idea. The safe, offline route needs no iPhone driver code of our own:
the user makes a normal backup with Apple's app on Windows, and we read that backup.

## Features in this group

| Feature (catalog row I) | Today | After this group |
|---|---|---|
| Read an iTunes / Apple Devices backup | — | Find backups in the default Windows locations, list them by device name and date |
| iPhone photos and videos to Android | — | Copy Camera Roll (HEIC kept, optional JPG copies via the existing tool) into a backup, then restore |
| iPhone contacts to Android | — | Read `AddressBook.sqlitedb` into contacts JSON; restore with 01 |
| iPhone messages to Android | — | Read `sms.db` (SMS and iMessage text, attachments) into SMS/MMS JSON; restore with 01 |
| iPhone call history to Android | — | Read `CallHistory.storedata`; restore with 01 |
| iPhone calendar to Android | — | Read `Calendar.sqlitedb`; restore with 01 |
| Encrypted iPhone backups | — | Later step: unlock with the backup password |
| Read the iPhone backup in the app | — | Same read page and exports as Android backups |

## What exists now

- Restore, read page and exports work on the backup folder layout; 01 adds personal-data
  restore from `data/*.json`.
- `tools.rs` HEIC to JPG.

## Design

- **Where backups are:** `%APPDATA%\Apple Computer\MobileSync\Backup\<udid>` (iTunes
  desktop) and `%USERPROFILE%\Apple\MobileSync\Backup\<udid>` (Microsoft Store iTunes and
  Apple Devices). `Info.plist` gives device name, iOS version, date; `Manifest.plist` says
  if encrypted.
- **File map:** `Manifest.db` (SQLite) maps `domain` + `relativePath` to the hashed file
  name `<first two hex>/<sha1>`. Use `rusqlite` (bundled) and `plist` crates.
- **Conversion, not a new reader:** `src/iphone.rs` writes a normal Phone Backup folder
  `iPhone_<name>_<udid8>/` with `files/DCIM/…`, `data/contacts.json`, `sms.json`,
  `mms/`, `call_log.json`, `calendar.json` in the helper's JSON shape and the `.txt`
  form, plus `report.txt`. Then every existing feature (read, export, verify, restore via
  01) just works.
- **Mappings:** iMessage and SMS both become SMS rows (type from `is_from_me`); group
  chats become MMS with several addresses; Apple dates (seconds or nanoseconds since
  2001-01-01) converted to Unix ms; tapbacks dropped or turned into text ("Liked …").
- **Encrypted backups (later step):** keybag in `Manifest.plist`, password → PBKDF2
  (SHA-256 then SHA-1 rounds as iOS 10.2+ uses) → unwrap class keys → decrypt
  `Manifest.db` and each file's key. Tested only with backups we make from a test iPhone.

## Steps

1. Find and list backups; `Info.plist`, `Manifest.plist`; tests with a synthetic backup
   folder.
2. `Manifest.db` file map; copy Camera Roll to `files/DCIM/`.
3. Contacts conversion; tests on a synthetic `AddressBook.sqlitedb` built in the test.
4. Messages conversion with attachments; tests on synthetic `sms.db`.
5. Calls and calendar conversion.
6. UI: "Bring in an iPhone backup" on the Restore page and Settings; progress, Stop.
7. Encrypted backups.
8. Docs: guide "From iPhone to Android" with Apple Devices / iTunes steps.

## Tests

- Synthetic SQLite and plist fixtures generated in tests (no real data in the repo).
- Manual: one real iPhone backup (unencrypted, then encrypted) → Pixel.

## Risks and open questions

- Apple changes schemas across iOS versions; support the last three majors and explain
  others.
- Very large photo libraries with "Optimise iPhone storage" are not fully in the backup;
  say "turn on Download and Keep Originals first".
- Live iPhone access (libimobiledevice) and Android → iPhone are not in this group.

## Not in this group

WhatsApp iPhone → Android (out of scope), iCloud download (needs Apple sign-in; not
planned), Mac support (16).

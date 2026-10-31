# Android design: companion app, restore and transfer

This is the plan for the next stage of Phone Backup on Android. Today the app backs up files, apps, and (when the phone allows it) contacts, call history and messages, and it can reinstall apps. This document covers what is still missing and how to build it with public Android APIs only.

## 1. Goals

1. Back up everything an unrooted Android phone allows, on every phone, not only the ones that let ADB read personal data.
2. Restore it to the same phone or to another one.
3. Export it to formats people can open without Phone Backup.

The app stays free, offline and account-free.

## 2. What an unrooted phone allows

| Data | Read with | Restore with | Needs the companion app |
|---|---|---|---|
| Photos, videos, music, documents, downloads, ringtones, wallpapers | `adb pull` / MTP (done) | `adb push`, then a media scan | No |
| Installed apps (base and split APKs) | `pm path` + `adb pull` (done) | `adb install-multiple` (done) | No |
| Contacts, with groups and photos | `ContactsContract` | `ContactsContract` batch insert | Yes |
| SMS | `Telephony.Sms` | Only the default SMS app may write | Yes, plus a temporary default-SMS switch |
| MMS, with attachments | `Telephony.Mms` and its parts | Same as SMS | Yes |
| Call history | `CallLog.Calls` | `CallLog.Calls` insert | Yes |
| Calendar events and reminders | `CalendarContract` | `CalendarContract` insert into a local account | Yes |
| Blocked numbers | `BlockedNumberContract` | Only the default dialer or SMS app may write | Yes; later |
| Device info (model, Android version, storage, battery) | `getprop` (`ro.product.manufacturer`, `ro.product.model`, `ro.config.marketing_name` where the vendor sets it, `ro.build.version.release`, `ro.build.version.sdk`), `dumpsys battery`, `df` | n/a | No |
| Alarms, browser bookmarks, system settings | No public API on current Android | No | Not supported; the UI says so |
| App private data | Needs root | No | Not supported (WhatsApp files are an exception, see section 7) |

Today contacts, call history and messages are read with `adb shell content query`. Many phones refuse this because the shell user lacks the permission. An ordinary app the user grants permission to can read these providers on every phone, which is why a companion app is needed.

## 3. Architecture

```
Windows PC (Rust app)                       Android phone
---------------------                       -------------
adb  --install / start------------------->  Companion app (Kotlin)
     --adb forward tcp:0 tcp:P----------->    foreground service
Rust client  <== JSON lines over TCP ==>      TCP server on 127.0.0.1:P
     <== adb pull / adb push (files) ===>   shared storage
```

- **Files keep using `adb pull` / `adb push`.** It is fast and already written. The companion app only handles data that lives in content providers.
- **Personal data goes over one TCP connection forwarded by adb.** The phone listens on loopback only, so nothing is exposed on Wi-Fi.
- **Wi-Fi mode works unchanged:** pair and connect with adb over Wi-Fi as today, then forward the same port.

### 3.1 Companion app

- New open-source Kotlin project with its own package name (proposal: `org.phonebackup.companion`).
- `minSdk 23` (Android 6, where runtime permissions start). `targetSdk` follows the current Google Play requirement.
- Components:
  - `MainActivity`: asks for permissions and shows the connection status.
  - `BridgeService`: foreground service (`foregroundServiceType="dataSync"`) with a persistent notification. Opens a `ServerSocket` bound to `127.0.0.1`. Holds a partial wake lock (plus a Wi-Fi lock in Wi-Fi mode) only while a request runs, so a long backup survives the screen turning off. No restart watchdog: the PC starts the service again if it dies.
  - The components Android requires before an app can become the default SMS app: an SMS deliver receiver, an MMS WAP-push receiver, a "respond via message" service and a compose activity. They can be minimal.
- Permissions, requested only for the data the user picked: `READ_CONTACTS`, `WRITE_CONTACTS`, `READ_SMS`, `READ_CALL_LOG`, `WRITE_CALL_LOG`, `READ_CALENDAR`, `WRITE_CALENDAR`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC`, `WAKE_LOCK`, `POST_NOTIFICATIONS` (Android 13+). Writing SMS needs no extra permission once the app is the default SMS app.
- Distribution: the PC app ships the APK next to its own executable and installs it with `adb install -r`. The APK is also published on the GitHub releases page so anyone can check the build.

### 3.2 Session start (PC side)

1. Pick the device (existing `detect()`).
2. Check `dumpsys package <companion>` for the installed `versionCode`; install or upgrade if missing or older with `adb install -r -g`. `-g` grants the runtime permissions at install time, so most phones need no permission taps. Some vendor builds ignore it, and the default-SMS role can never be granted this way; the `perms` reply shows what is still missing.
3. Start the service with `am start-foreground-service` (Android 8+), passing a random session token as an intent extra. Open the activity instead when permissions are still missing.
4. `adb forward tcp:0 tcp:<P>`; adb prints the free PC port it chose. `P` is a fixed port constant of the companion app.
5. Connect and send `hello` with the token. The service rejects any connection without it, so other apps on the phone cannot use the channel.
6. When done: `bye`, `adb forward --remove`, stop the service.

### 3.3 Protocol

Newline-delimited JSON, UTF-8, one object per line. JSON escapes newlines inside strings, so a line is always exactly one message and no length prefix is needed. Every request has an `id`. Lists stream as `item` lines followed by one `end` line.

```
PC -> {"id":1,"op":"hello","token":"...","proto":1}
PH <- {"id":1,"ok":true,"app":"1.0.0","sdk":34,"perms":{"contacts":true,"sms":false}}

PC -> {"id":2,"op":"list","kind":"contacts"}
PH <- {"id":2,"item":{...}}            (repeated)
PH <- {"id":2,"end":true,"count":812}

PC -> {"id":3,"op":"insert","kind":"sms","items":[...]}   (batches of about 200)
PH <- {"id":3,"ok":true,"inserted":200,"skipped":0}

PH <- {"id":4,"ok":false,"error":"permission_denied","need":"sms"}
```

- Operations: `hello`, `perms`, `count` (for progress bars), `list`, `insert`, `sms_role`, `bye`.
- Kinds: `contacts`, `sms`, `mms`, `calllog`, `calendar`.
- Binary data (MMS attachments, contact photos) does not travel in JSON. The phone writes it under its app-specific folder (`Android/data/<package>/files/out/`) and returns a relative path; the PC fetches it with `adb pull`. Restore mirrors this with `adb push` into `files/in/`.
- File bytes never go through this channel. `adb pull` / `adb push` already stream large files well, so the companion needs no file server of its own.
- `proto` is a version number. The PC reinstalls the companion when the phone reports an older one.

### 3.4 SMS and MMS restore

Android lets only the default SMS app write messages.

1. `sms_role` reports whether the companion is the default. If not, it asks: `RoleManager.createRequestRoleIntent(ROLE_SMS)` on Android 10+, `Telephony.Sms.Intents.ACTION_CHANGE_DEFAULT` on 6 to 9. The PC tells the user to confirm on the phone.
2. Remember the previous default (`Telephony.Sms.getDefaultSmsPackage`).
3. Insert in batches, keeping `date`, `date_sent`, `read` and `type`. Skip messages that already exist (same address, date and body hash), so restoring twice does not duplicate anything.
4. Ask the user to switch back by opening the system default-apps screen and naming their previous app. Android does not let an app give the role back by itself.
5. While the companion is the default, its receiver stores any incoming SMS in the provider so nothing is lost.

Known risk: some vendor builds (for example MIUI/HyperOS) block installs over USB unless a developer option is on, and may restrict SMS writes. Handle these when a test phone shows them.

### 3.5 File restore

- `adb push` each file back to its original path, skipping files already present with the same size (the reverse of `needs_copy`).
- Then ask the media store to rescan: `content call --uri content://media --method scan_volume --arg external_primary` on Android 10+; on older phones, have the companion call `MediaScannerConnection.scanFile`.

## 4. Backup folder

The current layout stays; new files are added beside it.

```
<backup folder>/<model>_<id>/
  files/ , sdcard_<name>/     storage mirror (existing)
  apps/<package>/*.apk        apps (existing)
  data/
    contacts.vcf              existing; later written from companion data
    contacts.json             new: all provider fields, for exact restore
    sms.json, mms/            new
    calllog.json              new
    calendar.ics              new: for other apps
    calendar.json             new: for exact restore
  manifest.json               new: app version, device info, counts, SHA-256 per data file
  report.txt                  existing
```

- Human formats (`.vcf`, `.ics`) for people; `.json` for exact restore. Restore prefers `.json`.
- The existing `content query` text files remain the fallback when the companion app is refused.
- `manifest.json` lets a Backups screen show what a backup holds and verify it.

### 4.1 Optional encryption

- One password per backup. Derive a key with Argon2id and encrypt each file with AES-256-GCM, a random nonce per file, salt and parameters stored in `manifest.json`.
- Default: encrypt the `data/` folder only. Media is large and usually not secret; encrypting it is an option.
- The password is never stored. The UI warns that a lost password means a lost backup.

## 5. Exports

| Data | Formats |
|---|---|
| Contacts | VCF (existing), CSV |
| Messages | One self-contained HTML file per conversation, CSV |
| Call history | CSV, HTML table |
| Calendar | ICS |

## 6. Phone-to-phone transfer

Back up phone A into a temporary folder, restore to phone B, then delete the temporary folder unless the user keeps it as a backup. A checklist screen shows each data type with its count and size, and lists what cannot move (app data, alarms, settings). No new protocol is needed.

## 7. WhatsApp

Only the user's own files and public formats:

1. Back up `Android/media/com.whatsapp/WhatsApp/` (and `com.whatsapp.w4b` for WhatsApp Business) with `adb pull`. This is a folder copy and can ship early.
2. Later, a read-only chat viewer: if the user enabled WhatsApp's end-to-end encrypted backup, they enter their 64-digit key and the app decrypts `msgstore.db.crypt15` (format documented by the open-source project `wa-crypt-tools`) and renders chats as HTML.
3. Not planned: downgrading WhatsApp or automating taps inside it. Both risk the user's data and break WhatsApp's terms. Moving WhatsApp between phones is handled by WhatsApp's own transfer and Google Drive backup.

## 8. Screens

1. **Device:** connected phones, battery, storage, Android version, companion status (not installed, needs permissions, ready).
2. **Back up:** existing choices plus calendar, and an encrypt option.
3. **Restore:** pick a backup and a phone, pick data types. Guided prompts for on-phone steps.
4. **Transfer:** phone A to phone B.
5. **Export:** pick a backup, a data type and a format.
6. **Backups:** list, size, date, verify, delete.

## 9. Build order

| # | Step | Size | Builds on |
|---|---|---|---|
| 1 | File restore (`adb push` + media scan) | S | `pull`, `needs_copy`, `safe_name` |
| 2 | `manifest.json` and a Backups screen with verify | S | `adb_backup` |
| 3 | Companion app v1: hello, permissions, list contacts, call history, SMS, calendar | M-L | new Kotlin project |
| 4 | Use the companion for personal data; keep `content query` as fallback | S | `adb_backup` |
| 5 | Exports: ICS, CSV, HTML conversations | M | `vcard` |
| 6 | Contacts, call history and calendar restore | M | companion `insert` |
| 7 | SMS and MMS restore with the default-SMS switch | M | companion `sms_role` |
| 8 | Phone-to-phone wizard | S | 1-7 |
| 9 | Optional encryption | S-M | `manifest.json` |
| 10 | WhatsApp folder backup, then the crypt15 viewer | S, then L | |

Each step gets its own branch and pull request, like the items in `PLAN.md`.

## 10. Testing

- Real phones: at least one Samsung, one Xiaomi and one Pixel or near-stock build; at least one on Android 8-9 and one on 13 or later.
- Round trip per data type: back up, clear test data on a spare phone, restore, compare counts and sample fields.
- Restore twice: counts must not double.
- Rust unit tests for parsers and exporters with fixture JSON; instrumented tests in the companion app for provider reads and writes.

## 11. Open questions

- Minimum Android version (proposal: 6.0).
- Publish the companion app on F-Droid as well? That needs reproducible builds.
- Later, a Wi-Fi mode without adb (companion app with QR pairing and TLS)? Larger; not needed now.

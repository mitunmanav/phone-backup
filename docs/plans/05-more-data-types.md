# 05 More data types

**Branch:** `plan/05-more-data-types` · **Depends on:** 01 for the restore halves · **Size:** medium

## Why

Smart Switch, Google One and MobileTrans move more than files and the big four: settings,
wallpapers, ringtones, blocked numbers, contact groups and photos, app permissions. Section
A of [feature-map.md](../feature-map.md) has about 40 data types; Phone Backup covers the
files, apps and personal-data rows but none of the "makes the new phone feel like the old
one" rows. Some of those are readable on an unrooted phone through adb or the helper;
others are not, and the UI should say so plainly (android-design.md section 2 already
lists alarms, bookmarks and system settings as "not supported", which this group revisits
one by one).

## Features in this group

| Data (catalog row A) | Read with | Restore with | Plan |
|---|---|---|---|
| Contact photos | helper: `ContactsContract.Contacts.openContactPhotoInputStream` (full size) → `data/contacts/<id>.jpg` | 01's contact insert | Build |
| Contact groups and labels | helper: `Groups` + `GroupMembership` rows | 01 inserts groups by title | Build |
| Blocked numbers | helper, only while default SMS app (01's role): `BlockedNumberContract` | same role, insert | Build, during restore's role window; backup only when the user agrees to the switch |
| Phone settings (display, sound, accessibility, font size, screen timeout…) | `adb shell settings list system`, `secure`, `global` | `settings put` for a **safe list** of keys only | Build; never restore device IDs, ADB, developer or carrier keys |
| Ringtone, notification and alarm sound | `settings get system ringtone` etc. + the file | push the file, `settings put` its media URI | Build |
| Wallpapers | helper: `WallpaperManager` where the phone allows (Android 12 and older; some 13+) | helper: `setStream` (SET_WALLPAPER is a normal permission) | Build with a clear "this phone does not share its wallpaper" message |
| Default apps (browser, SMS, dialer, launcher) | `cmd role get-role-holders` | Shown as a checklist; Android needs the user to choose | Build (report and reminder only) |
| App permissions | `dumpsys package <pkg>` runtime grants | `pm grant` after app restore | Build |
| App list details (version, installer, install date) | `dumpsys package` | n/a | Build; shown in report and read page |
| Wi-Fi network names | `cmd wifi list-networks` (Android 11+) | none: passwords are not readable | Build as a list in the report and transfer checklist ("rejoin these networks"); point to Android's own Wi-Fi QR share |
| Alarms, bookmarks, Samsung Notes, voicemail, keyboard dictionary | no public API | none | Keep "not supported" and list them in the UI |

## What exists now

- `adb.rs` data step runs `content query` commands and the helper (`helper.rs`) for four
  kinds; writes `data/*.txt` and `*.json`.
- `report.rs` `phone_report()` already reads `getprop`, storage and security patch.
- Restore (01) will insert contacts; apps restore through `install-multiple`.

## Design

- New files under `data/`: `settings.json` (`{"system":{…},"secure":{…},"global":{…}}`),
  `sounds/` (ringtone files plus `sounds.json`), `wallpaper/home.png`, `lock.png`,
  `blocked.json`, `roles.json`, `app_permissions.json`, `apps.json`, `wifi.txt`, and
  `contacts/<id>.jpg`.
- **Safe settings list** lives in one Rust table with a reason per key (e.g.
  `system/font_scale`, `system/screen_off_timeout`, `secure/accessibility_*`,
  `system/haptic_feedback_enabled`, `global/auto_time`). Restore only puts keys from the
  table, only if the value parses, only on the same or newer Android. Everything else is
  saved for reference but never written.
- Each new type is a tick under "Texts, calls, more" on the Back up page, off by default
  for the ones that need extra prompts (blocked numbers).

## Steps

1. **Settings backup** (`settings list` ×3) and the safe-key table; tests on captured
   output from Android 9, 12, 14.
2. **Settings restore** with the safe list; report what was set and skipped.
3. **Sounds** backup and restore.
4. **Contact photos and groups** in the helper and 01's insert.
5. **App details and permissions**: `apps.json`, `app_permissions.json`; `pm grant`
   after app restore; tests on captured `dumpsys package` output.
6. **Default apps and Wi-Fi names**: read, show in report, read page and 02's checklist.
7. **Wallpapers** through the helper, with per-version behaviour.
8. **Blocked numbers** inside 01's SMS role window (backup and restore).
9. **"Not supported" list** in the UI and FAQ, kept honest.

## Tests

- Parser tests on captured command output for each Android version; safe-list tests that
  dangerous keys are never written.
- Manual: Samsung → Pixel settings carry-over (font size, timeout), ringtone plays.

## Risks and open questions

- Vendor settings keys differ (Samsung `system/…` extras). Only restore AOSP keys.
- `pm grant` fails for permissions the target app no longer declares; report, do not stop.
- Wallpaper access tightened in Android 13+; test on a Pixel and a Samsung.

## Not in this group

App private data (out of scope), WhatsApp chats (07), alarms (no API).

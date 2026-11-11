# 12 Tools

**Branch:** `plan/12-tools` · **Depends on:** nothing · **Size:** small

## Why

The Tools page already has duplicates, HEIC to JPG, merge contacts, free up space and the
phone's screen. The catalog (sections O, P, R) shows a few more tools that fit the app's
purpose and its trust rules: erase a phone safely before selling it (Dr.Fone, MobileTrans,
Coolmuster, MobiKin all sell this), find what fills the phone, and make small copies of
photos for sharing. Ringtone makers, video converters and junk cleaners are left out:
they are off-topic or over-promise.

## Features in this group

| Feature (catalog row) | Today | After this group |
|---|---|---|
| Prepare a phone for selling (R: secure data eraser) | — | Guided: verify a backup, remove the Google account reminder (FRP), delete shared-storage files, then open the phone's own factory reset with steps per brand |
| Large file finder (P) | sizes in the picker | Biggest files and folders on the phone, with "back up then delete" |
| Empty and junk folder report (P) | some junk skipped | List of empty folders and app caches in shared storage the user can remove |
| Photo compressor (O) | — | "Make small copies" of chosen photos into `exports/small/` (long side 2048 px, JPEG 85); originals untouched |
| Storage summary (P) | phone report | Used and free by type (photos, videos, apps, other) on the device page |

## What exists now

- `tools.rs`: `duplicates`, `delete_copies`, `heic_to_jpg`, `merge_contacts`,
  `chosen_on_phone`, `match_on_phone`, `delete_from_phone` (checksum-checked),
  `scrcpy_path`.
- `tools_page.rs` UI; `report.rs` `phone_report()`.

## Design

- **Prepare for selling:** a checklist page, not one button. Factory reset is the only
  real wipe on modern encrypted phones; deleting files first is a courtesy. Steps:
  1. Check that the latest backup verified (03/Verify).
  2. Show signed-in accounts (`dumpsys account`, masked) and say "remove your Google
     account first, or the buyer will be locked out (FRP)".
  3. Optional: delete shared storage files (reuse `delete_from_phone` rules).
  4. Open Settings → Reset (`am start -a android.settings.PRIVACY_SETTINGS` or brand
     intent) and show the brand's path, like the USB-debugging steps.
  We never send a reset command ourselves.
- **Large files:** reuse `measure()`; top 100 files and top folders by size.
- **Compressor:** `image` crate resize + JPEG encode on a worker thread; keeps EXIF date.

## Steps

1. Storage summary on the device page.
2. Large file finder with back up / delete actions.
3. Empty and junk folder report.
4. Photo compressor.
5. Prepare-for-selling checklist with brand steps.

## Tests

- Unit tests for size ranking, empty-folder detection, resize maths and EXIF date kept.

## Risks and open questions

- Users may expect "erase" to wipe without a reset; the page must say plainly that the
  phone's own reset is the real wipe.

## Not in this group

Deleted-data recovery, junk "boosters", ringtone and video tools (not planned).

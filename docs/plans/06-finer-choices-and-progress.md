# 06 Finer choices and progress

**Branch:** `plan/06-finer-choices-and-progress` · **Depends on:** none (restore parts after 01) · **Size:** medium

## Why

Phone Backup already picks by type, by folder and by file, skips huge files and
duplicates, and remembers choices. Competitors go further in a few places users notice:
SMS Backup & Restore picks single conversations, Swift Backup and Neo Backup pick single
apps and save named selections, FolderSync and Syncthing filter by file pattern, and
MobileTrans, Dr.Fone and Smart Switch show time left. This group fills those holes on the
Back up, Restore and Transfer pages.

## Features in this group

| Feature (catalog row) | Today | After this group |
|---|---|---|
| Choose single apps (B) | all apps or none | Tick apps in a list with icons, sizes; "only user apps" filter |
| Choose single conversations (B, G) | all messages | Tick conversations for backup and for restore |
| Include or exclude by file type (B) | — | Simple ticks ("no screenshots", "no WhatsApp Status", "no .thumbnails") plus custom patterns |
| Saved selections (B) | one remembered choice | Named presets ("Photos only", "Everything but videos") |
| Time left estimate (C) | — | Shown after the first 30 s, from bytes per second |
| Speed shown (C) | — | MB/s in the progress line |
| Skip folders that are rebuilt by apps (B) | some junk skipped (`is_junk`) | Visible list the user can change |

## What exists now

- `src/backup.rs`: `Pick`, `KINDS`, `tally()`, `is_junk()`.
- `src/ui/picker.rs`: file and folder picker with thumbnails; `backup_tab.rs` "What to
  back up", "Skip files over".
- Apps backup reads `pm list packages` and pulls every APK.
- Progress messages flow through `Msg` in `backup.rs` and `jobs.rs`.

## Design

- **`Pick` grows** `apps: Option<BTreeSet<String>>` (None = all), `threads:
  Option<BTreeSet<String>>` (keyed by normalised address), `exclude: Vec<Pattern>` (glob,
  case-insensitive, relative to storage root). Saved in the settings file as today; old
  settings load as "all".
- **Presets:** `presets.json` beside settings: `{name, pick}` list; dropdown + "Save as…".
- **App list:** `pm list packages -3 -f` plus sizes from `du` of each APK path (one
  `shell_each` call). Reading labels from each APK's `resources.arsc` on the PC is heavy,
  so the label falls back to the package name; when the helper is running it supplies real
  labels and icons through a new `list` kind `apps`.
- **Conversations:** read `sms.txt`/`sms.json` of the last backup (or a quick `content
  query` with address only) to list threads; selection filters what is written and what
  01 restores.
- **Patterns:** a few ready ticks map to globs (`DCIM/.thumbnails/**`,
  `Pictures/Screenshots/**`, `**/WhatsApp/Media/.Statuses/**`, `Android/media/**/Cache/**`).
- **Time left:** moving average over the last 20 s of bytes copied; hide when under 10 s
  or before 30 s of data.

## Steps

1. `Pick` fields, serde defaults, migration test.
2. Exclude patterns: matching function with tests; ready ticks in the picker.
3. Time left and speed in progress messages (adb and MTP paths).
4. Single apps: list UI, filter in the apps step; restore page app list honours it.
5. Single conversations: thread list UI; backup filter; restore filter (after 01).
6. Presets: save, load, rename, delete.

## Tests

- Unit tests for pattern matching (Windows separators, case, `**`), presets round trip,
  time-left maths with a fake clock.
- Fixture screens via `PB_FIXTURE` for the app and conversation pickers.

## Risks and open questions

- Picking single conversations means the backup no longer holds "all messages"; the read
  page and exports must say "partial backup: 3 conversations".
- Large app lists on slow phones: load lazily.

## Not in this group

Scheduled runs (out of scope), size limits per type (enough with "skip files over").

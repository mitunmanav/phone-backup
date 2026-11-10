# 11 Read, export and import

**Branch:** `plan/11-read-export-import` · **Depends on:** 03 for search across versions · **Size:** medium

## Why

Phone Backup already reads messages, contacts, calls and calendar in the app and exports
CSV, HTML, text, vCard and `.ics`, which is more than MobileTrans does. The gaps are
smaller but visible: Dr.Fone, AnyDroid, Syncios and Google Photos let you browse photos in
the app; nobody searches across backups well; and no tool imports other apps' exports so
a user who used SMS Backup & Restore or Google Takeout can bring that history in. PLAN.md
lists "a photo gallery", "search across backups" and "importing public export formats" as
ideas.

## Features in this group

| Feature (catalog rows F, K) | Today | After this group |
|---|---|---|
| One HTML page per conversation | one messages page | Index page plus one page per conversation (android-design.md section 5) |
| Export messages to PDF | print the HTML page | "Save as PDF" through the system browser's print, with a print stylesheet; no PDF library |
| Photo gallery view of a backup | open folder in Explorer | Grid by month, thumbnails cached, open full size, show in folder |
| Search across backups | search within one backup's messages | One search box over all backups (and versions from 03): messages, contacts, call numbers, file names |
| Import SMS Backup & Restore XML | — | Convert `sms-*.xml` and `calls-*.xml` into a backup's `data/` so read, export and 01's restore work |
| Import Google Takeout | — | Contacts (`.vcf`), Calendar (`.ics`), Google Photos (folders + JSON sidecar dates) into a backup |
| Import any vCard or `.ics` | — | Same path as Takeout |

## What exists now

- `export.rs`: `export()`, `book()`, threads (`Thread`, `Said`), CSV, HTML (`page`,
  `table`), `messages_txt`, `ics`, `sort_photos()`.
- `read_page.rs`: tabs for messages, contacts, calls, calendar with search in messages.
- `tools.rs` `heic_to_jpg()` reads EXIF.

## Design

- **Per-conversation HTML:** `exports/messages/index.html` + `<name>-<number>.html`,
  pictures copied beside them; same escaping rules as today (tests exist for nasty input).
- **Print stylesheet:** `@media print` rules in `page()`; "Save as PDF" opens the page in
  the default browser with a hint.
- **Gallery:** new tab on the read page; scans `files/DCIM`, `Pictures`, WhatsApp media;
  thumbnails made on a worker thread with the `image` crate (already used for HEIC→JPG)
  into a cache under the temp folder; dates from EXIF, falling back to file time.
- **Search index:** built lazily per backup into memory (messages, contacts, numbers, file
  names); for history versions (03), index `run.json` and older `data/` files. No index
  files written into backups.
- **Importers** (`src/import.rs`): each produces the helper JSON shape (`sms.json`, etc.)
  plus the `.txt` form, written into a chosen or new backup folder, marked
  `"imported_from"` in `run.json`/report.

## Steps

1. Per-conversation HTML export and print stylesheet; tests extend existing export tests.
2. Gallery tab with thumbnail cache and month grouping.
3. Search across one backup (contacts, calls, files), then across all backups.
4. Search across versions once 03 lands.
5. SMS Backup & Restore XML importer (SMS, MMS with base64 parts, calls); tests on a
   synthetic file.
6. vCard and `.ics` importer; Google Takeout folder detection (Contacts, Calendar,
   Google Photos with `*.json` sidecars for dates).
7. Docs.

## Tests

- Unit tests for importers with synthetic files; HTML escaping; gallery date fallback.

## Risks and open questions

- SMS Backup & Restore files can be hundreds of MB; stream-parse XML (`quick-xml`).
- Takeout Photos JSON names are truncated at 46 characters; match by prefix, test it.

## Not in this group

iPhone backups (13), WhatsApp chats (07).

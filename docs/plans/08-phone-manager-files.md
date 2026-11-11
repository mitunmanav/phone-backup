# 08 Phone manager: files

**Branch:** `plan/08-phone-manager-files` · **Depends on:** nothing · **Size:** medium

## Why

Dr.Fone Phone Manager, MobiKin, AnyDroid, Syncios, MyPhoneExplorer, AirDroid, Droid
Transfer and OpenMTP/MacDroid all let the user browse the phone and copy files both ways.
Windows Explorer can do this over MTP, but it is slow, hangs on big folders, cannot copy
from the phone with progress you trust, and shows nothing over Wi-Fi. PLAN.md lists
*Phone manager* after 1.0. Phone Backup already has the hard parts: adb and MTP
listing, long-path handling, name-clash rules, Wi-Fi connection and a picker with
thumbnails.

## Features in this group

| Feature (catalog row L) | Today | After this group |
|---|---|---|
| Browse the phone's files from the PC | picker during backup only | A **Phone** page with a folder tree, sizes, dates, thumbnails |
| Copy files from phone to PC | through a backup | Select, "Copy to PC…", with progress and Stop |
| Copy files from PC to phone | restore only | Drag files or folders onto the page, or "Add files…" |
| Rename, move, delete on the phone | delete via Free up space | Yes, with confirm; deletes are listed first |
| Create folders | — | Yes |
| Media scan after changes | after restore | After every write, so the gallery shows new photos |
| SD card | in backups | Browsable like internal storage |
| Works over Wi-Fi | backups only | Yes (adb) |

## What exists now

- `adb.rs` `measure()` lists files with sizes; `shell_each()` batches shell commands.
- `mtp.rs` lists and copies over MTP (Windows Portable Devices).
- `restore.rs` pushes files and asks the media store to rescan.
- `picker.rs` shows a file list with thumbnails; `tools.rs` `delete_from_phone()` deletes
  with safety checks (`safe_to_delete`).

## Design

- New page `src/ui/phone_page.rs` (one line in `ui.rs`, one branch in `shell.rs`), shown
  when a phone is connected.
- **Listing:** adb `ls -la` per opened folder (lazy), or `find -maxdepth 1 -printf` where
  toybox supports it; MTP folder enumeration otherwise. Cache per session; Refresh button.
- **Thumbnails:** pull small images on demand into a temp cache (max 200 at a time); videos
  show an icon.
- **Copy to PC:** reuse the batch `adb pull` with long-path and name-clash code from
  backups; destination picker remembers the last folder.
- **Copy to phone:** `adb push` in batches; refuse writes into `Android/data`, `Android/obb`;
  same-name file prompts Keep both / Replace / Skip.
- **Rename, move, mkdir, delete:** `mv`, `mkdir -p`, `rm` with paths quoted by one shared
  function (test it against spaces, quotes, `$`, unicode). Delete shows the full list and
  needs a second click; nothing outside shared storage is ever touched.
- **MTP mode:** browse and copy to PC only at first; writes need WPD transfer calls and
  come in a later step.

## Steps

1. Page skeleton and folder listing over adb with tests on captured `ls` output from
   toybox and busybox.
2. Copy to PC with progress, Stop, long paths.
3. Copy to phone (button and drag-and-drop from Explorer via egui's dropped files).
4. Rename, move, new folder, delete with the shared quoting function and tests.
5. Media scan after writes; SD card roots.
6. MTP browse and copy to PC; then MTP writes.

## Tests

- Unit tests for parsing listings, quoting, refusing protected paths.
- `PB_FIXTURE` screen for the page; manual over USB and Wi-Fi on two phones.

## Risks and open questions

- Some phones list `/sdcard` slowly with thousands of files in one folder; page the list.
- Drag and drop out of the app into Explorer needs Windows OLE work; start with buttons.

## Not in this group

App management (09), contacts and messages editing (10), mounting the phone as a drive
(not planned).

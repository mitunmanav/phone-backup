# 03 Backup history

**Branch:** `plan/03-backup-history` · **Depends on:** nothing · **Unlocks:** 04, 11 (search across history) · **Size:** medium

## Why

MobileTrans, Dr.Fone, Coolmuster, Apeaksoft, Smart Switch, Swift Backup, Neo Backup and
Titanium Backup all keep each backup run separately, and Syncthing keeps old versions of
changed files. Phone Backup keeps one folder per phone and updates it in place: files
deleted from the phone stay in the backup (good), but a file that changed is overwritten,
and `data/sms.json` and friends are replaced on every run. A user who lost messages last
week and backed up since has lost them in the backup too. That breaks the promise a backup
makes.

## Features in this group

| Feature (catalog row) | Today | After this group |
|---|---|---|
| Several dated versions (F) | one folder, updated | Every run is a version you can open or restore from |
| File history (F) | — | Older copies of changed files are kept |
| Keep only the last N versions (F) | — | Setting: keep all, or the last N, or versions newer than M months |
| Backup history and timeline (F) | — | List of runs with date, what was added, changed, removed |
| Restore from an older version (G) | — | Pick a run on the Restore page |
| Personal data history (A) | replaced each run | Each run's contacts, calls, messages, calendar kept |

## What exists now

- `src/backup/adb.rs` `adb_backup()` and `src/backup/mtp.rs` copy only files that are new
  or changed (by size) into `<backup>/<model>_<id>/files/…`.
- `checksums.sha256` holds SHA-256 per file; `report.txt` describes the last run.
- `src/backup/merge.rs` renames, deletes and merges backups.
- `src/backup/restore.rs` `saved_backups()` lists backups by folder.

## Design

Keep the main folder exactly as it is: **the latest state, as plain files**. That keeps
every existing promise ("your backup is an ordinary folder") and every existing reader.
History goes beside it, reverse-incremental, like Time Machine's idea but with plain files:

```
<model>_<id>/
  files/ … apps/ … data/ …          latest, unchanged layout
  checksums.sha256                    latest
  history/
    2026-11-10_1432/
      run.json                        what this run added, changed, removed (paths, sizes, sums)
      files/DCIM/…/IMG_0001.jpg       the version this run replaced (only changed files)
      data/sms.json                   the personal data this run replaced
```

- Before a changed file is overwritten, the old copy is **moved** (same disk, so instant)
  into `history/<this run>/…`. Files only removed from the phone are not moved; they stay
  in the latest folder as today, and `run.json` notes "gone from phone".
- `data/*.json`, `*.txt`, `contacts.vcf` are moved into history before each new run writes
  them, but only when their content changed (compare checksums).
- **Viewing an older version:** the version as of run R = latest files, with every file
  that a later run replaced swapped for its copy in that later run's history folder, minus
  files first added after R. Computed from the `run.json` files; no copying.
- **Pruning:** "keep last N" deletes the oldest `history/<run>` folders (Recycle Bin, as
  today's delete). Nothing in the latest folder is ever pruned.
- Checksums of history files are kept in each `run.json`, so Verify can check history too.
- Old backups without `history/` simply have one version: the latest.

## Steps

1. **`run.json` writer.** Record added, changed, removed for each run in both adb and MTP
   paths. Unit tests on a temp folder.
2. **Move replaced files into history.** Before overwrite in adb and MTP copy loops; long
   path and name-clash rules apply (reuse existing helpers). Tests: change a file twice,
   both old versions present.
3. **Personal data history.** Move changed `data/` files before writing.
4. **Version reader.** `versions(folder)` and `files_as_of(folder, run)`; tests with three
   runs.
5. **Restore from a version.** Restore page gains a "Version" dropdown (default: latest).
   The read page (`read_page.rs`) can open an older version's personal data.
6. **Timeline.** A "History" view per backup: runs with counts; open a run's changed files
   in Explorer.
7. **Keep last N.** Setting on the Settings page; prune after a successful run only.
8. **Verify history.** Extend `checksums::verify` to `run.json` sums; report per version.
9. **Merge and delete.** `merge.rs` keeps both histories; delete warns how many versions go.

## Tests

- Unit tests for every step above; e2e test that backs up a fixture phone three times with
  changes in between and restores version 1 exactly.
- Disk: history must not double space for unchanged files (assert sizes).

## Risks and open questions

- Size detection only: a file whose content changes but size does not is not seen as
  changed today. Option: compare modified time as well (adb `stat`), in this group.
- Users who copy the folder by hand to another disk keep history too; fine.
- History on FAT32/exFAT external disks: moves are still same-volume; fine.

## Not in this group

Search across versions (11), encrypting history (04), cloud copies (not planned).

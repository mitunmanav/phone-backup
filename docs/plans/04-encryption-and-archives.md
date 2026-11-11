# 04 Encryption and archives

**Branch:** `plan/04-encryption-and-archives` · **Depends on:** 03 (layout must be settled first) · **Size:** medium

## Why

Apeaksoft, Smart Switch, HiSuite, Swift Backup, Neo Backup, Seedvault, SMS Backup &
Restore and WhatsApp all offer password-protected backups. Phone Backup tells users
"Backups are not encrypted, so keep them on a drive only you use", which is honest but
leaves a stolen laptop or a shared family PC exposed: messages and contacts are plain
text. PLAN.md lists this under *Comfort* with AES-256-GCM and Argon2id.

Compression is the second half: Neo Backup (zstd), Titanium and SMS Backup & Restore
(zip) offer it. Photos and videos are already compressed, so the value is in a **single
archive file** for moving a backup to another disk or person, not in shrinking it.

## Features in this group

| Feature (catalog row) | Today | After this group |
|---|---|---|
| Password encryption (C) | — | Optional per backup; `data/` by default, media optional |
| Recovery key instead of password (C) | — | Optional printed recovery key (Seedvault-style) |
| Compression (C) | — | "Save as one archive" (`.zip`, optionally encrypted) |
| Verify encrypted backups (C) | — | Checksums stay on the encrypted bytes; Verify needs no password |
| Read and restore encrypted backups (G, K) | — | Ask for the password once per session |

## What exists now

- Design in [android-design.md](../android-design.md) section 4.1: one password per backup,
  Argon2id key, AES-256-GCM per file with random nonce, `encryption.json` beside
  `checksums.sha256`, `data/` only by default, password never stored.
- Readers: `export.rs` (`book()`, exports), `read_page.rs`, restore (01).

## Design

- **Format:** each protected file `X` becomes `X.pbenc`: 4-byte magic `PBE1`, 12-byte
  nonce, ciphertext with 16-byte tag. Stream in 1 MiB chunks (STREAM construction with
  a per-chunk counter) so videos are not loaded whole.
- **`encryption.json`:** `{"v":1,"kdf":"argon2id","m":65536,"t":3,"p":1,"salt":…,
  "check":<encrypted known value>,"scope":"data"|"all","recovery":<key wrapped by
  recovery key>|null}`. A data key is random; the password-derived key wraps it, so changing
  the password rewraps one small value instead of re-encrypting everything.
- **Scope:** `data` encrypts `data/` and `history/*/data/`; `all` also encrypts `files/`,
  `sdcard_*/` and `apps/`. Explorer then shows `.pbenc` names, so the UI warns that
  photos will no longer open by double-click.
- **Turn on later:** encrypting an existing backup converts in place, file by file, with
  resume (progress file); turning off decrypts the same way.
- **Archives:** "Save as one file…" writes `<name>.zip` (store-only for media, deflate for
  text) with an optional ZIP AES-256 password (WinZip AE-2), so Windows users with 7-Zip
  can open it without our app. "Open archive" reads it back as a backup.
- **Crates:** `argon2`, `aes-gcm` (RustCrypto, pure Rust, audited), `zip` with the `aes`
  feature. All licences GPL-compatible.

## Steps

1. **Crypto module.** `src/backup/crypt.rs`: key derivation, wrap and unwrap, chunked
   encrypt and decrypt; known-answer tests; wrong password fails cleanly.
2. **`encryption.json` and password prompt.** Session cache of the unlocked key; UI
   prompt widget.
3. **Encrypt `data/` on backup.** Writers in `adb.rs` write `.pbenc` when on; checksums
   over ciphertext.
4. **Readers.** `export.rs`, `read_page.rs`, restore, merge, history (03) decrypt
   transparently.
5. **Scope "all".** Files, SD cards, apps; restore decrypts to a temp file before
   `adb push`.
6. **Convert existing backups** both ways, with resume and Stop.
7. **Recovery key.** Generate, show once, print-friendly page; unlock with it.
8. **Archives.** Save as zip (with or without password); open zip as backup.
9. **Docs.** FAQ: what is encrypted, what is not, lost password means lost data.

## Tests

- Round trip per reader; wrong password; truncated file; tampered chunk detected by GCM.
- Conversion stopped half way, then resumed, then verified.
- Archive opened in 7-Zip (manual check) and by the app.

## Risks and open questions

- Performance on large media with scope "all": measure; AES-NI makes it disk-bound.
- File names stay visible (`DCIM/Camera/IMG_…`); say so. Name encryption is out of scope.
- A lost password cannot be recovered; the UI must make the recovery key easy to keep.

## Not in this group

Cloud upload of archives (not planned), signing releases (14).

## As built

Built on branch `ccr-b9ef6697-ktzzs2` (this work session's branch), on top of group 03, rather
than `plan/04-encryption-and-archives`.

- `src/backup/crypt.rs`: Argon2id (64 MiB, 3 passes, 1 lane) turns the password into a key that
  wraps a random data key with AES-256-GCM. **Change from the plan:** each `X.pbenc` starts
  with `PBE1` and a random 32-byte file id instead of one 12-byte nonce. The file is sealed in
  1 MiB chunks with AES-256-GCM under a key made for that file alone (HKDF-SHA256 of the data
  key and the id); the nonce holds the chunk number and a last-chunk flag. A cut, reordered or
  changed file fails to open. Known-answer tests were checked against Python's `cryptography`.
  About 760 MB/s sealing and 620 MB/s opening on one core, far above what USB delivers.
- `src/backup/lock.rs`: `encryption.json` holds the settings, salt, wrapped key, scope
  (`data` or `all`), the key wrapped by the recovery key, and an `unfinished` mark. Unlocked
  keys live in memory for the session, by the backup's random id (so a rename keeps them), and
  are wiped when the app closes. Helpers read, hash, copy and write files plain or sealed.
- Writers: personal data, picture-message files and merged contacts are sealed when the scope
  covers `data/`; unchanged data is compared with the text inside the sealed copy. With scope
  `all`, adb pulls a folder at a time into a temporary folder and seals each whole file into
  place with the phone's file time; MTP seals each download. Sealed copies are compared by the
  size inside them, so unchanged files are still skipped. Checksums cover the sealed bytes, so
  Verify needs no password.
- Readers: export, Read page (previews in memory, attachments open as temporary plain copies),
  history versions, restore (each file opened to a temporary file before `adb push`; app
  installers to a temporary folder), Find duplicates, Free up space (hashes of the files
  themselves), HEIC to JPG (JPGs sealed too) and photo export. Each job asks for the password
  first when it needs one; files and apps need it only with scope `all`.
- **Change from the plan:** turning the password on or off, or changing its scope, needs no
  progress file. `src/backup/protect.rs` writes each file whole under its new name with its
  old time before deleting the old one, then renames the checksums and every `run.json`. Run
  again after Stop or a crash, it carries on. The Lock dialog offers to finish such a change.
- UI: **Lock** on each backup in Restore sets a password (typed twice, at least 8 characters,
  personal data or everything, an "I understand" tick), then shows the recovery key once with
  Copy and a printable page. With a password it can change it (one value rewrapped), protect
  more or less, lock now, make a new recovery key, or remove the password after a confirm.
  The password prompt offers the recovery key; after using it a new password is asked for.
- Copy into another backup refuses a target that would hold protected files plain, and seals
  files again with the target's key.
- **Change from the plan:** "Open archive" unpacks the zip into the backup folder as a new
  backup rather than reading the zip in place. Archives hold plain files (sealed ones opened)
  with a fresh `checksums.sha256` and `phone-backup-archive.json`; a backup with a password can
  only be saved with a zip password (WinZip AES-256). A zip made from a protected backup comes
  back protected with the zip's password and the same scope, with a new recovery key.
- Exports stay ordinary files, and the export result says so. File names, sizes, dates, the
  checksums and the version lists stay visible; the FAQ says so.
- Tests: round trips, wrong password, cut, changed and reordered files, conversion on, wider,
  after a simulated crash and off with Verify passing each time, merges between protected
  backups, archives with and without a password. The ignored `e2e_password_protected_backup`
  needs a phone or emulator and has not been run yet. 7-Zip opening a password zip is a manual
  check on Windows.
- Not done: restoring contacts, calls, messages and calendar to the phone comes from group 01
  (#90). When 01 merges, its readers must read through `read_file`, and its restore must ask
  for the password first.

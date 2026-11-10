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

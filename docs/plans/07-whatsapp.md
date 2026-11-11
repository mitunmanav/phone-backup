# 07 WhatsApp

**Branch:** `plan/07-whatsapp` · **Depends on:** nothing · **Size:** medium

## Why

WhatsApp is the headline feature of MobileTrans, Dr.Fone and iCareFone, and the reason
many people pay for them. Phone Backup already copies WhatsApp's media folders as files.
This group adds the parts that are safe and legal without root, following
[android-design.md](../android-design.md) section 7: back up WhatsApp's own local backup,
move it to a new Android phone with WhatsApp's own restore, and read chats on the PC when
the user has their key.

## Features in this group

| Feature (catalog row J) | Today | After this group |
|---|---|---|
| Back up WhatsApp chats to PC | media folders only | Whole `Android/media/com.whatsapp/WhatsApp/` incl. `Databases/` and `Backups/` |
| WhatsApp Business | as files if present | Same, for `com.whatsapp.w4b`, shown separately |
| Move WhatsApp Android → Android | — | Guided: copy the folder to the new phone before WhatsApp's first start, then WhatsApp's "Restore" |
| Read WhatsApp chats on the PC | — | With the user's 64-digit end-to-end backup key: decrypt `msgstore.db.crypt15` locally and show chats |
| Export WhatsApp chats to HTML | — | One page per chat with pictures, like our SMS export |
| Fresh local backup reminder | — | Before copying, tell the user to open WhatsApp → Chats → Chat backup → Back up, so the database is current |

## What exists now

- Files backup copies shared storage; `Android/data` and `Android/obb` are skipped, but
  `Android/media` is copied, so most of WhatsApp's folder is already in backups.
- Restore pushes files back and skips files that exist (`restore.rs`).
- The read page (`read_page.rs`) and `export.rs` render SMS threads; reuse its HTML.

## Design

- **Detection:** a WhatsApp card on the Back up page when `Android/media/com.whatsapp`
  or `.w4b` exists: last local backup date (newest `Databases/msgstore*.crypt*` mtime),
  size, whether end-to-end encryption is on (crypt15 vs crypt14).
- **Backup:** nothing new to copy beyond making sure `Databases/` and `Backups/` are
  never skipped by junk rules or 06's patterns; label them in the report.
- **Move:** a "WhatsApp" tick on Transfer and Restore. Steps shown to the user:
  1. On the old phone, make a fresh WhatsApp backup (we check the date changed).
  2. On the new phone, install WhatsApp but **do not open it** (we check with `pm list
     packages` and `dumpsys package com.whatsapp | firstInstallTime`; we cannot see if it
     was opened, so we ask).
  3. We push the folder to `/sdcard/Android/media/com.whatsapp/WhatsApp/`.
  4. Open WhatsApp, verify the number, tap **Restore**.
  The app never taps inside WhatsApp, never downgrades it, never touches its private data.
- **Viewer:** crypt15 format per the open-source `wa-crypt-tools` documentation: key file
  or 64-hex key → derive the backup key → AES-256-GCM decrypt → zlib → SQLite. Read with
  `rusqlite` (bundled SQLite) into the same `Thread`/`Said` shapes `export.rs` uses.
  Media references map to files under `WhatsApp/Media/`. Key is never stored.
- **crypt14 / Google Drive key:** without end-to-end backups the key lives in WhatsApp's
  private data; say "turn on end-to-end encrypted backup in WhatsApp to read chats here".

## Steps

1. WhatsApp card: detection, last backup date, encryption mode; tests on fixture folders.
2. Protect `Databases/` and `Backups/` from junk rules; report lines.
3. Guided move on Restore and Transfer, with the "do not open WhatsApp yet" check.
4. crypt15 decryption module with test vectors made from a test account (no real chats in
   the repo; generate a fixture database and encrypt it with a known key).
5. SQLite reader for `messages`, `chat`, `jid`, `message_media` (schema varies by
   version; support the current one, detect and explain older).
6. Read page: "WhatsApp" tab with chats, search, pictures.
7. HTML export per chat.
8. Docs: FAQ on what we do and do not do with WhatsApp.

## Tests

- Unit tests for detection, decryption (fixture), schema mapping.
- Manual: two Android phones, real move with WhatsApp's restore; Business app too.

## Risks and open questions

- WhatsApp changes its schema and backup format; keep the reader tolerant and fail with a
  clear message, never silently.
- Licence: `wa-crypt-tools` is GPL-3.0, compatible; we reimplement from its docs anyway.
- Terms: reading the user's own backup with the user's own key is the user's choice; no
  automation of WhatsApp's UI (android-design.md section 7.4).

## Not in this group

Android ↔ iPhone WhatsApp (out of scope), LINE/Viber/WeChat (no public local backup
format), Google Drive backup download (needs Google sign-in; not planned).

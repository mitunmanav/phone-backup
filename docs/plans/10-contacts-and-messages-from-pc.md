# 10 Contacts and messages from the PC

**Branch:** `plan/10-contacts-and-messages-from-pc` · **Depends on:** 01 (insert operations, SMS role) · **Size:** medium

## Why

MobiKin, Coolmuster, Syncios, AnyDroid, Dr.Fone and MyPhoneExplorer let users edit
contacts and send texts with a real keyboard; MyPhoneExplorer and Coolmuster sync contacts
with Outlook. Phone Backup reads contacts and messages from backups only. Once 01 can
write to the phone's providers, editing on the live phone is a short step, and merging
duplicate contacts (already a tool) can finally be applied to the phone instead of only
to a new `.vcf` file.

## Features in this group

| Feature (catalog row N) | Today | After this group |
|---|---|---|
| Add, edit, delete contacts from the PC | — | Contact editor on the Phone page (live phone) |
| Merge duplicate contacts | into a new `.vcf` | Also on the phone, with a preview of each merge |
| Import contacts from vCard or CSV | phone imports our vCard by hand | "Import…" writes straight to the phone; CSV from Google and Outlook column names |
| Export contacts for Outlook or Google | vCard, CSV | CSV in Google's and Outlook's column layouts |
| Send SMS from the PC | — | Compose box per conversation; uses `SmsManager` with `SEND_SMS` (no default-app switch needed) |
| Send one message to several people | — | Yes, sent one by one (not group MMS) |
| Delete messages or conversations | — | Only while default SMS app (01's role); with a backup first |
| Delete messages after backup | — | Optional, after Verify succeeds, same role rules |

## What exists now

- `tools.rs` `merge_contacts()` and `merge_vcards()` (union-find by name, number, email).
- `export.rs` people, threads, CSV and HTML writers.
- 01 adds `insert`, `keys`, `sms_role` to the helper.

## Design

- **Helper ops:** `update` and `delete` for `contacts` (by raw contact id), `send` for SMS
  (returns sent/failed per part via `PendingIntent` results), `delete` for `sms`/`mms`
  (needs role).
- **Editor:** fields that the JSON already carries (names, numbers with type, emails,
  addresses, notes, birthday, groups, photo). Save writes one `applyBatch`.
- **Merge on phone:** reuse `merge_vcards` grouping over the live list; show each group,
  user ticks which to merge; helper uses `AggregationExceptions` to join raw contacts
  (keeps account links) instead of deleting data.
- **Sending:** rate-limit (1 per second), split long texts with `divideMessage`, show
  delivery per message; never send without the user pressing Send.
- **Safety:** every destructive action makes a small safety backup of what it changes
  into the phone's backup folder first (same idea as Transfer's safety backup).

## Steps

1. Helper `update`/`delete` for contacts; Rust client; tests.
2. Contact list and editor UI (live), add and delete.
3. Import vCard and CSV (Google, Outlook headers) to phone; tests on sample files.
4. Merge duplicates on phone through aggregation; preview UI.
5. `SEND_SMS` permission and `send` op; compose UI with delivery status.
6. Delete messages and conversations under the SMS role; delete-after-backup option.
7. Outlook and Google CSV export layouts.

## Tests

- Parser tests for CSV variants; helper instrumented tests for update, delete, send
  (emulator SMS loopback).
- Manual: edit on PC, check in the phone's Contacts app; send to self.

## Risks and open questions

- Contacts synced from Google: editing changes the cloud copy too; say which account a
  contact belongs to.
- `SEND_SMS` is a sensitive permission; ask only when the user opens compose.
- Dual-SIM: pick the SIM (`SubscriptionManager`) in the compose box.

## Not in this group

Live Outlook or Google two-way sync (not planned), calls from the PC (live-link product,
not planned), editing calendar (later, same pattern if users ask).

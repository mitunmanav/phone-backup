# 01 Personal data restore

**Branch:** `plan/01-personal-data-restore` · **Depends on:** nothing · **Unlocks:** 02, 05, 10, 13 · **Size:** large

## Why

Every paid suite in [competitors.md](../competitors.md) (MobileTrans, Dr.Fone, Coolmuster,
MobiKin, AnyDroid, Syncios, Apeaksoft) and every phone maker's tool (Smart Switch, HiSuite,
Clone Phone) puts contacts, messages, call history and calendar back on a phone. Phone
Backup saves all four but can only hand the user `contacts.vcf` to import by hand. The app
even says so today: "Contacts and messages cannot be copied to a phone yet." This is the
largest gap in the core job and the base for groups 02, 10 and 13.

## Features in this group

| Feature (catalog row) | Today | After this group |
|---|---|---|
| Restore contacts to the phone (G) | vCard import by hand | One click, with groups, photos and all fields the helper saved |
| Restore SMS and MMS (G) | — | Yes, through a temporary default-SMS switch |
| Restore call history (G) | — | Yes |
| Restore calendar (G) | — | Yes, into a local calendar on the phone |
| Never duplicate on a second restore (G) | files only | Same rule for personal data |
| Resume a restore after a break (G) | — | Yes, per data type |
| Count of what arrived (G) | files only | "812 of 812 contacts, 10,233 of 10,240 messages" |
| Restore to a different phone or brand (G) | files and apps | Personal data too |

## What exists now

- `companion/…/BridgeService.kt` answers `hello`, `list` (contacts, sms, calllog, calendar)
  and `bye`. It only reads. Tests in `src/backup/helper.rs` already send a
  `set_default_sms` request, so a stub for the role exists.
- Backups hold `data/contacts.json`, `sms.json`, `call_log.json`, `calendar.json` (exact
  provider fields from the helper) beside the `content query` text files, plus `mms.txt`
  and `data/mms/` attachments. Older backups and phones that refuse the helper only have
  the `.txt` files.
- `src/backup/restore.rs` restores files and apps; `restore()` runs the steps and writes
  `restore-report.txt`. `src/ui/restore_tab.rs` shows "What to restore".
- Design: [android-design.md](../android-design.md) sections 3.3 (protocol: `insert`,
  `sms_role`, batches of about 200) and 3.4 (SMS restore steps).

## Design

### Protocol additions (companion)

- `{"op":"insert","kind":K,"items":[…]}` → `{"ok":true,"inserted":n,"skipped":m,"failed":[…]}`
  for K in `contacts`, `calllog`, `calendar`, `sms`, `mms`. Batches of 200 so one bad row
  cannot sink a whole list and progress moves.
- `{"op":"sms_role"}` → `{"default":bool,"previous":"com.google.android.apps.messaging"}`;
  `{"op":"sms_role","ask":true}` opens the system prompt on the phone.
- `{"op":"keys","kind":K}` → hashes of what is already on the phone (see duplicates), so
  the PC can show "will add 1,204, skip 9,036" before anything changes.
- New permissions, added only now: `WRITE_CONTACTS`, `WRITE_CALL_LOG`, `WRITE_CALENDAR`,
  plus the default-SMS components (deliver receiver, WAP-push receiver, respond-via-message
  service, compose activity), all minimal.

### Writing each kind

| Kind | How | Notes |
|---|---|---|
| Contacts | `ContactsContract` `applyBatch`, one raw contact per saved contact, into the account chosen on the phone (default: local "Phone" account) | Restores every `data` row the JSON holds (numbers, emails, addresses, notes, birthdays, groups by title, photo from `data/contacts/<id>.jpg` when saved). From a `.txt`-only backup, falls back to name, numbers, emails. |
| Call history | `CallLog.Calls` insert | Keeps date, duration, type, number, name. |
| Calendar | Create (or reuse) a local calendar "Restored by Phone Backup" with `ACCOUNT_TYPE_LOCAL`, then `Events` insert, then `Reminders` | Writing into a Google account calendar from an app is not allowed; the user can move events later. Keeps `rrule`, all-day, time zone. |
| SMS | Only while the helper is the default SMS app; `Telephony.Sms` insert keeping `date`, `date_sent`, `read`, `type`, `thread` by address | Incoming SMS during the switch are stored by the helper's receiver, so nothing is lost. |
| MMS | Same role; insert into `content://mms`, its `addr` rows and `part` rows; attachment bytes pushed to `Android/data/<pkg>/files/in/` first | Text-only parts go inline. |

### Duplicates and resume

- Key per item: contacts by normalised name + sorted numbers + emails; calls by number +
  date + duration; events by title + start + end; SMS by address + date + body hash; MMS by
  date + box + part hashes. The phone side computes the same key for what it already has.
- The PC writes `restore-progress.json` in the backup folder after each batch
  (`{"phone":serial,"kind":"sms","done":4200}`). A second run reads it and the key set, so
  "Restore again" carries on and never doubles.

### SMS role flow on the PC

1. Ask `sms_role`. If not default, show "On the phone, choose Phone Backup as the messages
   app for now. You will switch back at the end." and send `ask`.
2. Poll `sms_role` until default or the user presses Cancel.
3. Insert. Then open the phone's default-apps screen (`am start -a
   android.settings.MANAGE_DEFAULT_APPS_SETTINGS`) and say "Choose <previous app> again".
4. Do not uninstall the helper until the role is back with another app; uninstalling the
   default SMS app leaves some phones with none.

### UI

- Restore page: "Contacts", "Call history", "Calendar", "Messages" become real ticks,
  each with a count from the backup and, after **Check restore**, "will add / already on
  the phone".
- Remove the "cannot be copied to a phone yet" line.
- Report gains one line per kind with added, skipped, failed counts.

## Steps

1. **Protocol and duplicate keys, Rust side.** `helper.rs`: `insert`, `keys`, `sms_role`
   client calls; key functions for each kind with unit tests on fixture JSON from existing
   tests.
2. **Companion: contacts and call history insert + keys.** Kotlin, with instrumented tests
   on an emulator. Add the two write permissions.
3. **Restore contacts and calls end to end.** `restore.rs` step after files and apps;
   progress file; report lines. UI ticks. Test with `PB_FIXTURE` and an emulator.
4. **Calendar.** Local calendar creation, events, reminders, time zones; tests for `rrule`
   and all-day events.
5. **Default-SMS components and `sms_role`.** Manifest pieces, receivers that store
   incoming SMS; PC prompt flow.
6. **SMS insert.** Threads by address, read and type flags; restore twice test.
7. **MMS insert.** Push attachments, insert pmu, addr, parts; check pictures show in
   Google Messages and Samsung Messages.
8. **Fallback for `.txt`-only backups.** Parse `contacts.txt`, `sms.txt`, `call_log.txt`,
   `calendar.txt` into the same item shape (fewer fields), so older backups restore too.
9. **Docs.** FAQ entry on the SMS switch, guide section, README "Restore" row.

## Tests

- Rust unit tests: duplicate keys, `.txt` → item conversion, progress file resume,
  report wording.
- Companion instrumented tests: insert then list returns the same rows; insert twice adds
  nothing.
- Manual round trip on a Pixel or emulator (Android 8 and 14), a Samsung and a Xiaomi:
  back up, clear test data, restore, compare counts; restore again, counts unchanged.
- Restore an old backup made before the helper existed.

## Risks and open questions

- MIUI/HyperOS may block installs over USB or SMS writes; detect and explain.
- Samsung Messages and some carriers show restored MMS without pictures if `part` rows
  are off by one field; compare with a phone-made MMS.
- Large message sets (100k+): keep batches small, report every batch, keep the phone awake
  (existing wake lock).
- Contacts into Google account vs local: start with local and say where they went; an
  account picker can follow.

## Not in this group

Moving personal data inside phone-to-phone transfer (02), blocked numbers and contact
groups beyond titles (05), editing contacts or sending SMS from the PC (10).

# 02 Complete phone move

**Branch:** `plan/02-complete-phone-move` · **Depends on:** 01 · **Size:** medium

## Why

"I got a new phone" is the moment most people look for a tool like this. MobileTrans,
Dr.Fone, Smart Switch, Clone Phone, Mi Mover and Google's own setup copy contacts,
messages, calls and apps in one go. Phone Backup's **Transfer** page copies files and apps
only, and it already does things none of them do: it shows what will change, backs up the
first phone along the way, and takes a safety backup of the second phone first. This group
finishes the job once group 01 can write personal data.

## Features in this group

| Feature (catalog row) | Today | After this group |
|---|---|---|
| Copy contacts, messages, calls, calendar to the new phone (H) | — | Yes, through 01's restore |
| One checklist of everything that will move (H) | files and apps | Every data type with counts and sizes |
| List of what cannot move and why (H) | partial | App data, alarms, settings, Wi-Fi, accounts, with a short "do this by hand" line each |
| Restore to a different brand (G) | files, apps | Everything that moved |
| Sign-in reminder list (A: accounts list) | — | Names of accounts on the old phone, so the user knows what to sign into |
| Finish screen (H) | report | Checklist with ticks, plus "switch messages app back" reminder when SMS moved |

## What exists now

- `src/backup/restore.rs`: `transfer_check()` and `transfer_now()` back up phone A into its
  own folder, then restore to phone B, with a safety backup of B.
- `src/ui/transfer_tab.rs`: two-phone picker, "Check transfer", "What will happen".
- Group 01 adds personal-data restore steps that `transfer_now()` can call.

## Design

- `transfer_now()` gains the four personal-data kinds as extra steps after apps, reusing
  01's restore (same duplicate keys, same progress file, so a stopped transfer resumes).
- The SMS role switch happens on phone **B** only; the prompt names phone B so users with
  two similar phones are not confused.
- **Checklist model:** one `Move` row per kind: `{kind, count, bytes, status: will_move |
  already_there | cannot_move(reason) }`. Built by `transfer_check()`; rendered by
  `transfer_tab.rs`; reused in the report.
- **Cannot move:** a fixed table with reasons and a hint:
  - App data and logins → "Sign in again; WhatsApp: see the WhatsApp page (07)."
  - Alarms, home screen, settings, Wi-Fi passwords → "No public way to copy them."
    (Some settings move later in 05.)
  - eSIM, bank and authenticator apps → "Move these with the app's own transfer."
- **Accounts:** `adb shell dumpsys account` lists account types and names; show only types
  and a masked name ("Google: a…@gmail.com") on screen, never in the report file.

## Steps

1. **Checklist model.** `Move` struct, built from a backup folder and phone B's keys
   (from 01). Unit tests with fixture folders.
2. **Personal data in transfer.** Call 01's restore for each ticked kind; progress and
   resume; stop button.
3. **Transfer page.** Checklist UI with ticks per kind, counts, "already there", "cannot
   move" section; finish screen with the SMS reminder.
4. **Accounts reminder.** Parse `dumpsys account` (tests on captured output from
   Android 9 and 14), mask names.
5. **Docs.** Guide "Move to a new phone" rewritten around the checklist.

## Tests

- Fixture-based tests for the checklist (empty phone B, half-restored phone B, B already
  full).
- Manual: Samsung → Pixel, Pixel → Xiaomi, stop in the middle and resume.

## Risks and open questions

- Phone B often arrives with a few Google contacts already synced; the duplicate keys must
  treat a synced contact with the same name and number as "already there".
- Two phones from the same model show the same name; always show the serial suffix.

## Not in this group

Settings and Wi-Fi copying (05), WhatsApp move (07), iPhone as phone A (13).

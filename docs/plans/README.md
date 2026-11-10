# Work plan: feature groups

[competitors.md](../competitors.md) lists about 290 features found in other phone backup
apps. This page splits what is worth building into groups. Each group has its own plan file
and its own branch and pull request. The pull request starts as a draft holding only the
plan; the code for that group lands in the same pull request, step by step, and it is
merged into `main` only when the group is done and CI passes.

## How the groups were cut

- **One user-visible promise per group.** "Contacts and messages go back on the phone",
  "WhatsApp moves to the new phone". A group can be released on its own and rolled back
  on its own.
- **Shared foundations first.** Groups that add protocol operations to the phone helper
  (`companion/`) or change the backup folder layout come before the groups that use them,
  so those changes happen once.
- **Small steps inside a group.** Each plan ends with numbered steps. Each step is one
  commit that builds, passes `cargo fmt`, `cargo clippy -D warnings` and `cargo test`, and
  leaves the app usable.
- **Respect PLAN.md.** Nothing here overrides its "Out of scope" and "Never" lists. Where a
  group touches one of those lines it says so.

## Order and dependencies

| # | Group | Branch | Depends on | Size |
|---|---|---|---|---|
| 01 | [Personal data restore](01-personal-data-restore.md) | `plan/01-personal-data-restore` | none | Large |
| 02 | [Complete phone move](02-complete-phone-move.md) | `plan/02-complete-phone-move` | 01 | Medium |
| 03 | [Backup history](03-backup-history.md) | `plan/03-backup-history` | none | Medium |
| 04 | [Encryption and archives](04-encryption-and-archives.md) | `plan/04-encryption-and-archives` | 03 | Medium |
| 05 | [More data types](05-more-data-types.md) | `plan/05-more-data-types` | 01 for restore parts | Medium |
| 06 | [Finer choices and progress](06-finer-choices-and-progress.md) | `plan/06-finer-choices-and-progress` | none (restore parts after 01) | Medium |
| 07 | [WhatsApp](07-whatsapp.md) | `plan/07-whatsapp` | none | Medium |
| 08 | [Phone manager: files](08-phone-manager-files.md) | `plan/08-phone-manager-files` | none | Medium |
| 09 | [Phone manager: apps](09-phone-manager-apps.md) | `plan/09-phone-manager-apps` | none | Small |
| 10 | [Contacts and messages from the PC](10-contacts-and-messages-from-pc.md) | `plan/10-contacts-and-messages-from-pc` | 01 | Medium |
| 11 | [Read, export and import](11-read-export-import.md) | `plan/11-read-export-import` | 03 for search across history | Medium |
| 12 | [Tools](12-tools.md) | `plan/12-tools` | none | Small |
| 13 | [iPhone to Android](13-iphone-to-android.md) | `plan/13-iphone-to-android` | 01 | Large |
| 14 | [Trust and install](14-trust-and-install.md) | `plan/14-trust-and-install` | none | Medium |
| 15 | [Accessibility and languages](15-accessibility-and-languages.md) | `plan/15-accessibility-and-languages` | none | Medium |
| 16 | [Mac and Linux](16-mac-and-linux.md) | `plan/16-mac-and-linux` | 14 for signing | Large |

Suggested working order: **01 → 02 → 03 → 04 → 07 → 05 → 06 → 09 → 08 → 10 → 11 → 12 →
15 → 14 → 13 → 16.** 01 and 02 close the biggest gap against every paid suite. 03 must
settle the folder layout before 04 encrypts it. 07 is the most-asked-for feature after
those. 13 and 16 are the largest and least certain, so they come last.

Groups with no dependency on each other (for example 07, 08, 09, 12, 14, 15) can be worked
in parallel without touching the same files much.

## Not planned, and why

| From the catalog | Why not |
|---|---|
| Scheduled and automatic backups | PLAN.md: out of scope for a PC app; would need a resident phone app. |
| Cloud destinations (Drive, Dropbox, WebDAV, SFTP) | Breaks "no cloud"; users can point the backup folder at a synced or network drive. |
| App private data | Android blocks it without root (PLAN.md: out of scope). |
| WhatsApp between Android and iPhone | WhatsApp has its own official transfer (PLAN.md: out of scope). |
| Phone-to-phone without a PC | Every phone maker already ships this; ours is a PC app. |
| Live link: notifications, clipboard, calls, remote camera, find phone | A different product (Phone Link, KDE Connect, AirDroid). scrcpy already covers screen control. |
| Deleted-data recovery | Needs root to work and over-promises without it. |
| Screen-lock or FRP removal, fake GPS, password readers | Never: clashes with the trust promise. |
| Media downloader, storage-saver recompression | Legal risk; we keep originals. |
| Reminder pop-ups, accounts, telemetry, ads | Never (PLAN.md). |

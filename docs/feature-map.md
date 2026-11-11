# Feature map: phone backup apps

This is a list of every feature we could find in Android backup, transfer and phone-manager
apps. For each feature it shows which apps have it and where Phone Backup stands. Use it to
spot gaps and to check ideas against [PLAN.md](PLAN.md) before building them. The work
it leads to is split into groups in [plans/README.md](plans/README.md).

Researched November 2026 from vendor pages, user guides, store listings, release notes and
reviews. Vendors rename modules, move features between price tiers and inflate claims, and
many review sites are paid affiliates. A cell here means "someone credible says so", not
"we tested it". Check the vendor's current page before relying on any one cell.

## Contents

1. [How to read this](#how-to-read-this)
2. [The apps](#the-apps)
3. [Feature catalog](#feature-catalog)
   - [A. What can be backed up](#a-what-can-be-backed-up)
   - [B. Choosing what to back up](#b-choosing-what-to-back-up)
   - [C. How a backup is made](#c-how-a-backup-is-made)
   - [D. When backups run](#d-when-backups-run)
   - [E. Where backups go](#e-where-backups-go)
   - [F. Managing backups](#f-managing-backups)
   - [G. Restore](#g-restore)
   - [H. Phone to phone](#h-phone-to-phone)
   - [I. iPhone and cross-platform](#i-iphone-and-cross-platform)
   - [J. WhatsApp and other chat apps](#j-whatsapp-and-other-chat-apps)
   - [K. Reading and exporting a backup](#k-reading-and-exporting-a-backup)
   - [L. Phone manager: files and media](#l-phone-manager-files-and-media)
   - [M. Phone manager: apps](#m-phone-manager-apps)
   - [N. Phone manager: contacts, messages, calls, calendar](#n-phone-manager-contacts-messages-calls-calendar)
   - [O. Media and file tools](#o-media-and-file-tools)
   - [P. Cleaning up and freeing space](#p-cleaning-up-and-freeing-space)
   - [Q. Live phone link and remote control](#q-live-phone-link-and-remote-control)
   - [R. Recovery, repair and device tools](#r-recovery-repair-and-device-tools)
   - [S. Connecting the phone](#s-connecting-the-phone)
   - [T. Safety, privacy and trust](#t-safety-privacy-and-trust)
   - [U. Ease of use and app comfort](#u-ease-of-use-and-app-comfort)
   - [V. Platforms, price and licence](#v-platforms-price-and-licence)
4. [What this means for Phone Backup](#what-this-means-for-phone-backup)
5. [Sources](#sources)

## How to read this

Each catalog row gives the feature, what it means, **who has it** (codes from the app list
below), and **Phone Backup's status**:

| Status | Meaning |
|---|---|
| ✓ | Shipped |
| ~ | Partly, or a workaround exists |
| Planned | Listed in PLAN.md under "After 1.0" |
| Idea | Listed in PLAN.md under "Ideas after 1.0" |
| Out of scope | PLAN.md says no for now |
| Never | Clashes with the app's promises; do not build |
| — | Not done and not planned yet; free to consider |

"R" after an app code means the feature needs root on that app. "Paid" means only in the
paid tier.

## The apps

About 35 apps in nine groups. Codes are used in the catalog.

**Paid desktop suites (Windows and Mac)**

| Code | App | Maker | Notes |
|---|---|---|---|
| MT | MobileTrans | Wondershare | Phone transfer, backup to PC, WhatsApp and chat apps, file transfer, iCloud transfer. ~$50/yr. |
| DF | Dr.Fone | Wondershare | Toolkit of separately sold modules: Phone Backup, Phone Manager, Phone Transfer, WhatsApp Transfer, Data Recovery, Data Eraser, Screen Unlock, System Repair, Virtual Location. ~$100/yr bundle. |
| CM | Coolmuster Android Assistant / Android Backup Manager | Coolmuster | Manager plus one-click backup. ~$30. |
| MK | MobiKin Assistant for Android / Backup Manager | MobiKin | Manager plus one-click backup. |
| AD | AnyDroid / AnyTrans for Android | iMobie | Manager, transfers, iTunes and iCloud to Android, media downloader. |
| SY | Syncios Mobile Manager | Syncios | Manager with free tools (ringtone maker, converters, photo compressor). Free and Ultimate tiers. |
| AP | Apeaksoft Android Data Backup & Restore / Aiseesoft FoneLab for Android | Apeaksoft, Aiseesoft | Backup with optional password encryption; sister recovery tools. |
| EM | MobiMover | EaseUS | Mostly iPhone; Android to PC, Android to Android, Android to iPhone. |
| IC | iCareFone Transfer | Tenorshare | WhatsApp backup and transfer, both platforms. |
| DT | Droid Transfer | Wide Angle Software | Windows manager over USB or Wi-Fi with a phone companion. ~$30. |

**Recovery tools**

| Code | App | Notes |
|---|---|---|
| UD | UltData for Android (Tenorshare) | Deleted-data scan, WhatsApp recovery, Google Drive backup extraction. |
| DB | D-Back for Android (iMyFone) | Deleted-data scan and preview. |

**Phone makers' tools**

| Code | App | Notes |
|---|---|---|
| SS | Smart Switch (PC, Mac and phone) | Samsung. Backup to PC or SD card, phone-to-phone over cable or Wi-Fi, from iPhone too. |
| SC | Samsung Cloud | Samsung. Cloud backup and sync on Galaxy phones. |
| HS | HiSuite and Phone Clone | Huawei. PC backup with password, phone clone by QR code, system updates. |
| CP | Clone Phone | OnePlus, OPPO, realme. Phone-to-phone and local backup. |
| MM | Mi Mover | Xiaomi. Phone-to-phone. |
| VE | EasyShare | vivo. Phone-to-phone and file sharing. |

**Built into Android or Google**

| Code | App | Notes |
|---|---|---|
| G1 | Backup by Google One | Cloud backup of apps, settings, SMS, calls; restore during setup; cable or Wi-Fi copy from an old phone. |
| GP | Google Photos | Photo and video backup, free up space, Locked Folder. |
| SV | Seedvault | Open-source backup built into LineageOS, CalyxOS, GrapheneOS. Encrypted to USB or WebDAV. |

**Backup apps that run on the phone**

| Code | App | Notes |
|---|---|---|
| SB | Swift Backup | Apps, SMS, calls, Wi-Fi, wallpapers; app data with root or Shizuku; many clouds. |
| NB | Neo Backup (was OAndBackupX) | Open source, root. Apps and data, special backups, schedules, encryption, zstd. |
| TB | Titanium Backup | Classic root tool: batch backup, freeze, schedules, Dropbox sync. Unmaintained. |
| SBR | SMS Backup & Restore | SyncTech. SMS, MMS, call logs to XML; schedules; clouds; Wi-Fi transfer. |
| WA | WhatsApp's own backup | Google Drive or local backup, end-to-end encryption, QR-code chat transfer. |

**Live phone link and remote control**

| Code | App | Notes |
|---|---|---|
| AIR | AirDroid Personal | Wireless files, remote control, remote camera, SMS, notifications, find phone. Freemium. |
| PL | Phone Link | Microsoft. Messages, calls, photos, notifications, apps, clipboard on Windows. |
| KC | KDE Connect | Open source. Files, clipboard, notifications, SMS, remote input, run commands, ring phone. |
| MPE | MyPhoneExplorer | Free Windows manager. Outlook and Google sync, SMS, calls, file browser, screen mirror. |
| SCR | scrcpy | Open-source screen mirror and control. Phone Backup opens it. |

**Sync and plain file transfer**

| Code | App | Notes |
|---|---|---|
| FS | FolderSync | One-way or two-way folder sync to SMB, SFTP, WebDAV and clouds on a schedule. |
| ST | Syncthing | Open-source peer-to-peer folder sync with file versioning. |
| OM | OpenMTP, MacDroid | USB file transfer on Mac (MTP; MacDroid also over adb). |
| IZ | InClowdz | Wondershare. Cloud-to-cloud migration and sync. |

**This app**

| Code | App |
|---|---|
| **PB** | **Phone Backup**: free, GPL-3.0, Windows. |

## Feature catalog

### A. What can be backed up

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Photos and videos | Camera roll and other picture folders | MT, DF, CM, MK, AD, SY, AP, EM, DT, SS, HS, CP, MM, VE, GP, AIR, FS, ST | ✓ |
| Music and audio | Song files on the phone | MT, DF, CM, MK, AD, SY, AP, SS, HS | ✓ |
| Documents | PDFs, Office files and other files | DF, CM, MK, AD, SY, AP, SS, HS | ✓ |
| Downloads and any other folder | Everything in shared storage, not just known types | DF, AD, SY, FS, ST, OM | ✓ |
| SD card | Files on a removable card | DF, AD, SY, SS (backup to card), TB | ✓ |
| Very large files | Files over a few GB (videos) | most PC tools; SC refuses files over 1 GB | ✓ (with a "skip files over" option) |
| Contacts | Names, numbers, emails, photos, notes | all suites, SS, SC, HS, CP, MM, VE, G1 (via Google account) | ✓ |
| Contact groups and labels | Which contacts are in which group | DF, CM, MK, SY | — |
| Blocked numbers list | The phone's call and SMS block list | MT (contact blacklist) | — |
| Text messages (SMS) | All conversations | all suites, SS, SC, HS, CP, MM, G1, SB, SBR, NB (special backup) | ✓ |
| Picture and group messages (MMS) | Messages with pictures, group threads | SS, SC, G1, SBR, SY, SB | ✓ |
| RCS / chat features messages | Google Messages and Samsung RCS chats | SC (enhanced messages), G1 | ~ (what the phone exposes as SMS/MMS) |
| Call history | Calls in and out, missed, durations | all suites, SS, SC, HS, CP, MM, G1, SB, SBR, NB | ✓ |
| Voicemail | Visual voicemail recordings | MT (iPhone) | — |
| Calendar | Events from phone calendars | MT, DF, AD, SY, SS, SC, CP | ✓ |
| Reminders and to-dos | Samsung Reminders, to-do lists | MT, SC | — |
| Notes | Samsung Notes, phone notes app | MT, SS, SY | ~ (only if saved as files) |
| Voice recordings | Recorder app files | MT, SS, HS | ✓ (they are files) |
| Bookmarks and browser history | Saved web pages | MT, SY, TB (XML) | — |
| Alarms and clock | Alarm list and settings | MT, SS, CP | — |
| Wallpapers | Home and lock screen pictures | MT, SB, G1 | — |
| Ringtones | Custom ringtones | MT, SY | ~ (as files) |
| Home screen layout | Icons, folders, widgets | SS, SC, G1 (Pixel) | — |
| Phone settings | Display, sound, accessibility, keyboard | SS, SC, G1, HS | — |
| Wi-Fi networks and passwords | Saved networks | SS, G1, SB (Shizuku or R), NB (R), TB (R) | — |
| Bluetooth pairings | Paired headphones, cars, watches | SC, G1 | — |
| Installed apps (APK files) | The app packages, including split APKs | MT, DF, CM, MK, AD, SY, SS, HS, CP, MM, G1, SB, NB (R), TB (R) | ✓ |
| App private data | Logins, game saves, settings inside apps | SB (R or Shizuku partial), NB (R), TB (R), G1 (apps that opt in), SS (Samsung apps), CP (some apps), HS | Out of scope (Android blocks it without root) |
| App data for apps that allow adb backup | Older `adb backup` for apps with allowBackup | some suites quietly use it | — |
| App permissions and settings | Grants, battery and notification settings per app | G1 (part), SB | — |
| Accounts list | Which accounts were signed in (for reminders, not passwords) | SS (on transfer) | — |
| eBooks | Books in reading apps or book folders | SY | ✓ (as files) |
| Samsung Health, Samsung Pass and other maker data | Maker app data | SS (Samsung only) | — |
| Emails | Locally stored mail | SS (only with encryption on) | — |
| WhatsApp media | Photos, videos, voice notes in WhatsApp folders | most file tools | ✓ (as files) |
| WhatsApp chats | Chat database with history | MT, DF, IC, UD, WA | Idea (read with user's own key) |
| WhatsApp Business | Same for the Business app | MT, DF, IC | — |
| LINE, Viber, Kik, WeChat | Other chat apps | MT (mostly iPhone), DF | — |
| Signal, Telegram | Chat apps with their own backups | none (they rely on in-app backup) | — |
| Phone and system info | Model, Android version, storage, security patch | DF, SY, AIR | ✓ (phone report) |

### B. Choosing what to back up

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Back up everything in one click | No choices needed | MT, DF, CM, MK, SY, AP, SS, HS, G1 | ✓ |
| Choose by data type | Tick photos, contacts, messages… | all suites, SS, CP, G1 (part), SB, NB | ✓ |
| Choose single folders | Pick which folders to include | DF, AD, SY, FS, ST | ✓ |
| Choose single files | Tick files in a list | DF, AD, SY, MK | ✓ |
| Choose single conversations | Only some SMS threads | SBR, DF | — |
| Choose single apps | Pick apps to include | DF, SB, NB, TB, SS | ~ (apps all or none) |
| App labels and saved selections | Named groups of apps or folders | SB (labels), NB (blocklist, filters) | — |
| Remember choices between runs | Same selection next time | SB, NB, SS, FS | ✓ |
| Show size before starting | "About 12 GB will be backed up" | SS, MT, PB | ✓ |
| Skip files over a size | Leave out huge videos | SC (hard 1 GB limit) | ✓ |
| Skip duplicates and empty files | Untick repeats before copying | none found | ✓ |
| Include or exclude by file type | Only .jpg, no .tmp | FS, IZ, ST (ignore patterns) | — |
| Exclude system and cache folders | Leave out Android/data, thumbnails | ST, FS (by filter) | ~ (Android blocks Android/data anyway) |

### C. How a backup is made

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Copy only what changed | Later backups add new or changed files only | G1, GP, FS, ST, SB | ✓ |
| Pause and carry on | Stop now, continue later without starting over | none of the suites | ✓ |
| Stop button that really stops | Kills the copy, keeps what is done | most | ✓ |
| Progress with counts and percent | X of Y files, MB copied | all | ✓ |
| Time left estimate | Minutes remaining | MT, DF, SS | — |
| Checks free disk space first | Stops before the PC fills up | SS | ✓ |
| Handles long Windows paths | Deep folders over 260 characters | rarely mentioned | ✓ |
| Handles names that clash on Windows | `IMG.jpg` and `img.jpg` both kept | rarely mentioned | ✓ |
| Original quality kept | No recompression of photos and video | all PC tools; GP only in Original mode | ✓ |
| Keeps file dates | Modified dates preserved | most | ✓ |
| Checksums saved with the backup | SHA-256 per file | none of the suites | ✓ |
| Verify a backup later | Re-check every file against its checksum | NB (verification batch), TB | ✓ |
| Compression | Smaller backups (zip, zstd) | NB (zstd), SBR (zip, Pro), TB, SB | — |
| Password encryption | Backup unreadable without a password | AP, SS, HS (media not encrypted), SB, NB (AES-256 or PGP), SBR (Pro), SV (AES-GCM, 12-word key), WA (E2E), TB | Planned (AES-256-GCM, Argon2id) |
| Recovery key instead of password | Long generated key you write down | SV, WA | — |
| De-duplication inside the backup | Same file stored once | ST (block level) | — |
| Report of what was copied | List of copied, skipped, failed files | PB, SS (summary) | ✓ |
| Read-only on the phone | Backing up never changes the phone | CM claims it; PB | ✓ |
| Safety backup before changes | Backs up a phone before a transfer writes to it | none found | ✓ |
| Backup notes | A note attached to a backup | SB (Paid) | — |
| Taskbar flash and sound when done | Tells you when a long job ends | PB | ✓ |

### D. When backups run

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Manual backup | You press a button | all | ✓ |
| Scheduled backups | Daily, weekly, at a time | G1, SB (Paid), NB, TB (Paid), SBR, FS, WA, AD | Out of scope |
| Unlimited schedules | Several schedules with different contents | NB, TB | Out of scope |
| Back up on plug-in | Starts when the phone is connected | DT (continuous backup), Apple-style tools | — |
| Only on Wi-Fi or while charging | Conditions for automatic runs | G1, GP, FS, SB, WA | — |
| Continuous / instant upload | Each new photo uploads as taken | GP, AIR (Paid), FS (instant sync), ST | — |
| Reminder to back up | Nudge after N days | Smart Switch-style prompts | Never (no reminder pop-ups) |

### E. Where backups go

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Folder on the PC | Local disk | all PC tools | ✓ |
| Choose any folder or drive | External disk, NAS share | CM, MK, DF, MT, AP, PB | ✓ |
| One folder per phone | Several phones kept apart | SS, PB | ✓ |
| Backup is ordinary files | Open photos in Explorer without the app | PB, FS, ST, SBR (XML) | ✓ |
| Backup in a private format | Only the vendor app can open it | MT, DF, CM, MK, AP, SS, HS | (we avoid this) |
| SD card or USB stick from the phone | No PC needed | SS, SV (USB), SB, NB | — |
| Google Drive | Cloud | G1, SB, SBR, WA, FS, TB | — |
| Dropbox, OneDrive, Box, Mega, pCloud, Yandex | Other clouds | SB, SBR, FS, TB, IZ | — |
| WebDAV, Nextcloud | Self-hosted cloud | SV, SB, SBR (Pro), FS | — |
| SMB / network share | Windows share or NAS from the phone | SB, FS | ~ (pick a mapped drive on the PC) |
| SFTP, FTP, S3 | Servers | SB, FS | — |
| Email | Send the backup file by mail | SBR | — |
| Vendor cloud | Samsung Cloud, Huawei Cloud, Mi Cloud | SC, HS | — |
| Peer-to-peer to another device | No server in the middle | ST, KC | — |

### F. Managing backups

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| List of backups | See all backups with dates and sizes | MT, DF, CM, MK, AP, SS, HS, SB | ✓ |
| Several dated versions | Keep last week's backup as well as today's | MT, DF, CM, AP, SS, SB, NB, TB, ST (versioning) | ~ (one folder per phone, updated in place; old files are kept) |
| Keep only the last N versions | Auto-delete old backups | NB, SB, SBR, TB | — |
| File history | Older copies of a changed file | ST, FS (some targets) | — |
| Rename a backup | Give it a friendly name | PB | ✓ |
| Delete a backup | Remove from disk safely | most | ✓ (to Recycle Bin) |
| Merge two backups | Copy one backup's files into another | none found | ✓ |
| Search across backups | Find a message or file in any backup | none found | Idea |
| Backup history and timeline | When each backup ran, what changed | SB (Paid) | Idea |
| Open a backup folder | Jump to it in Explorer | PB, SS | ✓ |
| Drop a folder on the window to open it | Drag and drop | — | Planned |
| Import other formats | Google Takeout, SMS Backup & Restore XML, iPhone backup | AD (iTunes), MT (iTunes, iCloud), SS (iPhone) | Idea |

### G. Restore

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Restore files and photos | Copy back to the phone | all suites, SS, HS, CP | ✓ |
| Restore apps | Reinstall APKs | MT, DF, CM, MK, AD, SY, SS, HS, G1, SB, NB, TB | ✓ |
| Restore app data | Logins and saves back into apps | SB (R), NB (R), TB (R), G1, SS (Samsung apps) | Out of scope |
| Restore contacts to the phone | Write contacts back | all suites, SS, HS, G1 | Planned (vCard import by hand works today) |
| Restore SMS and MMS | Write messages back (needs default-SMS role) | all suites, SS, SBR, SB, G1 | Planned |
| Restore call history | Write calls back | all suites, SS, SBR, SB, G1 | Planned |
| Restore calendar | Write events back | MT, DF, SS | Planned |
| Restore settings, Wi-Fi, wallpaper | Put phone settings back | SS, G1, SB, NB | — |
| Restore everything in one click | No choices | MT, CM, MK, AP, SS | ✓ |
| Choose by type | Only photos, only apps | all | ✓ |
| Choose single items with previews | Thumbnails and lists | MT, DF, MK, AD, AP, PB | ✓ |
| Preview what will change | Added, skipped, cannot move, and why | PB | ✓ |
| Never overwrite the phone's files | Existing files left alone | MT, PB | ✓ |
| Restore to a different phone | Any Android phone, not only the one backed up | MT, DF, CM, MK, SS | ✓ |
| Restore to a different brand | Samsung backup to a Pixel | MT, DF, CM, MK | ✓ |
| Restore from older Android to newer only | Version rules | G1 (newer only) | ✓ (no version rule) |
| Resume a restore after a break | Continue where it stopped | none found | Planned (for personal data) |
| Count of what arrived | "1,203 messages restored" | PB (files), SS | ~ |
| Restore during phone setup | From the setup screen | G1, SC, SS | — |
| Restore WhatsApp | Put chats back | MT, DF, IC, WA | Planned (Android to Android, guided) |
| Restore from another app's backup | iTunes, iCloud, Google contacts, Smart Switch | MT, AD, SY | Idea |
| Verify before restore | Check files are intact first | PB (Verify on Restore page) | ✓ |

### H. Phone to phone

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Copy one Android phone to another | Two phones on one PC | MT, DF, AD, SY, EM, PB | ✓ |
| Phone to phone without a PC | Wi-Fi Direct or hotspot | SS, CP, MM, VE, HS (Phone Clone), G1, SBR (messages) | — |
| Phone to phone by cable | USB-C to USB-C | SS, G1 | — |
| QR code to pair the phones | Scan to start | HS, CP, MM, VE, WA | ✓ (for Wi-Fi debugging pairing) |
| See what will change first | Preview before copying | PB | ✓ |
| Back up the first phone along the way | Transfer also leaves a backup | PB | ✓ |
| Safety backup of the second phone | Before it is changed | PB | ✓ |
| Copy contacts, messages, calls to the new phone | Personal data | MT, DF, SS, CP, MM, VE, G1 | Planned |
| Copy apps | Reinstall on the new phone | MT, SS, CP, MM, G1 | ✓ |
| Copy app data | Logins and saves | SS (Samsung), CP (some), G1 (some) | Out of scope |
| Copy settings and home screen | Feels like the old phone | SS, G1 (Pixel) | — |
| Keep using the old phone while it copies | Background transfer | SS, G1 | — |

### I. iPhone and cross-platform

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| iPhone to Android | Contacts, photos, messages, calendar | MT, DF, AD, SY, EM, SS, CP, HS, G1 | Idea |
| Android to iPhone | The other way | MT, DF, EM, AD | — |
| Restore an iTunes / Finder backup to Android | Read Apple's PC backup | MT, AD, SY | Idea |
| Restore from iCloud | Sign in and pull data | MT, AD, SY, SS | — |
| iPhone backup and management | Manage iPhones too | MT, DF, AD (AnyTrans), SY, EM, IC | Idea |
| WhatsApp between Android and iPhone | Chat history across platforms | MT, DF, IC, WA (official) | Out of scope |

### J. WhatsApp and other chat apps

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Back up WhatsApp chats to PC | Database and media | MT, DF, IC | ~ (media folders only) |
| Read WhatsApp chats on the PC | View conversations | MT, DF, IC, UD | Idea (with the user's own key) |
| Export WhatsApp chats to HTML or PDF | Printable copy | DF, MT, IC | — |
| Restore WhatsApp to a phone | Put chats back | MT, DF, IC, WA | Planned (guided, Android only) |
| Move WhatsApp Android to Android | Same platform | WA (QR transfer, Drive, local), MT, DF | Planned |
| Move WhatsApp across platforms | Android and iPhone | WA, MT, DF, IC | Out of scope |
| WhatsApp Business | Same features | MT, DF, IC | — |
| Extract a WhatsApp Google Drive backup | Download and read it | UD, DF | — |
| LINE, Viber, Kik, WeChat backup | Other chat apps | MT (mostly iPhone), DF | — |

### K. Reading and exporting a backup

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Read messages in the app | Conversation view without a phone | DF, CM, MK, AD, SY, SBR (on phone), PB | ✓ |
| Search messages | Find text across conversations | SBR, PB | ✓ |
| Read contacts, calls, calendar | Browse personal data | DF, CM, MK, AD, SY, PB | ✓ |
| See pictures in messages | MMS images inline | SBR, PB | ✓ |
| Export messages to CSV / spreadsheet | | CM, MK, SY, PB | ✓ |
| Export messages to HTML or printable page | | CM, MK, SY, DT, PB | ✓ |
| Export messages to text | | CM, PB | ✓ |
| Export messages to PDF | | DT, DF (WhatsApp) | ~ (print the page to PDF) |
| Export contacts to vCard | | CM, MK, SY, DF, AD, PB | ✓ |
| Export contacts to CSV, Excel | | CM, MK, SY, PB | ✓ |
| Export contacts to Outlook / sync with Outlook | | CM, MPE, MK (some) | — |
| Export calendar to .ics | | PB | ✓ |
| Export call history to CSV or HTML | | SY, CM, PB | ✓ |
| Sort photos into month folders | | PB | ✓ |
| Photo gallery view of a backup | Browse pictures by date | DF, AD, SY, GP | Idea |
| Open the XML of a backup | Readable by other tools | SBR | — |

### L. Phone manager: files and media

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Browse the phone's files from the PC | Folder tree of internal storage and SD | DF, MK, AD, SY, AIR, MPE, OM, DT | Planned |
| Copy files from PC to phone | Drag in | DF, MK, AD, SY, AIR, MPE, OM, KC | Planned |
| Copy files from phone to PC | Drag out | same, PB (via backup) | ~ |
| Rename, move, delete files on the phone | File operations | DF, AD, SY, AIR, MPE | — |
| Create folders on the phone | | DF, AD, SY | — |
| Photo manager | Albums, delete, add photos | DF, MK, AD, SY, CM | — |
| Music manager | Playlists, add songs, convert | DF, MK, AD, SY | — |
| Video manager | Add and remove videos | DF, MK, AD, SY | — |
| Sync with iTunes library | Move iTunes music to Android | DF, AD, DT | — |
| Transfer files without a cable | Wi-Fi file transfer | AIR, KC, MPE, AD, DT, VE | ~ (Wi-Fi debugging) |
| Transfer files between PC and phone on the internet | Remote, not local | AIR (data caps on free) | — |
| Mount the phone like a drive | Phone in Explorer or Finder | OM (MacDroid) | — |

### M. Phone manager: apps

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| List installed apps | Names, sizes, versions | DF, MK, AD, SY, AIR, MPE, SB, NB | ~ (read during backup) |
| Install APKs from the PC | Single or batch | DF, MK, AD, SY, AIR, MPE | ~ (restore installs apps) |
| Uninstall apps from the PC | Single or batch | DF, MK, AD, SY, AIR, MPE | Planned |
| Save (export) an app's APK | Copy an app to the PC | DF, MK, SY, SB | ✓ (part of backup) |
| Turn off (disable) or freeze apps | Without uninstalling | TB (R), NB, SB (Shizuku) | Planned |
| Move apps to SD card | | TB (R) | — |
| Convert user apps to system apps | | TB (R) | Never |
| Download apps from the internet | App downloader | AD | — |
| Kill or force-stop apps | | NB, SB | — |

### N. Phone manager: contacts, messages, calls, calendar

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Add, edit, delete contacts from the PC | Full editor | DF, CM, MK, AD, SY, MPE | — |
| Merge duplicate contacts | Combine same name, number or email | MK, DF, SY, AD, PB | ✓ (into a new file) |
| Import contacts from vCard, CSV, Outlook | | CM, MK, SY, MPE | ~ (phone imports our vCard) |
| Sync contacts with Outlook or Google | Ongoing two-way sync | MPE, CM | — |
| Send SMS from the PC | Type on keyboard | MK, AD, AIR, PL, KC, MPE, DF | — |
| Send group SMS | One message, many people | DF | — |
| Reply to messages from PC notifications | | PL, KC, AIR | — |
| Delete messages from the PC | | MK, SY, MPE, SBR (on phone) | — |
| Delete messages after backup | Free space on the phone | SBR | — |
| Make and answer calls from the PC | | PL, MPE, AIR | — |
| Edit calendar from the PC | | MPE, SY | — |
| Sync calendar with Outlook or Google | | MPE | — |
| Manage bookmarks and notes | | SY | — |

### O. Media and file tools

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| HEIC to JPG | Convert iPhone-style photos | SY, DF, AD, EM, PB | ✓ (keeps EXIF, turns upright) |
| Find duplicate files | Same content, any name | SY, AD, DF, PB | ✓ |
| Photo compressor | Smaller copies | SY | — |
| Ringtone maker | Cut songs into ringtones | SY, AD, DF | — |
| Audio converter | Convert music formats | SY | — |
| Video converter | Convert video formats | SY | — |
| Media downloader | Videos from websites | AD | Never (legal risk) |
| GIF maker | | DF (older) | — |
| Live photo or motion photo handling | Keep motion photos intact | GP | — |
| Storage saver quality | Compress photos and videos to save space | GP | Never (we keep originals) |

### P. Cleaning up and freeing space

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Free up space after backup | Delete from the phone what is safely backed up | GP, PB | ✓ (only files whose checksum matches) |
| Delete duplicates from a backup | | PB | ✓ |
| Junk and cache cleaner | | AD, SY (older), CP | — |
| Large file finder | | AD | — |
| AI clean before restore | Drop unneeded items | MT | — |
| Locked or hidden folder | Passcode-protected photos | GP | — |

### Q. Live phone link and remote control

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| See and control the phone screen | Mirror with mouse and keyboard | SCR, AIR, DF (MirrorGo), MPE, PL (apps), AD | ✓ (opens scrcpy) |
| Phone notifications on the PC | | PL, KC, AIR | — |
| Shared clipboard | Copy on phone, paste on PC | PL, KC | — |
| Remote keyboard | Type on phone from PC | AIR, KC | ~ (via scrcpy) |
| Remote camera | See through the phone camera | AIR | — |
| Find phone, ring, wipe | Lost phone tools | AIR, KC (ring) | — |
| Run PC commands from the phone | | KC | — |
| Use phone apps on the PC | | PL (some phones), SCR | ✓ (via scrcpy) |
| Cast PC screen to phone | | AIR (Cast) | — |

### R. Recovery, repair and device tools

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Recover deleted files from the phone | Scan for deleted photos, messages | DF, UD, DB, AP, CM (Lab.Fone), MK (Doctor) | — (needs root to work well; over-promises) |
| Recover from a broken phone | Read a phone with a dead screen | DF | ~ (Wi-Fi and scrcpy help if adb was on) |
| Recover from Google Drive backups | | UD, DF | — |
| Secure data eraser | Wipe a phone before selling | DF, MT, SY, CM, MK | — |
| Screen lock and FRP removal | Bypass locks | DF, IC (4uKey) | Never |
| System repair / reflash firmware | Fix boot loops | DF, HS (system update) | — |
| System updates and rollback | Install firmware | HS | — |
| Fake GPS location | Spoof location | DF | Never |
| Password manager | Read saved passwords | DF (iPhone) | Never |
| Phone report | Model, Android, storage, security patch | DF, SY, PB | ✓ |
| Diagnostics file for support | Versions and connected phones | PB | ✓ |

### S. Connecting the phone

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| USB file transfer (MTP) without debugging | Plain cable mode | PB, SS, OM | ✓ |
| USB debugging (adb) | Deeper access | MT, DF, CM, MK, AD, SY, EM, PB | ✓ |
| Wi-Fi with debugging | Wireless adb | PB (QR pairing), MacDroid, DF | ✓ |
| Wi-Fi through a phone app | Companion over the local network | MT, AD, AIR, KC, MPE, DT, CM, MK | ~ (helper is USB/adb based) |
| Bluetooth | | MPE, PL | — |
| QR code pairing | Scan to connect | PB, AIR, HS, MT | ✓ |
| Gets adb for you | Download Google's tools | PB (from Google with terms) | ✓ |
| adb bundled | Ships adb inside the app | MT, DF, CM, MK, AD | ~ (fetched on request) |
| Brand-specific setup steps | Pictures or text per maker | PB, MT, DF, CM, MK | ✓ |
| Explains when a cable only charges | Helps with bad cables and hubs | PB | ✓ |
| Detects USB driver problems | Windows driver hints | DF, PB | ✓ |
| Helper app on the phone | Reads contacts and messages | MT, DF, CM, MK, AD, SY, EM, AIR, DT, PB | ✓ (removed afterwards) |
| Removes its helper after | Leaves nothing behind | PB | ✓ |
| Several phones at once | Pick which phone | MT, PB | ✓ |
| Same phone on USB and Wi-Fi | Shown once | PB | ✓ |
| Root mode | Extra data if rooted | DF (some), SB, NB, TB | Out of scope |
| Shizuku mode | adb-level powers on the phone | SB, NB | — |

### T. Safety, privacy and trust

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| No account needed | Use without signing up | PB, CM, MK, SS, NB, SBR, ST, KC, OM | ✓ |
| Works fully offline | Nothing goes online | PB, NB, SV, ST, OM | ✓ (except Get adb and update check) |
| No telemetry or ads | | PB, NB, SV, ST, KC | ✓ |
| Open source | Code anyone can read | PB, NB, SV, KC, ST, SCR, OM | ✓ |
| Data never leaves the PC | No vendor server | PB, NB, ST, KC | ✓ |
| Plain list of what the app never does | Trust statement | — | Planned |
| Confirm before anything is deleted | | PB, most | ✓ |
| Deletes go to Recycle Bin | Undo is possible | PB | ✓ |
| Signed releases | Code-signed exe | MT, DF, SS (all paid tools) | Planned |
| Installer | Start menu, uninstall entry | all paid tools | Planned |
| Published checksum of the download | | PB | ✓ |
| Diagnostics you read before sharing | | PB | ✓ |
| App lock / password to open the app | | TB (freeze lock), GP (Locked Folder) | — |

### U. Ease of use and app comfort

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| First-run intro | Guided start | MT, DF, PB | ✓ |
| Light and dark mode | Follows Windows | PB, SY, AIR | ✓ |
| Animation on or off | | PB | ✓ |
| Sound when a job ends | | PB | ✓ |
| Built-in help and FAQ | | all, PB (guide, FAQ) | ✓ |
| Update check | | all, PB | ✓ |
| Automatic updates | Installs updates itself | MT, DF, AIR | — |
| Other languages | Translations | MT, DF, CM, MK, SY (many) | Idea |
| Keyboard shortcuts | | — | Planned |
| Screen-reader labels | Accessibility | — | Planned |
| Large text support | | — | Planned |
| Live chat or email support | Human help | MT, DF, CM, MK | — |
| Video tutorials | | MT, DF, CM, MK | — |

### V. Platforms, price and licence

| Feature | What it means | Who has it | PB |
|---|---|---|---|
| Windows | | all PC tools, PB | ✓ |
| Mac | | MT, DF, CM, MK, AD, SY, AP, EM, SS, OM | Idea |
| Linux | | KC, ST, SCR | Idea |
| Runs on the phone itself | No PC | SS, CP, MM, VE, G1, SB, NB, SBR, FS, ST | — |
| Web version | | AIR, IZ | — |
| No install needed (portable exe) | | PB | ✓ |
| Free with no limits | | PB, SS, NB, SBR (basic), KC, ST, SCR | ✓ |
| Free trial with limits | Backup only, or 10 photos | MT, DF, CM, MK, AD, SY, AP, EM | (n/a) |
| Subscription | Monthly or yearly | MT, DF, AIR, EM | (n/a) |
| One-time licence | | CM, DT, SBR (Pro), TB (Pro) | (n/a) |

## What this means for Phone Backup

**Where we are already ahead.** None of the paid desktop suites we checked advertises
pause and resume, SHA-256 checks, a "what will change" preview, a safety backup before a
transfer, merging backups, or "free up space" that deletes only files whose checksum
matches. None of them keeps the backup as an open folder you can use without the app. Add
free, no account, open source and no telemetry, and that is the pitch against Wondershare
and the rest.

**Gaps in the core job: back up, restore, move.** Ranked by how often users meet them:

1. **Put contacts, messages, calls and calendar back on a phone.** Every competitor does it.
   It is the first thing people try after a backup. (PLAN.md: *Personal data restore*.)
2. **Several dated backups and file history.** Most suites keep each run separately; ours
   updates one folder. Users want "the backup from before I deleted it". Fits the
   *history of backups* idea.
3. **WhatsApp chats.** The headline feature of MobileTrans, Dr.Fone and iCareFone. The
   Android-only guided move in PLAN.md covers the safe part.
4. **Password-protected backups.** Apeaksoft, Smart Switch, HiSuite, Swift Backup, Neo
   Backup, Seedvault and SMS Backup & Restore all have it. (PLAN.md: *Comfort*.)
5. **Phone manager.** Browse and copy files both ways, install and uninstall apps.
   (PLAN.md: *Phone manager*.)
6. **Settings, Wi-Fi networks, wallpaper, alarms, bookmarks, blocked numbers.** Smart
   Switch, Google One and MobileTrans cover some of these. Most are readable over adb or
   by the helper without root; worth checking one by one.

**Smaller ideas worth weighing:** single SMS conversations in backup and restore;
compression; time-left estimate; keep the last N backups; include/exclude by file type;
export messages as text and PDF; edit contacts and send SMS from the PC; secure erase
before selling a phone; iPhone to Android; translations; Mac and Linux builds.

**Leave alone:** screen-lock and FRP removal, fake GPS, password readers and media
downloaders clash with the trust promise. Deleted-data "recovery" mostly needs root and
over-promises. App private data without root is blocked by Android. Reminder pop-ups are
ruled out in PLAN.md. Scheduled backups stay out of scope as PLAN.md says, though Google
One, Swift Backup, Neo Backup and SMS Backup & Restore show users like them; a phone-side
app would be the natural home if that changes.

## Sources

**Wondershare MobileTrans**: [phone backup and restore](https://mobiletrans.wondershare.com/phone-backup-restore.html),
[backup and restore](https://mobiletrans.wondershare.com/backup-restore.html),
[WhatsApp transfer](https://mobiletrans.wondershare.com/whatsapp-transfer-backup-and-restore.html),
[phone and PC transfer](https://mobiletrans.wondershare.com/solutions/transfer-files-between-phone-and-pc.html),
[App Store listing](https://apps.apple.com/us/app/data-transfer-mobiletrans/id1545867834),
[TechJury](https://techjury.net/product-analysis/mobiletrans/),
[CNX Software](https://www.cnx-software.com/2022/10/21/mobiletrans-secure-phone-to-phone-transfer-solution/)

**Wondershare Dr.Fone**: [Android data manager](https://drfone.wondershare.com/android-transfer.html),
[Android backup and restore](https://drfone.wondershare.com/backup/android-backup-and-restore.html),
[Android recovery](https://drfone.wondershare.com/android-data-recovery.html),
[WhatsApp local backup](https://drfone.wondershare.com/whatsapp/whatsapp-local-backup.html),
[V13 upgrade](https://drfone.wondershare.com/upgrade-to-drfone-v13.html),
[Hongkiat V13 review](https://www.hongkiat.com/blog/wondershare-dr-fone-review/),
[Windows Report](https://windowsreport.com/wondershare-dr-fone-review/),
[Gizchina](https://www.gizchina.com/2022/06/17/wondershare-dr-fone-a-complete-software-for-android-and-ios-device-management/)

**Coolmuster**: [Android Assistant](https://www.coolmuster.com/android-assistant.html),
[guide](https://www.coolmuster.com/android/android-assistant-guide.html),
[Ryadel review](https://www.ryadel.com/en/coolmuster-android-assistant-review-backup-android/),
[MajorGeeks](https://www.majorgeeks.com/files/details/coolmuster_android_backup_manager.html)

**MobiKin**: [desktop manager](https://www.mobikin.com/android-backup/android-desktop-manager.html),
[AlternativeTo](https://alternativeto.net/software/mobikin-assistant-for-android),
[ShowMeTech how-to](https://en.showmetech.com.br/transfira-dados-entre-android-e-pc-mobikin/)

**iMobie AnyDroid / AnyTrans**: [iTunes backup to Android](https://imobie.com/guide/anydroid/itunes-backup-to-android.htm),
[Setapp](https://setapp.com/apps/anytrans-for-android),
[Android Police](https://www.androidpolice.com/2019/03/25/manage-all-your-android-data-from-your-computer-with-anytrans-for-android-sponsored-post/)

**Syncios**: [features](https://www.syncios.com/features.html),
[Ultimate](https://www.syncios.com/mobile-manager-ultimate/),
[version compare](https://www.syncios.com/mobile-manager-ultimate/version-compare.html),
[Mac manager](https://www.syncios.com/mac-ios-manager/)

**Apeaksoft / Aiseesoft**: [Android Data Backup & Restore](https://apeaksoft.com/android-data-backup-and-restore),
[guide](https://apeaksoft.com/android-data-backup-and-restore/guide),
[FoneLab on SourceForge](https://sourceforge.net/app/fonelab-for-android/)

**EaseUS MobiMover**: [product page](https://mobi.easeus.com/phone-transfer/mobimover.html),
[Sweet Bits](https://thesweetbits.com/tools/easeus-mobimover/)

**Tenorshare and iMyFone**: [iCareFone Transfer](https://maketecheasier.com/icarefone-transfer/),
[UltData for Android](https://www.androidcentral.com/tenorshare-ultdata-android-recover-data-without-root),
[recovery tools roundup](https://www.softwaretestinghelp.com/android-data-recovery-software)

**Droid Transfer**: [Capterra](https://www.capterra.com/p/276094/Droid-Transfer/),
[AndroidGuys](https://androidguys.com/reviews/app-reviews/droid-transfer-review/)

**Samsung**: [Smart Switch PC backup](https://www.samsung.com/au/support/mobile-devices/backup-with-smart-switch-pc/),
[Smart Switch FAQ](https://www.samsung.com/au/apps/smart-switch/faq-smart-switch-pc-or-mac/),
[back up and restore on Galaxy](https://www.samsung.com/us/support/answer/ANS10002780/),
[Smart Switch transfer items](https://www.samsung.com/latin_en/support/mobile-devices/how-to-transfer-and-backup-data-using-smart-switch-on-a-galaxy-phone/),
[Samsung Cloud](https://www.samsung.com/uk/support/apps-services/use-the-samsung-cloud-to-back-up-and-restore-data-to-your-galaxy-device/)

**Huawei**: [HiSuite backup](https://consumer.huawei.com/uk/support/content/en-gb00698618),
[HiSuite restore](https://consumer.huawei.com/en/support/content/en-us15801375/)

**OnePlus, Xiaomi, vivo**: [Clone Phone on Google Play](https://play.google.com/store/apps/details?id=com.oneplus.backuprestore&hl=en),
[clone methods overview](https://www.tecnoloblog.com/en/How-to-clone-your-Android-phone/)

**Google**: [Google One backup help](https://support.google.com/googleone/answer/9149304?hl=en&co=GENIE.Platform%3DAndroid),
[Android Authority restore guide](https://www.androidauthority.com/restore-android-phone-from-backup-3229485/),
[Android Central](https://www.androidcentral.com/how-back-your-android-phone-and-data-google-one),
[Google Photos Locked Folder backup](https://blog.google/products-and-platforms/products/photos/locked-folder-cloud-backup/)

**Seedvault**: [CalyxOS docs](https://calyxos.org/docs/guide/apps/seedvault/),
[GitHub](https://github.com/seedvault-app/seedvault)

**Swift Backup**: [official site](https://www.swiftapps.org/),
[Google Play](https://play.google.com/store/apps/details?id=org.swiftapps.swiftbackup),
[Shizuku file backups](https://androidcommunity.com/swift-backup-brings-rootless-file-backups-for-android-using-shizuku-20211102/)

**Neo Backup**: [GitHub](https://github.com/NeoApplications/Neo-Backup),
[FAQ](https://github.com/NeoApplications/neo-backup/blob/main/FAQ.md),
[changelog](https://github.com/NeoApplications/Neo-Backup/blob/main/CHANGELOG.md)

**Titanium Backup**: [AddictiveTips guide](https://www.addictivetips.com/mobile/how-to-use-titanium-backup-for-android-complete-guide/),
[matrixrewriter](https://matrixrewriter.com/android/)

**SMS Backup & Restore**: [SyncTech](https://www.synctech.com.au/sms-backup-restore/),
[Amazon Appstore](https://www.amazon.com/Ritesh-Sahu-SMS-Backup-Restore/dp/B0075A5S0K)

**WhatsApp**: [TechRadar transfer guide](https://www.techradar.com/computing/software/how-to-transfer-whatsapp-chats-to-a-new-phone),
[Nextpit](https://nextpit.com/how-to-transfer-whatsapp-conversations-to-new-devices?c=2350966)

**AirDroid**: [AirDroid Personal](https://www.airdroid.com/personal/),
[help overview](https://help.airdroid.com/hc/en-us/articles/360005038914-What-s-AirDroid-Personal-How-to-start-using),
[TechRadar](https://www.techradar.com/pro/software-services/airdroid-personal-review)

**Microsoft Phone Link**: [Microsoft](https://microsoft.com/en-sg/windows/sync-across-your-devices)

**KDE Connect**: [KDE UserBase](https://userbase.kde.org/KDEConnect),
[DebugPoint guide](https://www.debugpoint.com/kde-connect-guide/?amp=1)

**MyPhoneExplorer**: [official site](https://www.fjsoft.at/de/),
[FossHub](https://www.fosshub.com/MyPhoneExplorer.html)

**FolderSync, OpenMTP, MacDroid, InClowdz**: [FolderSync](https://foldersync.io),
[FolderSync concepts](https://foldersync.io/docs/help/concept),
[OpenMTP on XDA](https://www.xda-developers.com/openmtp-open-source-alternative-android-file-transfer-macos/),
[MacDroid](https://www.macdroid.app/android-file-transfer),
[InClowdz sync](https://inclowdz.wondershare.com/cloud-sync.html)

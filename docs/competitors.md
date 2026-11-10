# Feature map: popular phone backup apps

What other Android backup apps offer, set against Phone Backup. Use it to spot gaps
and to check ideas against [PLAN.md](PLAN.md) before building them.

Researched November 2026, mostly from vendor pages, user guides and reviews. Vendors
change feature lists and price tiers often, and many review sites are affiliate
pages, so treat any single cell as "check before relying on it".

**Key:** ✓ has it · ~ partly, or only in some versions or tiers · ✗ does not have it ·
? not confirmed in sources · **R** needs root

## The apps

| App | Maker | Runs on | Model | One-line pitch |
|---|---|---|---|---|
| **Phone Backup** (this repo) | open source | Windows | Free, GPL-3.0 | Back up, restore and move an Android phone into an ordinary PC folder. |
| **MobileTrans** | Wondershare | Win, Mac, phone app | Paid, ~$40/yr | Phone-to-phone and cross-OS transfer, backup to PC, WhatsApp and chat-app moves. |
| **Dr.Fone** | Wondershare | Win, Mac, phone app | Paid per module or bundle, ~$100/yr | Toolkit: recovery, WhatsApp transfer, phone transfer, backup, eraser, unlock, repair, manager. |
| **Coolmuster Android Backup Manager** | Coolmuster | Win, Mac | Paid, ~$30; trial backs up only | One-click backup and restore by category. |
| **MobiKin Assistant for Android** | MobiKin | Win, Mac | Paid; free tier limited | Android manager: backup, restore, edit contacts, SMS from PC, apps. |
| **AnyDroid / AnyTrans for Android** | iMobie | Win, Mac | Paid; free tier limited | Android content manager, transfers, iTunes backup to Android. |
| **Syncios Mobile Manager** | Syncios | Win, Mac | Free manager, paid backup tools | Manage and back up Android and iPhone from a PC. |
| **Android Data Backup & Restore** | Apeaksoft | Win, Mac | Paid | Backup with an optional password-encrypted mode. |
| **MobiMover** | EaseUS | Win, Mac | Paid; free tier limited | Mainly iPhone; Android-to-PC, Android-to-Android and Android-to-iPhone moves. |
| **Smart Switch** | Samsung | Win, Mac, phone app | Free | Samsung's own backup and move, including Samsung apps and settings. |
| **Google One backup** | Google | Built into Android | Free (15 GB shared) | Cloud backup of apps, settings, messages, calls and photos. |
| **Swift Backup** | SwiftApps | Android app | Freemium | App, SMS and call-log backup to cloud or SMB; app data with root or Shizuku. |
| **Neo Backup** (was OAndBackupX) | open source | Android app | Free, AGPL-3.0 | Root app backup with data, schedules and encryption. |
| **SMS Backup & Restore** | SyncTech | Android app | Free, Pro $4.99 | SMS, MMS and call logs to XML, on a schedule, to cloud. |
| **AirDroid Personal** | Sand Studio | Win, Mac, web, phone app | Freemium, ~$30/yr | Wireless file transfer, remote control, photo auto-backup. |

## 1. What gets backed up

PC suites: Phone Backup, MobileTrans (MT), Dr.Fone (DF), Coolmuster (CM), MobiKin (MK),
AnyDroid (AD), Syncios (SY), Apeaksoft (AP), Smart Switch (SS).

| Data | Phone Backup | MT | DF | CM | MK | AD | SY | AP | SS |
|---|---|---|---|---|---|---|---|---|---|
| Photos and videos | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Music, documents, other files | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| SD card | ✓ | ? | ? | ? | ? | ? | ? | ? | ~ |
| Pick single folders | ✓ | ✗ | ? | ✗ | ~ | ~ | ~ | ✗ | ✗ |
| Contacts | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Text messages (SMS) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Picture and group messages (MMS) | ✓ | ? | ? | ? | ? | ? | ~ | ? | ✓ |
| Call history | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Calendar | ✓ | ✓ | ✓ | ? | ? | ✓ | ? | ✗ | ✓ |
| Installed apps (APK files) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✓ |
| App private data | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ~ Samsung apps |
| WhatsApp chats | ~ media files only | ✓ | ✓ | ✗ | ✗ | ? | ✗ | ✗ | ✗ |
| LINE, Viber, WeChat, Kik | ✗ | ~ mostly iPhone | ~ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Phone settings, Wi-Fi, home screen | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| Notes, voice memos, alarms | ~ as files | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ Samsung |

Phone and cloud apps: Google One (G1), Swift Backup (SB), Neo Backup (NB),
SMS Backup & Restore (SBR), AirDroid (AIR).

| Data | G1 | SB | NB | SBR | AIR |
|---|---|---|---|---|---|
| Photos and videos | ✓ via Google Photos | ✗ | ✗ | ✗ | ✓ |
| Other files | ✗ | ✗ | ✗ | ✗ | ~ manual transfer |
| Contacts | ✓ via Google account | ✗ | ✗ | ✗ | ✗ |
| SMS and MMS | ✓ | ✓ | ✗ | ✓ | ✗ |
| Call history | ✓ | ✓ | ✗ | ✓ | ✗ |
| Apps | ✓ | ✓ | ✓ R | ✗ | ✗ |
| App private data | ~ apps that opt in | ✓ R or Shizuku | ✓ R | ✗ | ✗ |
| Device settings, wallpaper | ✓ | ~ wallpaper | ✗ | ✗ | ✗ |

## 2. How backups work

| Feature | Phone Backup | MT | DF | CM | MK | AP | SS | G1 | SB | NB | SBR |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Pick data types | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✓ | ✓ | ✓ |
| Copy only what changed | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ? | ✓ | ? | ? | ✗ |
| Pause and carry on later | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | n/a | ✗ | ✗ | ✗ |
| Checksum check of the backup | ✓ SHA-256 | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ? | ? | ? | ✗ |
| Backup is a plain, open folder | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ~ | ~ | ✓ XML |
| Password encryption | ✗ planned | ✗ | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ AES-256 | ✓ | ✓ Pro |
| Scheduled automatic backups | ✗ out of scope | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ daily | ✓ paid | ✓ | ✓ |
| Choose backup folder or drive | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ~ | ✗ | ✓ | ✓ | ✓ |
| Cloud destination | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ Google | ✓ Drive, Mega, Box, SMB | ~ any synced folder | ✓ Drive, Dropbox, WebDAV |
| Keeps several dated backups | ~ one folder, updated | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ one | ✓ | ✓ | ✓ |

## 3. Restore and transfer

| Feature | Phone Backup | MT | DF | CM | MK | AD | SS | G1 |
|---|---|---|---|---|---|---|---|---|
| Restore files and photos | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Restore apps | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Restore contacts, SMS, calls, calendar to the phone | ✗ planned (vCard import works by hand) | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Pick single items to restore, with previews | ✓ | ✓ | ✓ | ~ by type | ✓ | ✓ | ~ by type | ✗ |
| Never overwrite what the phone has | ✓ | ✓ | ? | ? | ? | ? | ? | ✗ |
| Show what will change first | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Android to Android, direct | ✓ | ✓ | ✓ | ✗ | ✗ | ✓ | ✓ | ✓ setup |
| iPhone to Android | ✗ idea | ✓ | ✓ | ✗ | ✗ | ✓ | ✓ | ~ |
| Restore from iTunes or iCloud backup | ✗ idea | ✓ | ✓ | ✗ | ✗ | ✓ iTunes | ✓ | ✗ |
| WhatsApp between phones | ✗ planned (Android only) | ✓ incl. iPhone | ✓ incl. iPhone | ✗ | ✗ | ? | ~ | ✓ own backup |

## 4. Read, export and manage

| Feature | Phone Backup | MT | DF | CM | MK | AD | SY | AIR |
|---|---|---|---|---|---|---|---|---|
| Read messages, contacts, calls in the app | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ live phone |
| Export to CSV or spreadsheet | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ |
| Export to printable page or HTML | ✓ | ✗ | ✓ | ~ | ✓ | ✓ | ✓ | ✗ |
| Export contacts to vCard | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ |
| Export calendar to .ics | ✓ | ✗ | ? | ? | ? | ? | ? | ✗ |
| Browse phone files, copy both ways | ✗ planned | ✗ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| Install, uninstall, save apps from PC | ✗ planned | ✗ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| Edit contacts from PC | ✗ | ✗ | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |
| Send SMS from PC | ✗ | ✗ | ? | ✗ | ✓ | ✓ | ? | ✓ |
| See and control the phone screen | ✓ via scrcpy | ✗ | ✓ MirrorGo | ✗ | ✗ | ✗ | ✗ | ✓ |
| Notifications on PC | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |

## 5. Extra tools

| Tool | Phone Backup | Who else has it |
|---|---|---|
| Find duplicate files | ✓ | Syncios, AnyDroid (photo dedupe), Dr.Fone (manager) |
| HEIC to JPG | ✓ | Dr.Fone, AnyTrans, MobiMover |
| Merge duplicate contacts | ✓ | MobiKin, Dr.Fone, Syncios, AnyDroid |
| Sort photos by month | ✓ | few; mostly gallery apps |
| Free up phone space after backup | ✓ | Google Photos ("free up space"), Samsung Device Care |
| Deleted-data recovery (scan phone) | ✗ | Dr.Fone, Coolmuster Lab.Fone, MobiKin Doctor, Tenorshare UltData |
| Secure data eraser | ✗ | Dr.Fone, MobileTrans, Coolmuster, MobiKin Eraser |
| Screen-lock removal, FRP bypass | ✗ never | Dr.Fone, Tenorshare 4uKey |
| System repair / reflash | ✗ | Dr.Fone, Tenorshare ReiBoot |
| Fake GPS location | ✗ never | Dr.Fone |
| Ringtone maker, media downloader | ✗ | AnyDroid, Syncios |

## 6. Connection and setup

| Feature | Phone Backup | MT | DF | CM | MK | SS | AIR |
|---|---|---|---|---|---|---|---|
| USB file transfer (MTP), no debugging | ✓ | ✗ | ✗ | ✗ | ✗ | ✓ | ✗ |
| USB debugging (adb) | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ~ |
| Wi-Fi | ✓ QR pairing | ✓ phone app | ✓ | ✓ | ✓ | ✗ | ✓ |
| Fetches adb itself | ✓ | ✓ bundled | ✓ bundled | ✓ bundled | ✓ bundled | n/a | n/a |
| Brand-specific debugging steps | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a |
| Helper app installed on phone | ✓ removed after | ✓ | ✓ | ✓ | ✓ | n/a | ✓ stays |
| Works on any Android brand | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ Samsung target | ✓ |
| Needs account or sign-in | ✗ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ |
| Mac / Linux | ✗ idea | Mac | Mac | Mac | Mac | Mac | Mac, web |
| Price | free | paid | paid | paid | paid | free | freemium |

## What this means for Phone Backup

**Where we are already ahead.** No paid desktop suite does incremental backups,
pause and resume, SHA-256 verification, a "what will change" preview, or a backup
that is just an open folder. Together with free, no account and open source, that
is the pitch against Wondershare and the rest.

**Gaps every paid suite fills that we do not yet:**

1. **Putting personal data back** (contacts, messages, calls, calendar onto a phone).
   Every competitor has it, and it is the obvious question after "back up".
   Already in PLAN.md as *Personal data restore*. Highest value.
2. **Phone manager**: browse and copy files both ways, install and uninstall apps.
   Also in PLAN.md.
3. **WhatsApp move.** The headline feature of MobileTrans, Dr.Fone and iCareFone.
   PLAN.md covers Android to Android only, which is a fair line to hold.
4. **Password-protected backups.** Apeaksoft, Smart Switch, Swift Backup and
   SMS Backup & Restore offer it. Planned under *Comfort*.
5. **Several dated backups (versions)** instead of one updated folder. Most suites
   keep one backup per run. Worth adding to the *history of backups* idea.

**Smaller ideas worth weighing:** edit contacts and send SMS from the PC (MobiKin,
AirDroid); iPhone to Android (MobileTrans, Dr.Fone, Smart Switch); Mac and Linux builds.

**Leave alone.** Screen-lock and FRP removal and fake GPS clash with the app's trust
promise. Deleted-data recovery mostly needs root and over-promises. App private data
without root is blocked by Android. Scheduled backups stay out of scope as PLAN.md
says, though Google One, Swift Backup, Neo Backup and SMS Backup & Restore show users
like them; a phone app would be the natural place if that ever changes.

## Sources

- MobileTrans: [backup and restore](https://mobiletrans.wondershare.com/backup-restore.html), [WhatsApp transfer](https://mobiletrans.wondershare.com/de/whatsapp-transfer-backup-restore.html), [CNX review](https://www.cnx-software.com/2022/10/21/mobiletrans-secure-phone-to-phone-transfer-solution/), [SoftwareHow review](https://softwarehow.com/?p=34651)
- Dr.Fone: [WhatsApp local backup](https://drfone.wondershare.com/whatsapp/whatsapp-local-backup.html), [backup guide](https://drfone.wondershare.com/guide/social-app-data-backup.html), [Windows Report review](https://windowsreport.com/wondershare-dr-fone-review/), [Hongkiat review](https://hongkiat.com/blog/wondershare-dr-fone-review)
- Coolmuster: [Softpedia](https://www.softpedia.com/get/Mobile-Phone-Tools/Others/Coolmuster-Android-Backup-Manager.shtml), [MajorGeeks](https://www.majorgeeks.com/files/details/coolmuster_android_backup_manager.html), [G2](https://www.g2.com/products/coolmuster-android-backup-manager/discuss)
- MobiKin: [desktop manager](https://www.mobikin.com/android-backup/android-desktop-manager.html), [AlternativeTo](https://alternativeto.net/software/mobikin-assistant-for-android), [Gizchina review](https://www.gizchina.com/2019/11/20/mobikin-assistant-a-simple-way-to-manage-your-android-device-2/)
- AnyDroid: [iMobie guide](https://imobie.com/guide/anydroid/itunes-backup-to-android.htm), [Ashampoo listing](https://ashampoo.com/en-gb/anydroid), [FileHorse](https://www.filehorse.com/download-anydroid/)
- Syncios: [MajorGeeks](https://www.majorgeeks.com/files/details/syncios.html), [DEV article](https://dev.to/yukinoshita/manage-your-phone-like-a-pro-simplify-data-transfer-and-backup-with-syncios-mobile-manager-14a9)
- Apeaksoft: [product page](https://apeaksoft.com/android-data-backup-and-restore), [guide](https://apeaksoft.com/android-data-backup-and-restore/guide)
- EaseUS MobiMover: [product page](https://mobi.easeus.com/phone-transfer/mobimover.html), [Sweet Bits review](https://thesweetbits.com/tools/easeus-mobimover/)
- Samsung Smart Switch: [PC backup](https://www.samsung.com/au/support/mobile-devices/backup-with-smart-switch-pc/), [FAQ](https://www.samsung.com/au/apps/smart-switch/faq-smart-switch-pc-or-mac/), [backup and restore](https://www.samsung.com/au/support/apps-services/backing-up-and-restoring-data-using-smart-switch/)
- Google One: [Android Central guide](https://www.androidcentral.com/how-back-your-android-phone-and-data-google-one), [Google support](https://support.google.com/googleone/answer/9149304)
- Swift Backup: [APKMirror release notes](https://www.apkmirror.com/apk/swiftapps-org/swift-backup/swift-backup-3-7-0-release/swift-backup-3-7-0-android-apk-download/), [android-stats](https://android-stats.com/d/org.swiftapps.swiftbackup)
- Neo Backup: [GitHub](https://www.Github.com/NeoApplications/Neo-Backup), [F-Droid](https://f-droid.org/zh_Hans/packages/com.machiav3lli.backup/), [XDA](https://www.xda-developers.com/oandbackupx-android-backup-app)
- SMS Backup & Restore: [SyncTech](https://synctech.com.au/sms-backup-restore/)
- AirDroid: [product page](https://www.airdroid.com/personal/?lang=en), [TechRadar review](https://www.techradar.com/pro/software-services/airdroid-personal-review)
- Tenorshare iCareFone: [Make Tech Easier](https://maketecheasier.com/icarefone-transfer/)

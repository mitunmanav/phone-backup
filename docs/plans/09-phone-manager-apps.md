# 09 Phone manager: apps

**Branch:** `plan/09-phone-manager-apps` · **Depends on:** nothing · **Size:** small

## Why

Dr.Fone, MobiKin, AnyDroid, Syncios, AirDroid and MyPhoneExplorer list apps and install,
uninstall or export them from the PC; Titanium Backup, Neo Backup and Swift Backup also
freeze apps. PLAN.md's *Phone manager* includes "list, install, save, uninstall or turn
off apps". Phone Backup already lists packages, pulls APKs and installs split APKs, so
this is mostly a screen on top of existing code.

## Features in this group

| Feature (catalog row M) | Today | After this group |
|---|---|---|
| List installed apps | read during backup | An **Apps** view: name, package, version, size, installed date, user or system |
| Install APKs from the PC | through restore | "Install…" for `.apk`, split sets in a folder, and `.apks`/`.xapk`/`.apkm` bundles (zip of splits) |
| Uninstall apps | — | Yes, with "keep app data" option (`pm uninstall -k --user 0`) |
| Turn off (disable) and turn on apps | — | `pm disable-user --user 0` / `pm enable`; good for bloatware; with a warning list of apps never to touch |
| Save an app's APK to the PC | inside backups | "Save APK…" for one app |
| Open app settings on the phone | — | `am start -a android.settings.APPLICATION_DETAILS_SETTINGS` |
| Clear cache (where allowed) | — | Not possible without root on current Android; say so |

## What exists now

- `restore.rs` `restore_apps()` and `only_in()`; `install-multiple -r`.
- Backups pull every APK path from `pm path`.
- Group 06 adds an app list with labels from the helper; share that code.

## Design

- Apps tab on the Phone page from 08 (or its own page if 08 is not merged yet).
- Data: `pm list packages -f -U -i --show-versioncode` and `dumpsys package` for dates
  and versions, parsed once and cached.
- **Bundles:** unzip `.apks`/`.xapk`/`.apkm` to temp, collect `*.apk`, `install-multiple`;
  OBB files from `.xapk` pushed to `Android/obb/<pkg>/` (allowed for adb shell).
- **Safety list** of packages that must not be disabled (system UI, phone, settings,
  launcher in use, keyboard in use, SMS default); disable needs confirm and lists what
  stops working. Re-enable list shown at the top ("Turned off by you").

## Steps

1. Package list parser with tests on captured output (Android 8, 11, 14, Samsung).
2. Apps view with search and filters (user, system, turned off).
3. Uninstall (with keep-data option) and Save APK.
4. Disable and enable with the safety list.
5. Install from APK, split folder and bundle formats; tests on fixture zips.

## Tests

- Parser and bundle tests; manual install of a split app (e.g. one from a backup).

## Risks and open questions

- `pm uninstall --user 0` on system apps is reversible only with `cmd package
  install-existing`; offer that as "Put back" and test it.
- Play Protect may warn on installs from the PC; explain in the UI.

## Not in this group

App data backup (out of scope), app store or downloader (not planned).

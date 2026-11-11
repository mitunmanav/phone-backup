# 16 Mac and Linux

**Branch:** `plan/16-mac-and-linux` · **Depends on:** 14 for signing and release work · **Size:** large

## Why

MobileTrans, Dr.Fone, Coolmuster, MobiKin, AnyDroid, Syncios, Apeaksoft and Smart Switch
all run on Mac; macOS has no built-in way to reach an Android phone's files at all, which
is why OpenMTP and MacDroid exist. Linux users have KDE Connect and Syncthing but no
backup-and-restore app like this one. PLAN.md lists "Linux and Mac builds" as an idea.
The app is Rust and egui, so most of it is already portable; the work is in the Windows-
only edges.

## Features in this group

| Feature (catalog row V) | Today | After this group |
|---|---|---|
| Linux build | — | AppImage and `.tar.gz`, adb mode; MTP mode where `mtp-rs` works over libusb |
| Mac build | — | Universal (Intel + Apple silicon) `.app` in a `.dmg`; adb mode; MTP mode |
| Get adb on each system | Windows zip from Google | The right Platform Tools zip per OS |
| scrcpy on each system | Windows download | Find it on PATH or Homebrew; link to install steps |
| CI on each system | Windows | Build and test on Linux and macOS runners too |

## What exists now (Windows-only parts)

| Part | Where | Portable replacement |
|---|---|---|
| Keep the PC awake | `backup.rs` `keep_awake` (`SetThreadExecutionState`) | Linux: `systemd-inhibit` or D-Bus inhibit; Mac: `caffeinate -i` child process or IOKit assertion |
| Free disk space | `backup.rs` `free_space` (Windows API; a non-Windows stub exists) | `statvfs` (`libc`) |
| Recycle Bin deletes | `merge.rs`, Shell API | `trash` crate (freedesktop trash, macOS Finder) |
| Open folders | `adb.rs` `explorer()` | `xdg-open` / `open` |
| USB driver hints | `adb.rs` `pnputil` parsing | Linux: udev rules hint ("plugdev" group, `51-android.rules`); Mac: none needed |
| Taskbar flash, sounds | `windows-sys` UI and audio | egui `RequestUserAttention`; sounds through `rodio` or skip |
| MTP | `mtp-rs` crate | Check its Linux and macOS support; on macOS the kernel lets apps claim the device after Android File Transfer is quit |
| Long paths, name clashes | Windows rules | Keep name-clash code (APFS default is case-insensitive too); long paths not needed |
| Windows high contrast and language | 15 | Read from `NSUserDefaults` / `gsettings` or skip |

## Design

- One `src/platform/` module with `windows.rs`, `linux.rs`, `macos.rs` behind small
  functions (`keep_awake`, `free_space`, `trash`, `open_folder`, `attention`,
  `adb_download_url`, `driver_hint`). Call sites stop using `cfg(windows)` directly.
- Default backup folder: `Documents/Phone backups` on all systems (same as today).
- Release: Linux AppImage via `cargo-appimage`; Mac `.app` via `cargo-bundle`, universal
  with `lipo`, `.dmg` via `create-dmg`.
- **Mac signing and notarisation need a paid Apple Developer account.** Without one,
  ship unsigned with the "right-click → Open" steps, or ship via Homebrew cask, which
  users trust and which can build from source. Decide with the maintainer.

## Steps

1. `platform` module; move Windows code behind it with no behaviour change (Windows CI
   must stay green).
2. Linux: build, free space, trash, open folder, keep awake; adb mode end to end on a
   real phone; udev hint.
3. Linux CI job (build, clippy, test) and AppImage in releases.
4. macOS: same set; adb download for Mac; build on `macos-latest`; `.app` and `.dmg`.
5. MTP on Linux and Mac with `mtp-rs`; fall back to "turn on USB debugging" where it fails.
6. scrcpy discovery per system.
7. Website download page per system.

## Tests

- Existing tests run on all three CI systems; platform functions get small unit tests.
- Manual: Ubuntu LTS, Fedora, macOS on Apple silicon, with a Pixel and a Samsung.

## Risks and open questions

- macOS USB access may need the user to quit other MTP apps (Android File Transfer,
  OpenMTP); detect and explain.
- Apple notarisation cost conflicts with "free options"; Homebrew cask may be the answer.
- Wayland and X11 differences in egui windows (drag and drop, attention requests).

## Not in this group

iPhone live access, Mac-specific iCloud features.

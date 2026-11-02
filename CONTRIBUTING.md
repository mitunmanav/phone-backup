# Contributing

Bug reports and focused improvements are welcome. Open an issue before starting a large change so its scope can be discussed.

## Development setup

Build and run checks on Windows with the stable Rust toolchain installed through [rustup](https://rustup.rs/).

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

## Code map

| To change | Look in |
|---|---|
| A screen or dialog | `src/ui/`, one file per screen; `src/ui/shell.rs` for the top bar and page frame |
| Colors, sizes, fonts | `src/ui/theme.rs` |
| Shared controls | `src/ui/widgets.rs` |
| Starting a job, what it reports | `src/jobs.rs` |
| Phone commands, backup over adb | `src/backup/adb.rs` |
| Backup over MTP | `src/backup/mtp.rs` |
| Restore and phone to phone | `src/backup/restore.rs` |
| Checksums, reports, tidying backups | `src/backup/checksums.rs`, `report.rs`, `merge.rs` |
| Exports, Tools page work | `src/export.rs`, `src/tools.rs` |
| Android helper app | `companion/`; build with `companion\gradlew.bat -p companion assembleRelease`, design in `docs/android-design.md`. The next `cargo build` embeds that APK; without it the app still builds, only the helper is missing |
| Starting, reading through and removing the helper | `src/backup/helper.rs`; the backup's data step in `src/backup/adb.rs` calls it |

To add a screen: add its file in `src/ui/`, a line in `src/ui.rs`, and a branch where `src/ui/shell.rs` picks the page.

## Debugging

- `PB_LOG=1` adds every adb command, with its exit code and time, to `phone-backup-adb.log` in the temp folder. It holds file names and the phone's serial number, so do not share it unredacted.
- `PB_FIXTURE="<screen and options>"` opens a debug build on test data with no phone. The options are listed in `src/main.rs`.

## Pull requests

- Keep each change focused and explain the user-visible behavior it changes.
- Include tests for changed backup, restore, parsing, or device-detection behavior.
- Describe any manual checks, including Windows version and phone connection mode when relevant.
- Call out changes that could affect existing backups or restore behavior.
- Do not change `version` in `Cargo.toml`. Changes collect on `main`; a maintainer bumps the version (patch for fixes, minor for features) when a release is ready, and merging that bump publishes it.
- Do not include real phone backups, contacts, messages, serial numbers, or unredacted logs in issues or pull requests.

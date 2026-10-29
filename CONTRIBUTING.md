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

## Pull requests

- Keep each change focused and explain the user-visible behavior it changes.
- Include tests for changed backup, restore, parsing, or device-detection behavior.
- Describe any manual checks, including Windows version and phone connection mode when relevant.
- Call out changes that could affect existing backups or restore behavior.
- If the change touches the app, bump `version` in `Cargo.toml` (patch for fixes, minor for features). Merging it to `main` publishes that version as a release.
- Do not include real phone backups, contacts, messages, serial numbers, or unredacted logs in issues or pull requests.

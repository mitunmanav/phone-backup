# 14 Trust and install

**Branch:** `plan/14-trust-and-install` · **Depends on:** nothing · **Unlocks:** 16 (signing on other systems) · **Size:** medium

## Why

Every paid suite ships a signed installer; Phone Backup ships an unsigned exe and the
README has to explain Windows' "More info → Run anyway" warning. For an app that asks
people to plug in their phone with all their messages on it, that first screen costs
trust. PLAN.md's *Trust* group lists "signed releases and an installer, where free
options allow", and *Comfort* lists "a plain list of what the app never does".

## Features in this group

| Feature (catalog row T, V) | Today | After this group |
|---|---|---|
| Signed releases | unsigned exe, SHA-256 published | Code-signed exe and installer through a free open-source signing programme |
| Installer | portable exe only | MSI installer (Start menu, uninstall entry), portable exe kept |
| winget | — | `winget install PhoneBackup` manifest submitted per release |
| Plain list of what the app never does | spread over README and FAQ | One page in the app and on the website |
| Published helper APK and checksum | embedded in exe | APK attached to each release with its SHA-256, as android-design.md promises |
| Supply-chain checks | CI fmt, clippy, test | `cargo deny` (licences, advisories), Dependabot already on |
| Reproducible build notes | — | Documented toolchain and steps so anyone can rebuild and compare hashes |

## What exists now

- `.github/workflows/release.yml` builds on `windows-latest`, tags `v<version>` and
  uploads the exe with a SHA-256 file.
- `.github/dependabot.yml` exists. `SECURITY.md` exists.
- `build.rs` embeds the icon and the helper APK.

## Design

- **Signing:** apply to SignPath Foundation (free code signing for open-source projects,
  used by many OSS Windows apps). Their GitHub Action signs artifacts from the release
  workflow; the certificate stays with them. If refused, fall back to documenting the
  checksum check and keep the warning text. Azure Trusted Signing costs money; out unless
  a sponsor appears.
- **Installer:** `cargo-wix` (WiX v4) building a per-user MSI with no admin prompt;
  installs to `%LOCALAPPDATA%\Programs\Phone Backup`; adds Start menu shortcut; uninstall
  leaves backups and settings alone and says so.
- **winget:** manifest generated in the release job; PR to `microsoft/winget-pkgs` done
  by hand the first time, then with `wingetcreate` in CI.
- **"Never" page:** short list in Settings and `docs/faq.html`: no account, no upload of
  your data, no ads, no telemetry, never starts with Windows, never deletes without
  asking, never touches app private data, leaves nothing on the phone.
- **Helper APK release:** CI builds `companion` with Gradle, attaches
  `phone-backup-helper-<version>.apk` and its checksum.

## Steps

1. `cargo deny` config and CI job.
2. "What this app never does" page and FAQ section.
3. Release job attaches the helper APK and checksum.
4. MSI with cargo-wix; release attaches it; download page offers both.
5. SignPath application and signing step (exe, MSI).
6. winget manifest.
7. Reproducible build doc.

## Tests

- CI dry runs on a branch with `workflow_dispatch`; install and uninstall MSI on a clean
  Windows VM; check signature with `signtool verify`.

## Risks and open questions

- SignPath approval takes time and has rules (OSS licence, build in CI); start early.
- SmartScreen reputation builds over downloads even when signed; the first releases may
  still warn.

## Not in this group

Automatic updates (keep the manual check; an auto-updater is another trust question),
Mac notarisation (16).

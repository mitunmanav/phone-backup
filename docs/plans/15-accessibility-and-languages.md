# 15 Accessibility and languages

**Branch:** `plan/15-accessibility-and-languages` · **Depends on:** nothing · **Size:** medium

## Why

MobileTrans, Dr.Fone, Coolmuster, MobiKin and Syncios ship in many languages; most of
the world's Android users do not read English first. PLAN.md's *Comfort* group lists
keyboard shortcuts, screen-reader labels, large text and dropping a backup folder on the
window, and its ideas list "other languages". None of the competitors talk about
accessibility, so doing it well is a way to stand out.

## Features in this group

| Feature (catalog row U, F) | Today | After this group |
|---|---|---|
| Screen-reader labels | egui defaults | Every button, tick, tile and picture labelled through AccessKit; courier animation hidden from readers |
| Keyboard use | partial | Every action reachable by Tab; visible focus ring; shortcuts (Ctrl+B back up, Ctrl+R restore page, Esc stop with confirm, F5 rescan phones, Ctrl+F search) |
| Large text | auto zoom by window size, Ctrl +/- | Text size setting (100–200 %) that wins over auto zoom, saved |
| High contrast | light and dark | High-contrast theme, and follow Windows' high-contrast setting |
| Reduced motion | animation on/off | Follow Windows' "Show animations" setting by default |
| Drop a backup folder on the window | — | Opens it on the Read or Restore page |
| Other languages | English | String table with a first set of languages, chosen in Settings, defaulting to Windows' language |

## What exists now

- `eframe` 0.37 (AccessKit available), `theme.rs` light/dark tokens, `main.rs` auto zoom,
  Settings for animations and sounds.
- All UI strings are inline literals across `src/ui/*.rs` (around 600).

## Design

- **AccessKit:** turn on eframe's `accesskit` feature if not already, give custom widgets
  (`widgets.rs` tiles, picker rows, courier) `Response::widget_info` labels; test with
  Narrator and NVDA.
- **Shortcuts:** one table in `shell.rs`; shown in a Help → Keyboard list.
- **Strings:** move literals into `src/i18n/en.rs` as `const` keys with a `t!("key",
  args)` macro (no runtime crate needed for English). Other languages as `.ftl` files
  (Fluent, `fluent-bundle` crate) embedded in the exe; plural rules from Fluent. Start with
  Spanish, Portuguese (Brazil), Hindi, Indonesian, French, German; contributors add more.
- **Formats:** dates and sizes through one function each, already mostly in `backup.rs`
  (`size`, `count`); make them locale-aware.
- **Fonts:** egui's default font lacks Devanagari and some scripts; bundle Noto Sans
  subsets for the shipped languages and keep the exe size in check.

## Steps

1. Labels for custom widgets; Narrator check; reduced motion from Windows.
2. Keyboard focus order and shortcuts table.
3. Text size setting; high-contrast theme.
4. Drop a folder on the window.
5. String extraction to keys (mechanical, page by page, one commit per page).
6. Fluent loader, language setting, first translation (Spanish) end to end.
7. Fonts for non-Latin scripts; more languages.

## Tests

- Unit test that every key exists in every shipped language (or falls back to English and
  is listed in a CI warning).
- Snapshot of string count per page so new literals are caught in review.

## Risks and open questions

- Right-to-left scripts (Arabic, Hebrew) need egui layout support that is still limited;
  leave them out until egui supports them.
- Translations go stale; fall back to English per string, never show a key.

## Not in this group

Mac and Linux builds (16).

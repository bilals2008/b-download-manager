# Languages and right-to-left layouts

**Wave 7 · Status:** ❌ not written (the reference build has `i18n/`, 86 languages)

## Goal

Every word the reader sees comes from a catalogue, the language is chosen once,
and Arabic, Hebrew, Persian, Urdu and Uyghur get a properly mirrored window.

## Where it stands

All text is English, written inline in the components. The settings window, the
menus and every dialog would have to be read again to find it. Layout direction
is whatever the platform sends, which is always left to right.

## Steps

1. Extract every reader-facing string, one row per string, into a keyed
   catalogue with a description for the translator. This is a mechanical pass
   over `src/components`, `src/components/shell` and the window titles in Rust.
2. One reader for the catalogue in the frontend and one for the handful of
   strings the backend owns (tray tooltip, window titles, error sentences).
3. A language picker on first start, and View → Language afterwards, stored in
   `localStorage` like the other window preferences.
4. Set `dir` on the root from the chosen language, and move the chrome's
   left/right spacing to logical properties so the window mirrors without a
   second stylesheet.
5. Keep the design tokens unchanged; only the direction and the strings move.
6. A checker that fails when a key exists in one catalogue and not another, and
   when a key is used in code but missing from the catalogue.

## Files

New `src/i18n/` (catalogue, reader, direction), `src/components/settings/*`
(a languages page), `index.html` (the pre-paint language decision),
`src-tauri/src/tray.rs`, `src-tauri/src/progress.rs`,
`src-tauri/src/complete.rs` (window titles and tooltips), `scripts/`.

## Edge cases

- A longer translation must not clip: the checker also measures the longest
  label per language for the toolbar and the list headers.
- A right-to-left window with a left-to-right path or URL inside it: mark those
  spans with the opposite direction.
- A missing key falls back to English, never to a blank.

## Tests

Unit: the catalogue reader and the direction rule. Checker: every key used in
code exists, and every catalogue has it. Visual: one screen in English and one
in Arabic, compared.

## Proof

`pnpm test`, the new checker in `pnpm check:all`, then the running app in Arabic
and in German, with the window mirrored and nothing clipped.

## Out of scope

Translating the website, and any machine translation shipped without review.

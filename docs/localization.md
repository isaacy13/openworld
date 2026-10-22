# Localization

> **Status: DRAFT. Spanish (`es`) is UNREVIEWED machine translation.**
> English is the source of truth and governs in every case. The Spanish strings in
> this repository have **not** had professional legal or safety review and must not be
> shipped to users as final. Every shell falls back to English for anything `es` does
> not cover.

OpenWorld's words come from the core by **code** (`docs/api.md`, `docs/decisions.md` §7).
A shell shows the English `text` a `Line` carries, or translates by its `code`. Shells
never compare English text to decide anything. Localization is therefore additive: a new
language is a table keyed by the same codes and the same shell-string ids, and English
stays exactly as it was.

## What `es` covers (v0 draft)

1. **Shell-owned UI strings** — the buttons, headings, and chips each shell defines
   itself (not scan sentences).
2. **The core's fixed `copy` sentences shown by code** — every sentence returned by the
   `copy` op (`core/openworld-core/src/copy.rs`, 122 codes), including the safety lines:
   `do_not_approach`, `not_identification`, `no_clearance`, the disclosure set
   (`on_device`, `nothing_uploaded`, `nobody_enrolled`, `not_affiliated`, `no_training`,
   `no_agency`, `do_not_share`, `groups_unmeasured`, `lawful_use`, `unauthenticated`,
   `not_real_proof`, `network_note`), and the leaving lines (`leaving`, `not_sending`,
   `leave_wanted`, `leave_missing`).

These were translated faithfully and conservatively. Kept **verbatim**, never translated:
`FBI`, `NCMEC`, `OpenWorld`, `OneX Engineering`, `911`, `1-800-CALL-FBI (1-800-225-5324)`,
`1-800-THE-LOST (1-800-843-5678)`, `tips.fbi.gov`, `fbi.gov`, `api.fbi.gov`, `SHA-256`,
`ONNX Runtime`, `CPU`, `GPU`, `Neural Engine`, and all URLs and format placeholders
(`%@`, `%lld`, `%1$s`, `{arg}`, …).

## What `es` does NOT cover

- **The long legal policy documents** `docs/legal/*.md` — Terms of Use (`terms.md`),
  Privacy Policy (`privacy.md`), Biometric Data Policy (`biometric.md`), and the
  Washington Consumer Health Data policy (`washington-health-data.md`), plus the licenses
  list. These are **legally operative** text served verbatim by the `legal_document` op.
  They are left **English on purpose**: a machine translation of binding legal text could
  change its meaning and its legal effect. A shell that shows them (the reading sheet on
  First run / About) shows the English paragraphs for now. Their short **titles**
  (`doc_terms`, `doc_privacy`, `doc_biometric`, `doc_health_data`, `doc_licenses`) are
  translated as link labels only; the document bodies stay English.
- **Arg-bearing core lines** built in English by the core — `faces_some`/`faces_all`/
  `faces_none`, `chance`, `frames_one`/`frames_all`/`frames_some`, `detect_full`/
  `detect_scaled`, `frame`, `frames_span`, `duration_*`. These are **not** part of the
  `copy` catalog; the core composes them in English with the values already substituted,
  so there is no code→template entry to translate by. Every shell shows the core's English
  for them (see "Gaps" below). This is the one place a Spanish user still sees English scan
  sentences.
- **Composed/interpolated helper strings in the desktop shell** (`strings.rs`:
  `estimate_summary`, `progress`, `pack_footer`, `update_progress`, `kept`, `skipped`,
  `candidate_count`, `face_label`, `class_chip`, `file_kind`, `clock`, `recorded`,
  `bundle_name`). These stitch numbers and English scaffolding together and are left
  English; proper Spanish needs grammar and plural work. Because some embed a translated
  label (e.g. `face_label` uses the `es` chip label), they can read as **mixed** under a
  Spanish locale. Flagged for the follow-up.

## How each shell selects `es`

### Apple (iPhone / Mac) — String Catalogs

- **Core sentences:** `apple/App/Resources/Core.xcstrings`, keyed by core code. Added an
  `"es"` `stringUnit` under each of the 122 codes. `Line.localized()`
  (`apple/Sources/OpenWorldKit/Line.swift`) looks a no-arg line up by `code` in the `Core`
  table; iOS/macOS pick `es` on a Spanish device and fall back to the catalog's source
  language (English) otherwise. Arg-bearing lines return the core's `text` and are not
  looked up.
- **Shell strings:** `apple/App/Resources/Localizable.xcstrings`, keyed by the English
  source string. Added `"es"` units (including the plural `variations`) for 109 keys. Six
  keys are intentionally left to English fallback: `OpenWorld`, `Demo`, `CPU`, `GPU`,
  `Neural Engine`, and the bare format key `%@: %@`.
- **`scripts/core_strings.py`:** regenerates `Core.xcstrings` from the core's `copy`
  answer and CI runs it with `--check`. It previously wrote only `"en"`, so adding `"es"`
  would have failed the check. **Fixed** so it now *merges*: it reads the existing
  catalog, (re)writes `"en"` from the core every time (English stays the source of truth),
  and **preserves** any other localization already present (the `es` draft), emitting the
  localizations in sorted order (`en`, then `es`) for a deterministic, byte-stable file.
  A code the core no longer emits is dropped with its translations. `--check` passes with
  `es` present, and the diff to `Core.xcstrings` is purely additive (732 inserted lines,
  0 English lines removed).

### Android — resource qualifier folders

- **Shell strings:** `android/app/src/main/res/values-es/strings.xml`. Android selects it
  on a Spanish device and falls back to `values/strings.xml` (English, unchanged)
  otherwise. All 126 English shell string/plurals names have an `es` entry.
- **Core sentences:** Android already has a code→resource path. `Words.of`
  (`ui/Components.kt`) looks up `core_<code>` and otherwise uses the core's `text`. Added
  `core_<code>` entries (121 codes; `missing_program` is served by the existing
  `core_missing_program` default) to `values-es` only. Because they are **translation
  only** (no English default), `Words.of` was hardened: it now wraps the `getString`
  resolution in `runCatching` so a non-Spanish device (where a `core_` resource has no
  value) falls through to the core's English `text` instead of throwing. English behavior
  is therefore unchanged — it still comes from the core. The `values-es` root carries
  `tools:ignore="MissingDefaultResource"`, because translation-only resources without a
  base entry are intentional here.

### Desktop (Linux / Windows) — `strings.rs` tables

- `strings.rs` previously held one English `text(S)` table with the note "Swap this table
  for a catalog to translate." Now:
  - `text(S)` is locale-aware: on a Spanish locale it returns `text_es(S)`, falling back
    to the unchanged English table `text_en(S)`.
  - `say(&Line)` translates no-arg core lines through a new `core_es(code)` catalog on a
    Spanish locale, falling back to the core's `line.text`; arg-bearing lines (`line.args`
    non-empty) always use the core's English text.
  - Locale is read once from the environment: `LC_ALL`, then `LC_MESSAGES`, then `LANG`;
    a value beginning with `es` selects Spanish. Anything else, including unset, is
    English (`is_spanish()`).
- Verified: `cargo build`, `cargo clippy --all-targets --features shots -- -D warnings`,
  and `cargo test` are all green under the default (non-`es`) locale. Note: the
  English-literal unit tests in `strings.rs`/`model.rs` assert English output and so must
  run under a non-`es` locale (as CI does); under `LANG=es` two of them fail because they
  compare against English scaffolding in the composed helpers listed under "Gaps".

## Verified in this change

- `python3 scripts/core_strings.py core/target/release/openworld --check` — **passes**
  with the `es` draft present.
- `Core.xcstrings` and `Localizable.xcstrings` — valid JSON; English values unchanged
  (diffs are additive only).
- `android/.../values/strings.xml` and `values-es/strings.xml` — well-formed XML; every
  English name has an `es` counterpart.
- Desktop — builds, clippy-clean, tests pass (default locale).

Not runnable in this environment, so **not** verified here: the full Xcode build and
`swift test`, and the Android Gradle `assembleDebug`/`assembleRelease`/`testDebugUnitTest`
and `lintVitalRelease`.

## Gaps still open

1. **Arg-bearing core scan sentences are not translatable by code.** The core builds
   `faces_*`, `chance`, `frames_*`, `detect_*`, `frame`, `frames_span`, and the duration
   lines in English with the numbers already inserted, and they are not in the `copy`
   catalog. To localize them, the core would need to emit a **template** per code (with
   `{arg}` placeholders and locale-aware pluralization) and the shells render the template
   with `args`. Android's `Words.of` already does `{arg}` substitution, so the core-side
   template work is the missing piece. Until then every shell shows English for these.
2. **Desktop composed helpers** (`estimate_summary`, `progress`, `pack_footer`, etc.) are
   English and can read mixed under `es`; they need locale-aware number/plural formatting.
3. **Android resource shrinking.** Release builds use `isShrinkResources = true`; the
   dynamically-referenced `core_*`/shell-code strings rely on the shrinker's safe mode
   keeping them. This is the same assumption the existing `core_missing_program` /
   `line.code` lookups already make, but it should be confirmed on a real release build
   (add `tools:keep` if the shrinker ever strips them).
4. **Only `es` exists.** No other language, and no per-region variants (`es-419`,
   `es-ES`); any Spanish locale gets the one draft.

## Required before any release

**Professional legal and safety review of the Spanish strings by a qualified
human translator/reviewer (legal review for the safety and disclosure lines), on the same
footing as the English review that gates a public release (`docs/legal-review.md`,
`docs/release-gates.md`).** Concretely:

1. A native-fluent legal/safety reviewer signs off on every `es` string, with special
   attention to the safety lines (`do_not_approach`, `leave_wanted`, `leave_missing`,
   `no_clearance`, `not_identification`, `do_not_share`) and the consent/disclosure lines
   (`accept_terms`, the disclosure set).
2. Decide whether the legal policy documents (`docs/legal/*.md`) need an official Spanish
   version; if so, translate and review them separately (they are out of scope here and
   remain English).
3. Fill the arg-bearing-line gap (#1 above) so Spanish users do not see English scan
   sentences, or decide it is acceptable for the first localized release and say so.
4. Remove the DRAFT/UNREVIEWED markers from this file and the shell catalogs only after
   that sign-off. Until then, treat every `es` string as provisional.

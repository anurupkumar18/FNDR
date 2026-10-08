# Capture fixture corpus (CAP-03)

30 synthetic screens across 6 classes (5 each): `editor`, `terminal`,
`browser_article`, `chat_mock`, `pdf_paper`, `privacy_negative`.

Every image is self-authored HTML rendered to PNG via headless Chrome at
1440x900 — never a real screenshot of anyone's own work, never scraped
external content. This keeps the corpus reproducible in CI (no network
dependency, no risk of a real page's content changing under us) and
sidesteps any question about reproducing copyrighted material.

`manifest.json` entries include `{id, file, app_class, bundle_id, window_title,
expected_outcome, expected_text, cer_budget}`. Admission cases may also provide
`app_name` and `url` so the pre-frame privacy gate receives the same metadata
shape as the native capture path.

- `expected_outcome` is `"store"` (metadata should pass admission) or
  `"skip:<reason>"` (metadata should be excluded by the production privacy
  gate before pixels reach OCR — see CAP-07). `scripts/quality_lab.py fixtures`
  runs these cases through the same deterministic gate called by the native
  capture loop and checks both blocks and false-positive passes.
- `expected_text` is the ground-truth text for `store` fixtures only;
  `privacy_negative` fixtures leave it empty since they are never OCR'd.
- `cer_budget` is the max acceptable character-error-rate for `store`
  fixtures, set to the observed baseline plus 0.05 (see
  `docs/evidence/W01/ocr-cer.md` for the measurement run and findings).

Regenerating a fixture: the generation script lived in a scratch directory
during CAP-03 and was not committed (it is a one-time content-authoring
tool, not project infrastructure). To change a fixture's content, write new
HTML and render it with:

```bash
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless --disable-gpu --screenshot=<output>.png \
  --window-size=1440,900 --hide-scrollbars --force-device-scale-factor=1 \
  "file://<input>.html"
```

Reused by CAP-08 (dedupe fixture replay) and MOD-04 (gold set) per the WS1
plan.

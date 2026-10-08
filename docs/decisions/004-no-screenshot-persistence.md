# 004: No Screenshot Persistence

FNDR's stable memory pipeline should not persist raw screenshots. The capture loop needs pixel data temporarily so Apple Vision can run OCR and so frame deduplication can avoid repeated work. After that, the durable memory record should contain compact text, metadata, embeddings, and summaries rather than raw screen pixels.

This decision keeps the product aligned with the local-first privacy promise. Screen pixels can contain passwords, private messages, banking data, health information, or content from apps the user did not intend to search later. Even when the database is local, retaining screenshots creates more sensitive data than the current stable search experience needs.

Privacy exclusions are checked before screen capture and OCR. Blocklisted apps, internal FNDR windows, and blocked URLs or titles are skipped before the expensive and sensitive parts of the pipeline run. Sensitive-context alerts are separate from the blocklist: they can warn the user about potentially private screens, but they do not justify persisting pixels.

The LanceDB schema still contains screenshot/image-related fields for compatibility with older records and adjacent experimental work. Capture records set `screenshot_path` to `None`. Store compaction also clears screenshot paths before indexing compact memory payloads. Visual semantic search can be reintroduced later only with an explicit privacy design.

## Update 2026-09-08: explicit Screen Guide captures

Screen Guide may capture the current display only after an explicit user request. The privacy policy is evaluated before pixels are obtained, Screen Guide's own panel and overlay are hidden before capture, and a normal turn keeps the resulting image only in process memory long enough for OCR and answer generation. A normal turn must not assign a `screenshot_path`, write pixels to a file, or create a `MemoryRecord`.

## Update 2026-09-28: explicit one-turn Screen Guide diagnostics

Screen Guide has one narrow troubleshooting exception to the normal ephemeral rule. Selecting **Save next turn** explicitly arms diagnostics for five minutes. The arm is consumed by the next display-reading turn; filename-only lookup does not consume it. Raw pixels remain in memory until OCR completes and the post-OCR safety gate allows the turn. Only then may FNDR save the exact encoded screenshot supplied to OCR, exact OCR text and positioned lines, and a bounded manifest of stage timing, display geometry, aggregate image statistics, and privacy-reduced context probes. An OCR failure retains aggregate evidence only.

Diagnostic bundles stay inside private FNDR app data. Their files are never indexed, embedded, added to Memory or LanceDB, supplied as model context, uploaded, or exported automatically. The diagnostics directory uses mode `0700` and its files use mode `0600`. Startup, periodic five-minute, and lazy command/session cleanup remove abandoned partials and completed bundles older than 24 hours. At most two completed bundles are kept, and active partials plus completed bundles share a 64 MiB cap. The panel reports completed bundles, active partials, bytes, and a typed save/error receipt; it can delete all diagnostic files and cancel an unused arm. An explicit backend-owned reveal action opens only FNDR's fixed local diagnostics directory.

Private Mode refuses diagnostic arming and revokes a pending arm or active turn. If a safety rule rejects the turn, FNDR writes no raw capture or OCR artifacts; any privacy-blocked manifest removes app and bundle identity and keeps only aggregate stage/context evidence. If the exact artifacts would exceed the storage budget or a write fails, no incomplete bundle is published and the UI receives a typed receipt stating that the screenshot and OCR were not saved. This exception is for explicit local debugging, not memory history, and does not broaden the stable capture pipeline's no-screenshot-persistence contract.

## Update 2026-09-30: debug-build Memory Journey exception

Memory Journey adds a second, separately bounded developer exception. In a
debug build, **Record next capture** may explicitly arm one capture attempt so
the exact frame supplied to the production capture pipeline can be correlated
with its observed OCR, cleanup, extraction, embedding-contract, storage,
retrieval, and presentation evidence. The arm is consumed by the next attempt,
including a privacy or admission skip. A pre-capture privacy rejection stores
no pixels, OCR, prompt, or content artifacts.

Memory Journey data stays in a private backend-owned app-data directory. It is
never assigned to `screenshot_path`, indexed, embedded, added to Memory or
LanceDB, used as model context, analyzed, or uploaded. Writes use partial data
and atomic publication with owner-only permissions. Retention is limited to 24
hours, six completed bundles, one active recording, and 128 MiB total. Export,
reveal, and deletion require explicit developer actions. Production builds
contain neither its raw-artifact commands nor its UI.

This exception exists to establish a six-case current-state quality baseline.
It does not authorize screenshot persistence in the stable capture pipeline or
generalize the narrower Screen Guide diagnostic contract.

## Update 2026-05-13: CLIP image vectors on screen captures

Screen captures now compute and store a 512-d CLIP image embedding alongside the existing text embeddings. The vector is derived from the same pixel buffer that already passes through Apple Vision OCR; **no raw pixels are persisted** (`screenshot_path` remains `None`). The vector is a compact, L2-normalized float32 representation — not a screenshot.

The CLIP embedding step runs **after** every existing privacy and signal gate fires (blocklist, internal-app exclusion, sensitive-context detection, surface policy, OCR low-signal, noise score, semantic dedup, grounding floor). Frames that are dropped before storage are never embedded. The embedding inherits the same protection as the OCR text it accompanies.

A new image-to-image retrieval surface (`find_visually_similar_memories`) uses cosine similarity over the `image_embedding` column. Cross-modal text->image retrieval (e.g. searching captures with a free-text query routed through a CLIP text tower) remains explicitly out of scope until a separate privacy design is documented. Records that pre-date this change keep their zero image vector and are filtered out of image-to-image results.

# 004: No Screenshot Persistence

FNDR's stable memory pipeline should not persist raw screenshots. The capture loop needs pixel data temporarily so Apple Vision can run OCR and so frame deduplication can avoid repeated work. After that, the durable memory record should contain compact text, metadata, embeddings, and summaries rather than raw screen pixels.

This decision keeps the product aligned with the local-first privacy promise. Screen pixels can contain passwords, private messages, banking data, health information, or content from apps the user did not intend to search later. Even when the database is local, retaining screenshots creates more sensitive data than the current stable search experience needs.

Privacy exclusions are checked before screen capture and OCR. Blocklisted apps, internal FNDR windows, and blocked URLs or titles are skipped before the expensive and sensitive parts of the pipeline run. Sensitive-context alerts are separate from the blocklist: they can warn the user about potentially private screens, but they do not justify persisting pixels.

The LanceDB schema still contains screenshot/image-related fields for compatibility with older records and adjacent experimental work. Current capture records set `screenshot_path` to `None` and write a zero image vector. Store compaction also clears screenshot paths before indexing compact memory payloads. Visual semantic search can be reintroduced later only with an explicit privacy design.

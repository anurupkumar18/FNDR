# `src/shared` — cross-domain frontend

| Path | Role |
| --- | --- |
| **`ipc/`** | Tauri `invoke` wrappers and DTO types (`tauri.ts`, `onboarding.ts`). Not HTTP APIs. |
| **`hooks/`** | Hooks used from multiple domains (`useSearch`, `usePolling`, …). |
| **`utils/`** | Pure helpers, app config constants, search helpers. |
| **`theme/`** | Visual tokens and cinematic palettes. |

Domain-specific hooks stay under `src/domains/<name>/`.

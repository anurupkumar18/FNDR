# Workspace panels

This folder holds **full-screen or overlay panels** that are opened from the sidebar or command palette (`AppPanels`). Each file is still named `*Panel.tsx` for discoverability.

When a panel grows past ~400 lines or mixes unrelated concerns (settings + capture + search), split it into a subfolder (for example `workspace/agent/`) and re-export a thin panel wrapper—see `docs/setup/engineering/repo-layout.md`.

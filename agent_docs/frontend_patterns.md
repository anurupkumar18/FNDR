# Frontend patterns

FNDR panels run in both the Tauri app and a synthetic browser preview. Native IPC stays behind the wrappers in `src/shared/ipc/tauri.ts`.

- `src/app/AppPanels.tsx` mounts domain panels through `PanelPresence`. Panels remain mounted when hidden; use `isVisible` for rendering and focus ownership.
- `PanelHeader` provides consistent headings, actions and close controls. `useModalFocus` traps modal focus and restores the invoking control.
- Domain CSS uses the active cinematic palette and film radius tokens. Avoid hardcoded colors that bypass light and dark themes.
- `src/dev/uiPreview.tsx` installs `createPreviewIpcHandler` before rendering. `/ui-preview.html?panel=ask` opens the real Search & Ask panel with synthetic memories. It does not run the native answer service.
- Search & Ask owns an ephemeral thread in `src/domains/ask/AskPanel.tsx`. New chat clears it; reload discards it. The existing `fndrAnswer` API answers each question independently. Visible history is not model conversation context.
- Use a synchronous request ref to prevent duplicate Enter submissions, not only a disabled button. Invalidate late results when a thread is cleared or stopped. Stop waiting does not imply backend cancellation.
- Unit tests live beside domain components. Browser acceptance journeys live in `tests/e2e/`; `ask-chat.e2e.ts` checks the conversation layout and actual sidebar route.

If conversation-aware inference is added, keep retrieval evidence separate from prior generated replies rather than concatenating the transcript into a search query.

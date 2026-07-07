# Decision 013: Release Channel and Auto-Update

## Status

Accepted (Gate 0, PRD-gate0-installable)

## Context

FNDR previously had no distributable artifact: installation required a developer toolchain, and no mechanism existed to deliver fixes to installed copies. Gate 0 ("Installable") requires strangers to install from a URL and receive subsequent releases automatically. There is no Apple Developer account for this phase (final-year college project), so notarization is out of scope.

## Decision

- **Single release path:** pushing a `v*` tag runs `.github/workflows/release.yml` (tauri-action on `macos-14`), which builds an aarch64 DMG, publishes a GitHub Release, and emits an updater manifest (`latest.json`).
- **Ad-hoc signing:** builds are ad-hoc signed. First launch requires right-click → Open. Notarization can be layered in later by adding Apple credentials to the same workflow, without redesign.
- **Auto-update:** `tauri-plugin-updater` with minisign signatures, independent of Apple signing. The public key is committed in `tauri.conf.json`; the private key lives outside the repo (`~/.tauri/fndr-updater.key`) and in GitHub Actions secrets (`TAURI_SIGNING_PRIVATE_KEY`, empty `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
- **Update endpoint:** `https://github.com/anurupkumar18/FNDR/releases/latest/download/latest.json`.
- **User surface:** Settings → Updates → "Check for updates" / "Install and restart" (`tauri-plugin-process` relaunch).

## Consequences

Positive:

- One tag push produces an installable, self-updating release; every future fix reaches installed users.
- Updater integrity does not depend on Apple infrastructure.

Negative / accepted risks:

- Gatekeeper shows the unidentified-developer flow on first launch until notarization is added.
- Losing the minisign private key breaks the update chain for existing installs; the key must be backed up.
- aarch64-only until an x86_64 target is added deliberately.

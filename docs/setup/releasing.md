# Releasing FNDR

A release is one tag push. `.github/workflows/release.yml` typechecks and tests, builds an Apple Silicon DMG, publishes a GitHub Release with the updater manifest, and verifies the signature.

```bash
# bump "version" in src-tauri/tauri.conf.json and package.json first
git tag v0.3.1
git push origin v0.3.1
```

Installed copies pick the release up from Settings → Updates.

## One-time: Developer ID signing and notarization

Without these secrets the build is ad-hoc signed. macOS then blocks the first launch until the user clicks "Open Anyway" in System Settings → Privacy & Security, and can ask for Screen Recording again after updates. With them, the DMG opens like any App Store-adjacent app.

Requires an Apple Developer Program membership.

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | `base64 -i DeveloperID.p12` of a "Developer ID Application" certificate exported from Keychain Access |
| `APPLE_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID1234)` |
| `APPLE_ID` | Apple ID email of the developer account |
| `APPLE_PASSWORD` | App-specific password from appleid.apple.com (not the account password) |
| `APPLE_TEAM_ID` | 10-character Team ID from developer.apple.com → Membership |
| `KEYCHAIN_PASSWORD` | Any random string; used for the temporary CI keychain |

```bash
gh secret set APPLE_CERTIFICATE < <(base64 -i DeveloperID.p12)
gh secret set APPLE_CERTIFICATE_PASSWORD
gh secret set APPLE_SIGNING_IDENTITY
gh secret set APPLE_ID
gh secret set APPLE_PASSWORD
gh secret set APPLE_TEAM_ID
gh secret set KEYCHAIN_PASSWORD --body "$(openssl rand -hex 24)"
```

The updater's minisign keys (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) are separate and already set; see decision 013. Back up `~/.tauri/fndr-updater.key`: losing it strands every installed copy on its current version.

## Local build

```bash
npm run tauri build -- --bundles dmg --config '{"bundle":{"createUpdaterArtifacts":false}}'
# → src-tauri/target/release/bundle/dmg/FNDR_<version>_aarch64.dmg
```

On this repo's Macs with the broken Command Line Tools libc++ headers, prefix with
`CXXFLAGS="-isystem $(xcrun --show-sdk-path)/usr/include/c++/v1"`.

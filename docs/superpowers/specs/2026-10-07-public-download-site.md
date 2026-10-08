# PRD: Public FNDR Download Site

## Problem

FNDR has an automated tagged-release path but no public product surface that explains the app, exposes the current compatible DMG, or gives accurate installation and Gatekeeper guidance.

## Goal

Give a first-time visitor one trustworthy path from understanding FNDR to downloading and installing the latest full Apple Silicon release. A successful `v*` release must update the download without a website edit.

## Users / actors

- A macOS 13+ Apple Silicon user evaluating or installing FNDR.
- A maintainer publishing a full GitHub release by pushing a `v*` tag.
- GitHub Actions, which publishes the release asset and deploys the static site.

## Current behavior

- `.github/workflows/release.yml` publishes a versioned DMG and `latest.json` for full `v*` releases.
- GitHub currently has no published release.
- Installation and signing guidance exists in `README.md` and `docs/setup/releasing.md`, but there is no public download site or stable DMG asset name.

## Proposed behavior

- Deploy `website/` to GitHub Pages from `main` when site source changes.
- Add `FNDR-latest.dmg` to every completed tagged release while preserving versioned artifacts and `latest.json`.
- Make the static primary CTA point at GitHub's stable latest-asset URL. Progressive JavaScript displays the newest full `v*` release's version, date, size, and notarization state; the static CTA remains usable when JavaScript or the API is unavailable.
- When no full release exists, say so and route visitors to GitHub Releases instead of claiming a download is available.

## Non-goals

- No app runtime, updater endpoint, analytics, account, backend, database, cookie, custom domain, Intel build, or external deployment change.
- No automated repository settings change, release creation, commit, push, or deployment.

## User workflows

1. A visitor reads the local-first proposition, confirms compatibility, and chooses **Download for Mac**.
2. The visitor opens the DMG, drags FNDR to Applications, and follows either the normal notarized path or the documented System Settings approval path for an ad-hoc build.
3. A maintainer pushes a `v*` tag; the existing release job builds and publishes, then uploads the stable alias. The site resolves the new release automatically.

## Functional requirements

- FR1: One primary download action targets only a DMG from the latest full `v*` release.
- FR2: Static HTML contains the essential compatibility, privacy, installation, and Gatekeeper information.
- FR3: Release enhancement ignores drafts, prereleases, updater archives, and non-`v*` tags.
- FR4: GitHub API failure does not remove the static download or release links.
- FR5: Pages deploys only public site assets with official GitHub Pages actions and minimum job permissions.

## Non-functional requirements

- Performance: no framework, external font, tracker, or render-blocking third-party asset.
- Reliability: GitHub Releases remains the single release/version source of truth.
- Security/privacy: strict static CSP; no form, cookie, storage, analytics, or user data collection.
- Accessibility: semantic landmarks and lists, keyboard focus, 44px primary targets, AA contrast, 200% zoom, and explicit reduced-motion, increased-contrast, and opaque fallbacks.
- Maintainability: plain HTML/CSS/ES modules with focused Node boundary tests.

## Domain language

| Term | Meaning | Existing code/docs |
| --- | --- | --- |
| Searchable local memory | Searchable records derived from permitted desktop context and stored locally | `README.md`, `docs/CONTEXT.md` |
| Full release | A non-draft, non-prerelease GitHub Release published from a `v*` tag | ADR 013, `release.yml` |
| Stable DMG alias | `FNDR-latest.dmg`, uploaded alongside the versioned DMG | New release workflow step |
| Ad-hoc signed release | A release that can require **Open Anyway** in Privacy & Security | ADR 013, releasing guide |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
| --- | --- | --- | --- |
| `website/` | New static product/download site | Public Pages site | Copy, links, fallback, selection, accessibility tokens |
| `.github/workflows/release.yml` | Upload stable DMG alias after verification | New stable release asset URL | Workflow boundary assertions, YAML parse |
| `.github/workflows/pages.yml` | Deploy static site | GitHub Pages environment | Workflow boundary assertions, YAML parse |
| Release docs / ADR 013 | Document website and setup | Maintainer process only | Documentation assertions |

## Data flow

`v*` tag → existing verification/build/release → signature verification → upload `FNDR-latest.dmg` → GitHub latest full release → static CTA and optional Releases API metadata.

## Acceptance criteria

- [ ] Site remains understandable and downloadable without JavaScript after a release exists.
- [ ] Latest full release metadata enhances the CTA without selecting prereleases or updater archives.
- [ ] No hard-coded product version exists in site source.
- [ ] Both notarized and ad-hoc Gatekeeper paths are accurate.
- [ ] Pages workflow uses official actions and least job permissions.
- [ ] Focused tests, YAML parsing, local browser checks, and design/accessibility review pass.

## Test plan

- Node tests for release/asset selection, copy/link/fallback semantics, version-source invariants, and workflow structure.
- Parse both edited workflows as YAML.
- Serve the folder locally and inspect compact/regular widths, keyboard focus, no-script behavior, and accessibility media-query fallbacks.

## Rollout / migration plan

Merge to the default branch, enable Pages with **GitHub Actions** as its source once, then publish the next full release with a `v*` tag. Existing updater clients and versioned assets are unchanged.

## Risks

- Before the first public release, no DMG can be offered; the site must state that honestly.
- GitHub's unauthenticated API can be unavailable or rate-limited; the stable alias is the no-API path.
- Signing state is inferred only from the release text emitted by the controlled workflow; the install guide always covers both paths.

## Open questions

None blocking. A custom domain and Intel build remain deliberate future decisions.

# Public download site implementation plan

## Milestone / goal

Ship one reviewable vertical slice from tagged release to public, accessible DMG download.

## Recommended order

1. Protect the public release contract with focused tests.
2. Build the static product/download experience.
3. Connect release aliasing and Pages deployment, then document and audit it.

## Slice 1: Release contract tests

**Outcome:** A deterministic boundary defines which release and asset may become the public download.

**Scope:** Node tests for full `v*` release selection, exact DMG preference, updater/prerelease exclusion, static fallback, required copy, and no hard-coded version.

**Verification:** `node --test website/tests/*.test.mjs`

**Reuse/delete guidance:** Use Node's built-in test runner and ES modules; add no test dependency or site framework.

## Slice 2: Public site

**Outcome:** Visitors can understand FNDR, check compatibility, download, install, and verify the privacy posture from one responsive page.

**Scope:** Semantic HTML, CSS tokens and media queries, a small release ES module, and local SVG brand asset.

**Verification:** focused Node test plus local browser inspection at compact and regular widths.

**Reuse/delete guidance:** Translate the app's system palette, SF stack, aurora pointer response, and critically damped timing directly; do not copy React/WebGL infrastructure into the site.

**Dependency:** Slice 1.

## Slice 3: Release and Pages integration

**Outcome:** A successful `v*` release exposes `FNDR-latest.dmg`; site changes on `main` deploy through Pages.

**Scope:** One alias-upload step in the existing release job, one Pages workflow, ADR/releasing/README updates.

**Verification:** focused Node test, YAML parse, repository diff review.

**Reuse/delete guidance:** Keep tauri-action as sole publisher and GitHub Releases as sole version source; do not add a manifest service or duplicate release workflow.

**Dependency:** Slices 1 and 2.

## Final reviews

- Anti-bloat: reject dependencies, duplicated release state, generic components, and speculative configuration.
- Design/accessibility: verify contrast, focus, target sizes, reduced motion, increased contrast, transparency fallback, 200% zoom, and glass-layer discipline.

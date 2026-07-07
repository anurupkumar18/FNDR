# Gate 0 release QA checklist

Manual verification for an installable FNDR release, per `PRD-gate0-installable.md`.
Run on a clean macOS 13+ Apple Silicon machine (no dev tools, no Python) before
announcing a release. Check each box in a release PR or issue.

## Install

- [ ] DMG downloads from the GitHub Release and mounts.
- [ ] App copies to Applications and opens via right-click → Open (ad-hoc signed; no other Gatekeeper bypass needed).
- [ ] Onboarding starts on first launch.

## Onboarding

- [ ] The required search embedder (~90 MB) starts downloading automatically on the model step.
- [ ] Progress, percent, and log lines render during the download.
- [ ] Quitting the app mid-download and reopening resumes or restarts cleanly.
- [ ] A corrupted download is rejected (checksum failure surfaces in logs; file re-downloads).
- [ ] The Qwen model is offered as an optional choice; "Skip for now" works.
- [ ] Screen-recording permission prompt appears with the privacy explanation.

## First capture

- [ ] With the embedder installed, memories appear within a few minutes of normal use.
- [ ] Search returns those memories (semantic, not just keyword).
- [ ] With the embedder deleted from `~/Library/Application Support/com.fndr.app/models/`, capture pauses visibly (Settings → Model shows the paused warning; skip counter `no embedding model` climbs) and **no** new memories are stored.
- [ ] Re-adding the model resumes capture within ~30 seconds without a restart.

## Degraded environment

- [ ] With no Python installed: Meetings record button is disabled or transcription errors are readable (no crash).
- [ ] With no ffmpeg: Meetings record button is disabled.
- [ ] Core capture/search is unaffected by missing Python/ffmpeg.

## Update path

- [ ] Settings → Updates → "Check for updates" reports up to date on the latest release.
- [ ] From release N−1: check finds N, "Install and restart" downloads, installs, and relaunches into N.
- [ ] `latest.json` on the Release lists the new version with a valid signature.

## Data safety

- [ ] Pause/resume capture works from the workspace.
- [ ] "Delete all data" removes memories (search returns nothing afterwards).

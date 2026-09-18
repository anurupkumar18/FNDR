#!/usr/bin/env bash
# Launches FNDR on the seeded demo profile. Set FNDR_DEMO_REVIEW_BACKFILL=1 once
# so the on-device review worker reviews the seeded week.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
DEMO="${FNDR_DEMO_DIR:-$HOME/Library/Application Support/com.fndr.app.demo}"
[[ -d "$DEMO/lancedb" ]] || "$REPO/scripts/demo/seed-demo-profile.sh"
cd "$REPO"
export FNDR_DATA_DIR="$DEMO"
export FNDR_DEMO_REVIEW_BACKFILL="${FNDR_DEMO_REVIEW_BACKFILL:-0}"
exec npm run tauri dev

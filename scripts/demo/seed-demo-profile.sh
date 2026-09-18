#!/usr/bin/env bash
# Builds the seeded alpha-demo profile. Never touches the real com.fndr.app data.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
REAL="$HOME/Library/Application Support/com.fndr.app"
DEMO="${FNDR_DEMO_DIR:-$HOME/Library/Application Support/com.fndr.app.demo}"

if [[ "${1:-}" == "--reset" && -d "$DEMO" ]]; then
  mv "$DEMO" "$HOME/.Trash/com.fndr.app.demo.$(date +%Y%m%d-%H%M%S)"
elif [[ -d "$DEMO/lancedb" ]]; then
  echo "Demo profile already exists at $DEMO (use --reset to rebuild)"; exit 0
fi

mkdir -p "$DEMO"
[[ -d "$REAL/models" ]] || { echo "Missing $REAL/models — install models via the app first"; exit 1; }
ln -sfn "$REAL/models" "$DEMO/models"
if [[ -d "$REAL/speech_models" ]]; then ln -sfn "$REAL/speech_models" "$DEMO/speech_models"; fi

NAME="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("display_name") or "Anurup")' "$REAL/onboarding.json" 2>/dev/null || echo Anurup)"
cat > "$DEMO/onboarding.json" <<JSON
{
  "step": "complete",
  "biometric_enabled": false,
  "screen_permission": true,
  "accessibility_permission": true,
  "model_downloaded": true,
  "model_id": "qwen3-vl-2b",
  "display_name": "$NAME"
}
JSON

cd "$REPO/src-tauri"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo run --example seed_demo -- \
  --data-dir "$DEMO" --corpus "$REPO/scripts/demo/demo-week.json"
echo "Demo profile ready: $DEMO"

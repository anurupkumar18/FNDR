#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/src-tauri/Cargo.toml"
TARGET_DIR="$REPO_ROOT/src-tauri/target"
RUNTIME_DIR="${HOME}/Library/Application Support/com.fndr.app"

DRY_RUN=0
ASSUME_YES=0

usage() {
    cat <<USAGE
Usage: scripts/clean-dev-build-cache.sh [--dry-run] [--yes]

Safely removes Rust/Tauri developer build artifacts with cargo clean.
This does not delete FNDR runtime data, memory cards, LanceDB, summaries, models,
screenshots, or app settings.

Options:
  --dry-run   Show what would be cleaned without deleting anything.
  --yes       Run without prompting.
  --help      Show this help.
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --yes|-y)
            ASSUME_YES=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

size_of() {
    local path="$1"
    if [[ -e "$path" ]]; then
        du -sh "$path" 2>/dev/null | awk '{print $1}'
    else
        echo "0B"
    fi
}

echo "FNDR developer build cache cleanup"
echo
echo "Build cache target:"
echo "  src-tauri/target: $(size_of "$TARGET_DIR")"
echo "  debug:            $(size_of "$TARGET_DIR/debug")"
echo "  release:          $(size_of "$TARGET_DIR/release")"
echo
echo "Protected runtime data, not touched:"
echo "  app data:         $(size_of "$RUNTIME_DIR")"
echo "  memory DB:        $(size_of "$RUNTIME_DIR/lancedb")"
echo "  frames:           $(size_of "$RUNTIME_DIR/frames")"
echo

if [[ ! -f "$MANIFEST_PATH" ]]; then
    echo "Could not find Cargo manifest at $MANIFEST_PATH" >&2
    exit 1
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "Dry run only. To clean: npm run clean:dev-cache"
    exit 0
fi

if [[ ! -e "$TARGET_DIR" ]]; then
    echo "Nothing to clean; src-tauri/target does not exist."
    exit 0
fi

if [[ "$ASSUME_YES" -ne 1 ]]; then
    printf "Remove Rust/Tauri build artifacts now? This will make the next Rust build slower. [y/N] "
    read -r reply
    case "$reply" in
        y|Y|yes|YES)
            ;;
        *)
            echo "Cancelled."
            exit 0
            ;;
    esac
fi

cargo clean --manifest-path "$MANIFEST_PATH"

echo
echo "Cleanup complete."
echo "  repo:             $(size_of "$REPO_ROOT")"
echo "  src-tauri/target: $(size_of "$TARGET_DIR")"
echo "  memory DB:        $(size_of "$RUNTIME_DIR/lancedb")"

#!/usr/bin/env bash
# Runs the Rust library tests on the committed tree plus only the files you
# name, in a scratch copy. Use it when another session's uncommitted work
# breaks the build in this checkout, or to prove a change stands on its own.
#
#   scripts/dev/test-clean.sh [-f path]... [cargo test filter and flags]
#
#   scripts/dev/test-clean.sh -f src-tauri/src/context_runtime/retrieve.rs context_runtime::retrieve
#
# Paths are relative to the repo root. With no -f it tests the committed tree.
# Integration tests under src-tauri/tests read fixtures by path and run fine in
# the checkout itself; this script is for `cargo test --lib`.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
CLEAN="${FNDR_CLEAN_DIR:-${TMPDIR:-/tmp}/fndr-clean-copy}"

files=()
while [[ "${1:-}" == "-f" ]]; do
  [[ -n "${2:-}" ]] || { echo "-f needs a path" >&2; exit 2; }
  files+=("$2")
  shift 2
done

rm -rf "$CLEAN/src-tauri"
mkdir -p "$CLEAN/dist"
git -C "$REPO" archive HEAD src-tauri | tar -x -C "$CLEAN"
for file in "${files[@]:-}"; do
  [[ -z "$file" ]] && continue
  [[ "$file" == src-tauri/* && -f "$REPO/$file" ]] || { echo "Not a file under src-tauri: $file" >&2; exit 2; }
  mkdir -p "$CLEAN/$(dirname "$file")"
  cp "$REPO/$file" "$CLEAN/$file"
done

echo "Testing HEAD $(git -C "$REPO" rev-parse --short HEAD) plus ${#files[@]} working-tree file(s) in $CLEAN" >&2
cd "$CLEAN/src-tauri"
cargo test --lib "$@"

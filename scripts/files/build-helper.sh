#!/bin/bash
# Builds fndr-files-helper (Files search's EmbeddingGemma 2, ADR 033) and
# stages it as src-tauri/binaries/fndr-files-helper-<target triple>, where
# Tauri's external binaries live. Builds the pinned llama.cpp first.
set -euo pipefail
cd "$(dirname "$0")/../.."
scripts/files/build-llama.sh
TARGET=$(rustc -vV | sed -n 's/^host: //p')
(cd src-tauri/helpers/fndr-files && cargo build --release)
mkdir -p src-tauri/binaries
cp src-tauri/helpers/fndr-files/target/release/fndr-files-helper "src-tauri/binaries/fndr-files-helper-$TARGET"
echo "Staged src-tauri/binaries/fndr-files-helper-$TARGET"

#!/usr/bin/env bash
set -euo pipefail

TARGET_DIR_DEFAULT="$HOME/Library/Application Support/com.fndr.FNDR/models"
TARGET_DIR="${1:-$TARGET_DIR_DEFAULT}"
MODEL_PATH="$TARGET_DIR/all-MiniLM-L6-v2.onnx"
TOKENIZER_PATH="$TARGET_DIR/tokenizer.json"

MODEL_URL="https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx"
TOKENIZER_URL="https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json"

mkdir -p "$TARGET_DIR"

download_if_needed() {
  local url="$1"
  local output="$2"
  local min_bytes="$3"

  if [ -f "$output" ] && [ "$(wc -c < "$output")" -ge "$min_bytes" ]; then
    echo "✅ $(basename "$output") already present."
    return
  fi

  echo "📥 Downloading $(basename "$output")..."
  curl -L --fail --retry 3 --retry-delay 2 "$url" -o "$output"
}

download_if_needed "$MODEL_URL" "$MODEL_PATH" 50000000
download_if_needed "$TOKENIZER_URL" "$TOKENIZER_PATH" 100000

echo "🎉 Embedding assets ready at: $TARGET_DIR"

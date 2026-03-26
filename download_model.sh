#!/bin/bash
set -euo pipefail

: "${HF_TOKEN:?Set HF_TOKEN to a valid Hugging Face token with access to Gemma}"

MODEL_DIR="src-tauri/models"
mkdir -p "$MODEL_DIR"

BASE_URL="https://huggingface.co/google/gemma-3-4b-it-qat-q4_0-gguf/resolve/main"

MODEL_FILE="gemma-3-4b-it-q4_0.gguf"
MMPROJ_FILE="mmproj-model-f16-4B.gguf"

MODEL_PATH="$MODEL_DIR/$MODEL_FILE"
MMPROJ_PATH="$MODEL_DIR/$MMPROJ_FILE"

echo "Checking FNDR model assets..."
echo ""

if [ -f "$MODEL_PATH" ]; then
    echo "✅ Gemma 3 4B QAT already exists at $MODEL_PATH"
else
    echo "📥 Downloading Gemma 3 4B Instruct QAT Q4_0 (~2.37GB)..."
    curl -L --progress-bar \
        -H "Authorization: Bearer $HF_TOKEN" \
        "$BASE_URL/$MODEL_FILE" \
        -o "$MODEL_PATH"
    echo "✅ Gemma 3 4B download complete."
fi

echo ""

if [ -f "$MMPROJ_PATH" ]; then
    echo "✅ mmproj already exists at $MMPROJ_PATH"
else
    echo "📥 Downloading mmproj (~851MB)..."
    curl -L --progress-bar \
        -H "Authorization: Bearer $HF_TOKEN" \
        "$BASE_URL/$MMPROJ_FILE" \
        -o "$MMPROJ_PATH"
    echo "✅ mmproj download complete."
fi

echo ""
echo "🎉 All models ready."
echo ""
echo "   Model  : $MODEL_PATH  (~2.37GB)"
echo "   mmproj : $MMPROJ_PATH (~851MB)"
echo ""
echo "   Usage:"
echo "   # Text only"
echo "   llama-cli -m $MODEL_PATH [args]"
echo ""
echo "   # Text + Vision"
echo "   llama-gemma3-cli -m $MODEL_PATH --mmproj $MMPROJ_PATH --image <path> -p '<prompt>'"

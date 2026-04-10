#!/bin/bash
# Helper to download necessary AI models for FNDR
# Minimal version: Downloads 1B Text LLM and 500M Vision model only.

MODEL_DIR="src-tauri/models"
mkdir -p "$MODEL_DIR"

# ============================================
# Text LLM: Meta Llama 3.2 1B Instruct (770MB)
# ============================================
LLM_URL="https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf"
LLM_PATH="$MODEL_DIR/Llama-3.2-1B-Instruct-Q4_K_M.gguf"

if [ -f "$LLM_PATH" ] && [ "$(wc -c < "$LLM_PATH")" -gt 1000000 ]; then
    echo "✅ Llama 3.2 1B exists."
else
    echo "📥 Downloading Llama 3.2 1B (~700MB)..."
    curl -L "$LLM_URL" -o "$LLM_PATH"
fi

# ============================================
# Vision LLM: SmolVLM 500M (437MB model + 199MB projector)
# ============================================
VLM_URL="https://huggingface.co/ggml-org/SmolVLM-500M-Instruct-GGUF/resolve/main/SmolVLM-500M-Instruct-Q8_0.gguf"
MMPROJ_URL="https://huggingface.co/ggml-org/SmolVLM-500M-Instruct-GGUF/resolve/main/mmproj-SmolVLM-500M-Instruct-f16.gguf"
VLM_PATH="$MODEL_DIR/SmolVLM-500M-Instruct-Q8_0.gguf"
MMPROJ_PATH="$MODEL_DIR/mmproj-SmolVLM-500M-Instruct-f16.gguf"

if [ -f "$VLM_PATH" ] && [ "$(wc -c < "$VLM_PATH")" -gt 1000000 ]; then
    echo "✅ SmolVLM 500M exists."
else
    echo "📥 Downloading SmolVLM 500M (~437MB)..."
    curl -L "$VLM_URL" -o "$VLM_PATH"
fi

if [ -f "$MMPROJ_PATH" ] && [ "$(wc -c < "$MMPROJ_PATH")" -gt 1000000 ]; then
    echo "✅ Vision Projector exists."
else
    echo "📥 Downloading Vision Projector (~199MB)..."
    curl -L "$MMPROJ_URL" -o "$MMPROJ_PATH"
fi

echo "🎉 Minimal model set ready."

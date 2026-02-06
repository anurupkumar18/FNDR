#!/bin/bash
# Helper to download AI models for FNDR
# Downloads: Meta Llama 3.2 1B, SmolVLM 500M, SmolVLM 256M

MODEL_DIR="src-tauri/models"
mkdir -p "$MODEL_DIR"

# ============================================
# Text LLM: Meta Llama 3.2 1B Instruct
# ============================================
LLM_URL="https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf"
LLM_PATH="$MODEL_DIR/Llama-3.2-1B-Instruct-Q4_K_M.gguf"

if [ -f "$LLM_PATH" ]; then
    echo "✅ Llama 3.2 1B already exists at $LLM_PATH"
else
    echo "📥 Downloading Meta Llama 3.2 1B GGUF (~700MB)..."
    curl -L "$LLM_URL" -o "$LLM_PATH"
    echo "✅ Llama 3.2 1B download complete."
fi

# ============================================
# Vision LLM (Primary): SmolVLM 500M
# ============================================
VLM_500M_URL="https://huggingface.co/ggml-org/SmolVLM-500M-Instruct-GGUF/resolve/main/SmolVLM-500M-Instruct-Q4_K_M.gguf"
VLM_500M_MMPROJ_URL="https://huggingface.co/ggml-org/SmolVLM-500M-Instruct-GGUF/resolve/main/mmproj-SmolVLM-500M-Instruct-f16.gguf"
VLM_500M_PATH="$MODEL_DIR/SmolVLM-500M-Instruct-Q4_K_M.gguf"
VLM_500M_MMPROJ_PATH="$MODEL_DIR/mmproj-SmolVLM-500M-Instruct-f16.gguf"

if [ -f "$VLM_500M_PATH" ]; then
    echo "✅ SmolVLM 500M already exists at $VLM_500M_PATH"
else
    echo "📥 Downloading SmolVLM 500M GGUF (~400MB)..."
    curl -L "$VLM_500M_URL" -o "$VLM_500M_PATH"
    echo "✅ SmolVLM 500M download complete."
fi

if [ -f "$VLM_500M_MMPROJ_PATH" ]; then
    echo "✅ SmolVLM 500M mmproj already exists"
else
    echo "📥 Downloading SmolVLM 500M mmproj (~200MB)..."
    curl -L "$VLM_500M_MMPROJ_URL" -o "$VLM_500M_MMPROJ_PATH"
    echo "✅ SmolVLM 500M mmproj download complete."
fi

# ============================================
# Vision LLM (Fallback): SmolVLM 256M
# ============================================
VLM_256M_URL="https://huggingface.co/ggml-org/SmolVLM-256M-Instruct-GGUF/resolve/main/SmolVLM-256M-Instruct-Q4_K_M.gguf"
VLM_256M_MMPROJ_URL="https://huggingface.co/ggml-org/SmolVLM-256M-Instruct-GGUF/resolve/main/mmproj-SmolVLM-256M-Instruct-f16.gguf"
VLM_256M_PATH="$MODEL_DIR/SmolVLM-256M-Instruct-Q4_K_M.gguf"
VLM_256M_MMPROJ_PATH="$MODEL_DIR/mmproj-SmolVLM-256M-Instruct-f16.gguf"

if [ -f "$VLM_256M_PATH" ]; then
    echo "✅ SmolVLM 256M (fallback) already exists"
else
    echo "📥 Downloading SmolVLM 256M GGUF (~200MB)..."
    curl -L "$VLM_256M_URL" -o "$VLM_256M_PATH"
    echo "✅ SmolVLM 256M download complete."
fi

if [ -f "$VLM_256M_MMPROJ_PATH" ]; then
    echo "✅ SmolVLM 256M mmproj already exists"
else
    echo "📥 Downloading SmolVLM 256M mmproj (~100MB)..."
    curl -L "$VLM_256M_MMPROJ_URL" -o "$VLM_256M_MMPROJ_PATH"
    echo "✅ SmolVLM 256M mmproj download complete."
fi

echo ""
echo "🎉 All models downloaded successfully!"
echo "   Text LLM:     $LLM_PATH "
echo "   VLM Primary:  $VLM_500M_PATH"
echo "   VLM Fallback: $VLM_256M_PATH"

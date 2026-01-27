#!/bin/bash
# Helper to download Llama 3.2 1B Instruct GGUF

MODEL_DIR="src-tauri/models"
MODEL_URL="https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf"
MODEL_PATH="$MODEL_DIR/Llama-3.2-1B-Instruct-Q4_K_M.gguf"

mkdir -p "$MODEL_DIR"

if [ -f "$MODEL_PATH" ]; then
    echo "Model already exists at $MODEL_PATH"
else
    echo "Downloading Llama 3.2 1B GGUF (~0.7GB)..."
    curl -L "$MODEL_URL" -o "$MODEL_PATH"
    echo "Download complete."
fi

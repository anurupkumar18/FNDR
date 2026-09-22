#!/bin/zsh
# Checkpoint 1: count tokens the way Llama-3.2 sees text. Synthetic strings only.
M="${FNDR_LAB_MODEL:-$HOME/Library/Application Support/com.fndr.app/models/Qwen3VL-2B-Instruct-Q4_K_M.gguf}"
count() {
  local label="$1" text="$2"
  local n=$(llama-tokenize -m "$M" -p "$text" --no-bos --show-count --log-disable 2>/dev/null | awk '/Total number of tokens/ {print $NF}')
  local c=${#text}
  printf "%-8s chars=%-4s tokens=%-4s chars/token=%.2f\n" "$label" "$c" "$n" "$(echo "$c / $n" | bc -l)"
}
count "A-prose" "Meeting notes: review the quarterly roadmap and assign owners for each milestone."
count "B-code"  "src-tauri/src/capture/mod.rs:1872 fn run_capture_loop(&mut self) -> Result<(), CaptureError>"
count "C-url"   "https://capstone.cs.utah.edu/fndr/fndr/-/issues/42?utm_source=share&ref=a91f3c"

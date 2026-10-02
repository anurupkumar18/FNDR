#!/bin/sh
set -eu

helper_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bundle_dir=$(mktemp -d "${TMPDIR:-/tmp}/fndr-speech-test.XXXXXX")
app="$helper_dir/../../binaries/FNDR Speech Helper.app"
input_pipe="$bundle_dir/stdin.pipe"
output_log="$bundle_dir/output.log"

if [ ! -x "$app/Contents/MacOS/fndr-speech" ]; then
    echo "Speech helper app is missing. Run cargo check from src-tauri first." >&2
    exit 1
fi

mkfifo "$input_pipe"
: > "$output_log"

echo "FNDR Speech Test is ready. Type start, speak, then type stop; type quit when done."
tail -f "$output_log" | while IFS= read -r line; do
    case "$line" in
        *'"type":"level"'*|*'"type": "level"'*) ;;
        *) printf '%s\n' "$line" ;;
    esac
done &
tail_pid=$!
open -W -n -i "$input_pipe" -o "$output_log" --stderr "$output_log" "$app" &
open_pid=$!

exec 3> "$input_pipe"
while IFS= read -r command; do
    printf '%s\n' "$command" >&3
    if [ "$command" = "quit" ]; then
        break
    fi
done
exec 3>&-

wait "$open_pid"
kill "$tail_pid" 2>/dev/null || true
wait "$tail_pid" 2>/dev/null || true

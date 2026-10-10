#!/bin/sh
set -eu

helper_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary=$(mktemp "${TMPDIR:-/tmp}/fndr-speech.XXXXXX")
transcript_test=$(mktemp "${TMPDIR:-/tmp}/fndr-speech-transcript.XXXXXX")
trap 'rm -f "$binary" "$transcript_test"' EXIT

swiftc -parse-as-library \
    "$helper_dir/StreamingTranscript.swift" \
    "$helper_dir/streaming-transcript-test.swift" \
    -o "$transcript_test"

stop_word_test=$(mktemp "${TMPDIR:-/tmp}/fndr-speech-stop-word.XXXXXX")
trap 'rm -f "$binary" "$transcript_test" "$stop_word_test"' EXIT
swiftc -parse-as-library \
    "$helper_dir/StreamingTranscript.swift" \
    "$helper_dir/stop-word-matcher-test.swift" \
    -o "$stop_word_test"
"$stop_word_test"
"$transcript_test"

swiftc "$helper_dir/main.swift" "$helper_dir/StreamingTranscript.swift" \
    -framework AVFoundation -framework Speech \
    -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist \
    -Xlinker "$helper_dir/HelperInfo.plist" \
    -o "$binary"

# The helper is launched as its own process, so macOS TCC reads privacy
# descriptions from the helper executable rather than FNDR's app plist.
otool -l "$binary" | grep -q '__info_plist'
strings "$binary" | grep -q 'NSMicrophoneUsageDescription'
strings "$binary" | grep -q 'NSSpeechRecognitionUsageDescription'
strings "$binary" | grep -q 'CFBundleExecutable'
strings "$binary" | grep -q 'CFBundlePackageType'

output=$(printf '\nquit\n' | "$binary")

printf '%s\n' "$output" | grep -qx '{"type":"ready"}'
if printf '%s\n' "$output" | grep -q 'invalid_command'; then
    echo "Blank protocol lines must be ignored." >&2
    exit 1
fi

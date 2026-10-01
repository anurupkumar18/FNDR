#!/bin/sh
set -eu

helper_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary=$(mktemp "${TMPDIR:-/tmp}/fndr-speech.XXXXXX")
trap 'rm -f "$binary"' EXIT

swiftc "$helper_dir/main.swift" -framework AVFoundation -framework Speech -o "$binary"
output=$(printf '\nquit\n' | "$binary")

printf '%s\n' "$output" | grep -qx '{"type":"ready"}'
if printf '%s\n' "$output" | grep -q 'invalid_command'; then
    echo "Blank protocol lines must be ignored." >&2
    exit 1
fi

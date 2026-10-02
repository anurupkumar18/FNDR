#!/bin/sh

# Deterministic JSON-lines stand-in for the native speech helper. Rust tests
# launch it through /bin/sh, so this file does not need executable permissions.
scenario="$1"
marker="$2"

printf '%s\n' '{"type":"ready"}'

while IFS= read -r command; do
  case "$command" in
    start)
      case "$scenario" in
        normal|cancel|idle)
          printf '%s\n' '{"type":"listening","level":0}'
          printf '%s\n' '{"type":"level","level":0.42}'
          printf '%s\n' '{"type":"partial","text":"Show my"}'
          ;;
        error)
          printf '%s\n' '{"type":"error","code":"recognition_failed","message":"Recognition failed"}'
          ;;
        crash)
          exit 17
          ;;
      esac
      ;;
    stop)
      printf '%s\n' '{"type":"final","text":"Show my meetings"}'
      ;;
    cancel)
      # A misbehaving helper may race a late result after cancellation. The
      # Rust owner must discard it because there is no longer an active session.
      printf '%s\n' '{"type":"final","text":"late result"}'
      ;;
    quit)
      printf '%s\n' quit >> "$marker"
      exit 0
      ;;
  esac
done

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
        spot)
          printf '%s\n' '{"type":"listening","level":0}'
          printf '%s\n' '{"type":"stop_word"}'
          printf '%s\n' '{"type":"partial","text":"Show my"}'
          ;;
        error)
          printf '%s\n' '{"type":"error","code":"recognition_failed","message":"Recognition failed"}'
          ;;
        permission_denied)
          printf '%s\n' '{"type":"requesting_permission","permission":"speech_recognition"}'
          printf '%s\n' '{"type":"unavailable","reason":"permission_denied","permission":"speech_recognition","settingsPane":"speech-recognition","message":"Allow speech recognition in System Settings."}'
          ;;
        permission_restricted)
          printf '%s\n' '{"type":"unavailable","reason":"permission_restricted","permission":"microphone","settingsPane":"microphone","message":"Microphone access is restricted on this Mac."}'
          ;;
        crash)
          exit 17
          ;;
      esac
      ;;
    spot)
      # The stop-word spotter. A real helper sends no text in this mode; the
      # text lines here stand in for an older helper that still does.
      printf '%s\n' '{"type":"listening","level":0}'
      case "$scenario" in
        spot)
          printf '%s\n' '{"type":"partial","text":"open the music"}'
          printf '%s\n' '{"type":"speech_ignored"}'
          printf '%s\n' '{"type":"final","text":"turn it up"}'
          printf '%s\n' '{"type":"partial","text":"please stop"}'
          printf '%s\n' '{"type":"stop_word"}'
          ;;
        *)
          printf '%s\n' '{"type":"partial","text":"Show my"}'
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

# CAP-06 ScreenCaptureKit spike (time-boxed)

## Verdict: partial GO — crate choice validated, full implementation deferred

The plan asked to read v2's reference first
(`~/FNDR-2.0/crates/fndr-capture/src/source.rs`) and expect its friction.
v2 pinned `screencapturekit = "=9.0.1"`, which wraps a Swift shim and needs a
workspace-wide `.cargo/config.toml` rpath hack
(`-C link-arg=-Wl,-rpath,/usr/lib/swift`) or the binary **builds fine and
crashes at startup** with `dyld: Library not loaded:
@rpath/libswift_Concurrency.dylib`. v2's own comment on that file already
flags this as a liability and names `objc2-screen-capture-kit` as the
preferred fallback if the Swift-shim dependency is ever dropped.

Given that documented crash mode is exactly the kind of risk not worth
taking against FNDR v1's live, working capture path, this spike checked the
fallback first instead of repeating v2's known-fragile choice.

## What was checked

- `objc2-screen-capture-kit = "0.3.2"` added to `src-tauri/Cargo.toml` with
  the `SCScreenshotManager` and `SCShareableContent` features.
- `cargo check` **builds cleanly**: no Swift shim, no rpath hack, no
  workspace-wide config change. This alone is a meaningfully better outcome
  than v2's experience with the pinned crate.
- The crate exposes exactly the API the plan wanted:
  `SCScreenshotManager::captureImageWithFilter_configuration_completionHandler`
  returns a `CGImage` (BGRA for SDR), matching the plan's ask ("hand back
  the same image type `CGDisplay::screenshot` returns today" — `capture_screen()`
  in `capture/macos.rs` currently produces PNG bytes from a `CGImage`, so the
  underlying image type lines up).

## What was not attempted

Actually calling `captureImageWithFilter_configuration_completionHandler`
requires:

1. Building an `SCContentFilter` from `SCShareableContent` (an async
   enumeration of shareable displays/windows, itself another
   completion-handler API to bridge).
2. Bridging the Objective-C completion-handler callback
   (`Option<&block2::DynBlock<dyn Fn(*mut CGImage, *mut NSError)>>`) into
   Rust in a way that fits `capture_screen()`'s current synchronous
   contract, or changing that contract to async.
3. Converting the resulting `CGImage` to the same PNG byte format
   `capture_screen()` returns today, verified byte-for-byte compatible with
   every downstream consumer (OCR, dedupe, storage).

This is real, unsafe-FFI-heavy implementation work against FNDR's core,
live capture path, not a spike-scale change. It was not attempted today.

## Why stopping here, not pushing further

- The plan's own Step 3 requires two 30-minute live release-build sessions
  and a manual permission-recovery checklist (revoke Screen Recording in
  System Settings, confirm `ScreenCaptureFailed` without a crash, re-grant,
  confirm capture resumes without relaunch). That needs a human at the
  keyboard; it cannot be completed autonomously.
- `capture_screen()` is the single most safety-critical function in the
  app: every capture depends on it working correctly. Rushing a new backend
  behind an unverified completion-handler bridge risks the app's primary
  function for a P1 (not P0) ticket the plan itself explicitly time-boxes
  and treats as GO/NO-GO, not "implement now."

## Recommendation

`objc2-screen-capture-kit` is validated as the right crate choice: it
already avoids v2's documented crash mode with a lower-risk build. The
dependency is left in `Cargo.toml` (harmless unused until wired in) so the
next pass starts from a validated choice rather than re-deciding this.
Next real step: implement `capture/sck.rs` behind `FNDR_CAPTURE_BACKEND`
(default `cg`, unchanged), get one real frame back as PNG bytes, and only
then run the live comparison and permission-recovery checklist with a
human driving System Settings.

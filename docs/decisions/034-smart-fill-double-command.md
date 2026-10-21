# 034. Smart Fill: two taps of Command write the focused text box

Status: Proposed (2026-10-20). Requested by Kunj. Not yet reviewed by the team.

## Context

Auto-fill (Option+F) finds a stored value for a field and asks the person to pick it. It cannot answer a message or a free-text form question, because the answer is not stored anywhere; it has to be written from what is on screen.

## Decision

Two quick taps of Command, in any app with a text box focused, make FNDR write the text for that box and put it in. The person's ChatGPT plan writes it (the same one-turn, read-only Codex path Screen Guide uses). Off until switched on in Settings, next to the ChatGPT sign-in.

- **Trigger.** A global `NSEvent` monitor reads modifier changes, key-downs and clicks. A tap is Command pressed and released alone within 350 ms; two taps closer than 400 ms fire. Any key or click in between, or another modifier, starts the count again, so Cmd+C then Cmd+V never fires. The monitor reads event types and flags only, never which characters were typed.
- **Evidence sent.** The focused field's app, window, label and current value; the focused window's text from Accessibility; a downscaled screenshot of the main display; up to four memories from `context_runtime::retrieve`. The screenshot is included by default for this feature, because the message or form being answered is on screen, not in the field. The setting says so.
- **Refusals.** Password fields, Private Mode, apps and windows on the blocklist, FNDR's own windows, and anything that is not a text box. A stray double tap with no text box focused does nothing and shows nothing.
- **Writing.** Native text fields take the reply word by word through Accessibility, so the text appears as it is written. Web and Electron fields are written by one paste, because setting their AXValue does not reach the page's own state. A paste puts back the plain text the clipboard held. The reply replaces what the field held; Esc, or two more taps, stops the request and puts the old text back.
- **Feedback.** A small click-through window, hidden from screenshots, follows the caret: a breathing halo while ChatGPT reads, a sweeping one while the words go in, a short label beside it.
- **Never sends.** Smart Fill does not press Return.

## Options not taken

- A fixed hotkey like Auto-fill's. Rejected by the request: two taps of Command is the gesture asked for. It cannot be a Tauri global shortcut, which needs a non-modifier key.
- Typing every character as key events. Rejected: key events from FNDR's process have aborted before on macOS (`paste_into_frontmost_app`), and they would fight the person's own typing.
- An on-device fallback when ChatGPT is unavailable. Not built: the 2B model extracts and does not write replies (AGENTS.md, 2026-10-07). Smart Fill says ChatGPT could not be reached.

## Consequences

- Each use spends the person's Codex allowance and sends a screenshot to OpenAI. The Settings text states both.
- macOS Dictation can be set to start on Command twice; the Settings text tells the person to change one of them.
- The prompt is `smart_fill` in `inference/prompts.rs` (`LLM_PROMPT_VERSION` v15).
- Open: URL entries on the blocklist are not applied, because Accessibility does not give the page URL here; the app name and window title are.

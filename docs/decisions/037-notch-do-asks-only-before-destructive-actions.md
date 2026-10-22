# 037. Notch Do asks only before an action that could destroy something

Status: Proposed (2026-10-22). Requested by Kunj. Not yet reviewed by the team.

## Context

`operator/policy.rs` (ADR 022 amendment, ADR 026) judges every Notch Do action as it runs, in code, from the tool, its arguments and the label FNDR observed for the target: `Runs`, `Confirm` (a tap in the notch) or `Never`. Many `Confirm` verdicts are not about harm. They are about FNDR not knowing enough: "the click target is not known", "clicks a control on a web page", "types outside a search field", "uses a menu action", "Return may submit a form", "puts text into an app". A person asking for several steps taps through all of them, which is the opposite of the product's promise.

## Decision

A new setting, `notch_do_autonomy`, with two values.

- **`smooth` (default).** Ordinary actions run without a tap. A `Confirm` stays a `Confirm`, reason "could destroy something", only when a deterministic check says so (`could_destroy` in `operator/policy.rs`): the observed label of the target, a short argument such as a menu action, or a key chord names delete, remove, trash, erase, wipe, discard, clear, reset, format, uninstall, overwrite, revoke, deactivate, restore, quit, don't save and similar, as whole words, or the chord closes a window or quits an app (Cmd+W, Cmd+Q). A drag is checked the same way for its destination (Trash).
- **`ask_each`.** The policy exactly as before. `classify` is unchanged and a test pins `classify_for(AskEach, ...)` equal to it.

What `smooth` still refuses (`Never`, unchanged): payments and purchases (pay, buy, checkout, order, subscribe, upgrade, transfer), deleting and emptying the bin, entering into a password field, force quit and ending the session, delete chords, password managers and security settings, an unknown tool, a link that is not http or https. What `smooth` lifts from `Never`: a click on a button whose label is only Send, and Return in a messaging app, because sending a message the person asked for is the point of a voice request. A button that says Send and also names a refused word stays refused.

What `smooth` still asks about on purpose: a link the person's words did not ask for, whether opened directly or typed into the address bar. It is the way a page steers the planner somewhere, and a tap costs almost nothing.

Also under `smooth`, a plan starts without a "Start" tap unless it touches an app FNDR may not operate or a link nobody asked for, and a paste step is no longer a separate approval; each action in the plan is still judged when it is taken.

The setting is in Settings, Notch Do: "Ask before each action", off by default. A run keeps the value it started with.

## Options not taken

- Keep asking everywhere. It is the safe default and the reason the feature felt slow.
- Let the model say which actions are risky. The module's rule is that nothing the model says can lower a level; that stays.
- Per-app allowlists. More settings for the person to manage than the problem justifies.
- Make destructive actions `Never`. They already are for deleting; others such as "Remove from favorites" or "Clear history" ask instead, so a person who does want one can approve it.

## Risks

- **Prompt injection.** With fewer taps, a hostile page can steer the planner into ordinary clicks and typing without a person checking each one. The destructive check reads labels the page can influence, so a destructive control labelled innocently is not caught by its label. Mitigations kept: the `Never` list, the unrequested-link tap, the off-limits apps and blocklist, Private Mode and the kill switch, the audit journal, and `ask_each`.
- **Over-asking by word.** A control named "Reset" or "Clear" asks even when harmless. That is the intended error direction.
- **Unsaved work.** Cmd+W and Cmd+Q ask; a click on a window's close button is judged by its label like any click.

## Consequences

- Most requests that took several taps now take none.
- The labelled request suite and the policy tests keep their `ask_each` expectations; `smooth` has its own table.
- The Settings text describes the default. `docs/product/voice-ux.md` still describes the plan card that waits for Start; under `smooth` that card appears only for a plan that touches an app FNDR may not operate or a link nobody asked for, and it is unchanged under `ask_each`. Updating that page is follow-up work.

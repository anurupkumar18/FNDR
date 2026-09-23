# Privacy Activity native-session runbook

**Status:** runbook only. No native session has been recorded in this file.

This procedure produces content-free evidence for the Privacy Activity claim:
a blocklisted source is skipped, a reason is visible, and the source does not
become retrievable. It does not prove a complete network audit.

## Safety boundary

- Use a disposable, non-sensitive test account and a synthetic test document
  or app window. Do not use passwords, personal messages, customer data, or a
  private browser window.
- Do not commit screenshots, capture databases, raw OCR, search text, or
  video. Record only the template fields below.
- Stop if FNDR shows unexpected content, stores the test source, or reports an
  ambiguous permission state. Record the failure without copying the content.

## 1. Record the build and preflight state

1. Start from the intended Git commit and record its short identifier.
2. Launch the bundled FNDR app, not a browser preview. Browser preview cannot
   prove native TCC behavior.
3. In macOS System Settings, check **Privacy & Security > Screen Recording**.
   Record whether the FNDR bundle is enabled. If it is changed, quit and
   relaunch FNDR before continuing.
4. In **Privacy & Security > Accessibility**, record whether FNDR is enabled.
   Accessibility is not required for the blocklist check itself, but its state
   is part of the native environment and is required by Screen Guide or
   autofill paths.
5. If Screen Recording is denied, stop. The runtime preflight should direct the
   operator to enable FNDR and restart; do not replace this failure with a
   fixture or preview result.

## 2. Prepare a safe blocklist scenario

1. In FNDR Settings, add one clearly synthetic app, title, or domain marker to
   the blocklist, for example `FNDR Privacy Test`. Do not use a real sensitive
   app as the test fixture.
2. Open a disposable window whose app name, title, or domain contains exactly
   that marker and only harmless placeholder text.
3. Let the normal capture interval evaluate the window. Do not force a raw
   capture, inspect stored capture files, or loosen a privacy gate.
4. Open **Privacy Activity** and record the before and after numeric counts
   plus the displayed skip reason. A blocklist counter increment is expected;
   an unchanged counter is a failed or inconclusive run, not a pass.

## 3. Verify absence through product surfaces

1. Use Search to look for the synthetic marker. Record only whether a result
   is absent or present, not any returned text.
2. If Resume Work is available in the tested build, verify the marker is not in
   its cited context. If it is not available, record `not available in build`.
3. Open Memory Vault and verify no memory for the synthetic marker appears.
4. A present result is a failure. Stop, preserve no sensitive content, and file
   the observed stage and content-free reason.

## 4. Record the bounded evidence

Fill in this block after the session. Leave unavailable fields as
`unavailable`, never as a passing result.

```text
Date and operator:
Git commit and app version:
macOS version:
Screen Recording permission: granted | denied | unavailable
Accessibility permission: granted | denied | unavailable
Synthetic blocklist marker used:
Privacy Activity before counts:
Privacy Activity after counts:
Displayed skip reason:
Search result for marker: absent | present | unavailable
Resume Work context for marker: absent | present | not available in build
Memory Vault result for marker: absent | present | unavailable
Outcome: pass | fail | inconclusive
Failure stage or limitation:
Stored artifacts: none in git
```

## Claim boundary

A passing session supports only this claim: FNDR visibly skipped the synthetic
blocklisted scenario during the tested app session, and that marker was absent
from the tested retrieval surfaces. It does not establish that all sensitive
content is blocked, that all network egress is audited, or that permissions
will persist across app updates.

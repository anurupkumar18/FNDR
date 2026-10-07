# VS-35 evidence: `fndr.remember` write-back spec and injected-note corpus

Date: 2026-10-04. Branch `claude/train-f-new`, based on `b948dfd`.
Kind: spike (docs and a test fixture only; no Rust or TypeScript changed).

## Delivered

- `docs/product/fndr-remember-spec.md`: verified current behavior with file and
  line citations, the proposed tool contract, provenance mapping onto existing
  `MemoryRecord` columns, auth and write gate, size and rate limits, secret and
  blocklist handling, embedding and isolation rules, what the person sees,
  out-of-scope list, threat table, and 39 named tests mapped to corpus attacks
  and invariants.
- `src-tauri/tests/fixtures/agent_notes/injected-notes.json`: 30 synthetic
  cases (10 benign, 20 attacks across all 8 attack kinds), plus the fixture
  (settings, limits, two seed memories), harness rules (auth, transport, burst
  semantics), and the vocabulary of attacks, invariants, and error codes. The
  file is ASCII; invisible and accented characters use JSON `\u` escapes.
  `git check-ignore` confirms it is tracked: the matching rule is the
  allow-list `!src-tauri/tests/fixtures/**/*.json` (`.gitignore:27`), and
  `git check-ignore -q` exits 1.

## Decisions made in the spec, and why

1. **Secrets are refused with an error, not redacted.** Capture skips a frame
   whose text matches a secret pattern (`capture/mod.rs:651-679`, `:2924-2941`);
   it never redacts, and `redact_mode` is reported but applied nowhere
   (`config.rs:705-706`, `mcp/mod.rs:1357`). The shared detector returns one
   decision, not spans (`safety_gate.rs:20-155`), so redaction would need a new
   detector and could leave parts of a secret behind while looking safe.
   Unlike capture, the caller can fix the input, so FNDR says why it refused,
   without echoing the text.
2. **Only the detector's text rules apply to notes.** The window-title rules
   (sign in, banking, private browsing) describe a screen, not a sentence; using
   them would refuse "Login redirect bug" (corpus `an-003`).
3. **Injection-looking notes are stored as inert, labeled data, not filtered.**
   The defense is structural: no code path reads note text as a command, notes
   feed no model that writes memory, and every read is labeled. A keyword
   filter would be bypassed and would block honest notes (`an-006`). Invisible
   characters are the exception and are refused (`an-013`), because the person
   must be able to see everything stored.
4. **Agent notes are leaf records.** No `sync_memory_record`, no merge, no
   memory review, no project context, ledger, task, graph, Resume, or Daily
   Brief before Beta. This closes the laundering path where a claim loses its
   provenance inside a derived artifact.
5. **Client name comes from the MCP session (`clientInfo.name`), not from an
   argument.** A prompt-injected model controls arguments but not its client's
   handshake. The token is shared (`mcp/token.rs`), so the name is still
   self-reported and the UI says so; names starting with "FNDR" become
   "Unknown client".
6. **Fixed `app_name = "Agent note"`.** The client name as app name would be
   inferred as `coding` by MCP reads (`mcp/mod.rs:4889-4909`) and could collide
   with the captured Claude app; an "FNDR..." prefix would be dropped from
   agent packs (`agent/context.rs:376-386`).
7. **Own write gate instead of `MCP_SIDE_EFFECT_TOOLS`.** Listing the tool
   there makes every call answer "needs approval ... not run"
   (`mcp/mod.rs:2094-2098`). The gate is: token, no writes with auth disabled,
   kill switch, then a "Let assistants add notes" setting.
8. **No LanceDB migration.** Every provenance field maps to an existing column;
   full provenance goes in `raw_evidence.agent_provenance`, like the ADR-010
   embedding manifest.
9. **Limits: 4,000 characters; 10 per minute and 200 per day per client; 30
   per minute and 500 per day across clients.** 4,000 characters is about what
   one row's primary and support vectors cover; the global caps stop name
   rotation (`an-030`).

## Open questions for the owner

| # | Question | Recommended default |
| --- | --- | --- |
| 1 | Should "Let assistants add notes" start on or off? | Off. Turning it on widens what memory holds, so it is the person's deliberate step (ADR-022's privacy-direction rule). |
| 2 | Should each note need a one-tap confirm on the Mac? | No. The setting, badge, Vault filter, delete, and limits are enough for a labeled, deletable leaf record; per-note confirms would train people to approve blindly. |
| 3 | What happens to `fndr_remember_decision`, which writes today with no limits, secret check, or session provenance? | Put it behind the same gate now; retire it once `fndr.remember` with `kind: decision` ships, so there is one write path. |
| 4 | Should agent notes appear in Resume and the Daily Brief? | Not before Beta. Search, Ask, Vault, and MCP reads only. |
| 5 | Should Ask rank agent notes below captured evidence? | No ranking change; label and attribute instead. Revisit with `make qa-retrieval` once real notes exist. |
| 6 | Per-client tokens so the client name is verified? | After Beta. |
| 7 | Extend the shared secret detector (cloud access key ids, private key blocks, JWTs)? | Yes, in `safety_gate.rs` as its own ticket, so capture benefits too. |
| 8 | Fix per-item auth for JSON-RPC batches now, separately from this feature? | Yes, first. Today a loopback batch is judged by its first method (`mcp/mod.rs:953-959`, `:1052-1066`), which affects every tool, not only writes. |
| 9 | Accept substring blocklist matching on note text (an entry "nda" also matches "agenda")? | Yes for Beta: it is the rule the retroactive cleanup already uses. Consider word-boundary matching for both later. |
| 10 | Confirm the numbers: 4,000 characters, 10 per minute, 200 per day, 500 per day globally. | As written. |

## Related findings outside this ticket

These came up while verifying the spec. None is fixed here.

- MCP batch auth (open question 8). A follow-up task was suggested.
- `agent.privacy_status` reports `redaction_enabled` from `redact_mode`, which
  nothing applies (`config.rs:705-706`, `mcp/mod.rs:1357`, `:3324`).
- The Companion manual-note comment says a retried insert "silently no-ops"
  (`companion/handlers/memories.rs:68-72`), but insert dedup only compares
  records within one batch (`normalize_embed_migrate.rs:1408-1428`). The same
  path stores default (zero) vectors and its app name "FNDR Mobile (...)" is
  dropped from agent packs by `is_internal_app`.
- MCP JSON rows infer `source_type` from URL and app name instead of reporting
  the stored value (`mcp/mod.rs:4646`, `:4687`).

## How to validate the corpus

Required keys and unique ids (run from the repository root):

```bash
python3 -c "import json,collections as C;d=json.load(open('src-tauri/tests/fixtures/agent_notes/injected-notes.json'));cs=d['cases'];R={'id','kind','text','client','attack','expected'};E={'accepted','stored_source_type','must_not_change'};A={'none','instruction_injection','policy_override','tool_call_injection','false_fact','impersonation','oversize','secret','flood_burst'};ids=[c['id'] for c in cs];assert len(ids)==len(set(ids)),'duplicate ids';bad=[c['id'] for c in cs if not R<=c.keys() or not E<=c['expected'].keys() or c['attack'] not in A or (not c['expected']['accepted'] and 'error' not in c['expected'])];assert not bad,bad;print(len(cs),'cases; ids unique; required keys present; accepted',sum(c['expected']['accepted'] for c in cs),'refused',sum(not c['expected']['accepted'] for c in cs));print(dict(C.Counter(c['attack'] for c in cs)))"
```

Output:

```text
30 cases; ids unique; required keys present; accepted 20 refused 10
{'none': 10, 'instruction_injection': 4, 'policy_override': 3, 'tool_call_injection': 3, 'false_fact': 2, 'impersonation': 3, 'oversize': 1, 'secret': 2, 'flood_burst': 2}
```

A second, throwaway check (not committed) mirrored the text rules of
`safety_gate::evaluate` (`SECRET_PATTERNS` and `contains_prefixed_secret` with
the 12-character suffix) in Python and confirmed that exactly `an-027` and
`an-028` match the detector, that only `an-013` contains refused characters,
and that the boundary texts are 4,000 characters (4,000 and 4,552 UTF-8 bytes)
and 4,001 characters. The Rust test
`remember_secret_check_matches_capture_detector` is the authoritative version
of that check.

GitHub's secret scanning API could not be run on the placeholder lines
("Repository does not have GitHub Advanced Security enabled"). The
placeholders (`FAKE_TOKEN_DO_NOT_USE_1234`, `sk-FAKE_DO_NOT_USE_00000000`)
avoid every provider key format on purpose: no `ghp_`, `xoxb-`, AWS key id, or
private key block appears in the corpus.

Dash check on the three files: `LC_ALL=C.UTF-8 grep -nP '[\x{2013}\x{2014}]'`
printed nothing.

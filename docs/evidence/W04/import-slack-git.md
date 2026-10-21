# W04: Slack export and local Git import evidence

## Scope

`SlackSource` reads a chosen Slack export directory or zip, groups messages by channel and day, keeps replies with their thread, resolves known users and channels, and returns one capped `ImportItem` per channel-day. It includes attachment file names without reading their contents. Private channels and direct messages are marked in `people`. The connector reads only JSON entries and skips hidden paths, `node_modules`, `target`, `Library`, and `secrets`.

`GitSource` discovers repositories under chosen roots, or under the home directory by default, to depth four. It skips hidden directories, `node_modules`, `Library`, `.Trash`, `target`, and symlinks. It asks the local `git` CLI for the global and repository `user.email` values, commits by those authors within `window_days`, file names and line counts, branch state, and the last 20 checkout events. It never reads diff contents. GitHub and GitLab origin remotes produce commit page links; other repositories use a local path and hash.

`scan_gh` is separate from local Git scanning. It returns `Disabled` until explicitly enabled, invokes only `gh search prs` and `gh search issues` with fixed arguments, and returns command labels for egress logging, including commands already run when a later command fails. Missing and unauthenticated CLI states have typed errors. FNDR does not read or pass a token.

## Checks

- TDD red phase: the two initial focused tests failed at `todo!()` in the connector stubs.
- Focused Rust unit tests: six passed in the first implementation run. Synthetic fixtures covered directory and compressed zip exports, Unicode, deleted user fallback, message groups and bot filtering, large text, corrupt metadata, temporary Git commits by two authors, repository skips and a symlink loop, and a fake `gh` executable.
- Expanded focused Rust unit tests: 10 passed. Added direct messages, unreadable exports, empty Git repositories, global author config, branch checkout history, and remote commit URLs. The first expanded run failed three tests because a selected hidden temporary root was skipped; the root rule was corrected and the rerun passed.
- `rustfmt --edition 2021 --check` on changed Rust modules passed.
- No real Slack export, repository outside test fixtures, or authenticated `gh` account was scanned. The real `git` executable was used against synthetic temporary repositories. `gh` was tested with a fake executable on `PATH`.

## Integration notes

The temporary `contract_stub.rs` mirrors the draft import item, cursor, source trait, and scan batch. It supplies placeholder `Permission` and `ImportError` types because the draft names them without definitions. Workstream A should replace the stub, switch each connector import to the real contract, and register `SlackSource` and `GitSource` with the runner. The optional `scan_gh` call belongs behind a separate explicit toggle and egress log.

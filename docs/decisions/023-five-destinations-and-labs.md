# ADR 023: Five destinations and Labs

## Status

Accepted — 2026-10-01

## Context

The mounted shell currently exposes ten panels under Memory, Reflect, and
Assist. The UI-UX program's D-01 defines the primary information architecture;
D-05 keeps the graph as a Vault detail, D-06 defers merging reflection panels,
and D-07 restricts developer-facing tools to Developer Mode. The visible
sidebar and command palette should describe the product's everyday jobs rather
than every implementation surface.

## Decision

FNDR's primary navigation has five destinations:

1. **Home** — resume context and begin a recall.
2. **Search & Ask** — search saved memories and ask a grounded question.
3. **Memory Vault** — browse, inspect, and reopen memories; the graph is an
   optional insight inside this destination.
4. **Daily Brief** — daily reflection and follow-ups. Daily Summary, To-dos,
   Stats, and Wrapped remain separate panels for now, but are grouped here
   until D-06 supplies measured evidence for a merge.
5. **Trust & Settings** — capture, permissions, privacy activity, models,
   profile, and appearance.

Everything outside these jobs moves to a clearly labeled **Labs** group rather
than appearing as primary navigation. Labs contains Screen Guide, Hermes Agent,
and Engine diagnostics. Engine diagnostics is developer-facing and is shown
only in Developer Mode; it remains compiled and reachable for the people who
need it. No mounted panel is deleted by this decision.

The command palette mirrors these primary destinations and offers Labs entries
only when Labs is visible. Historical, unmounted keys stay guarded and gain no
new visible trigger.

## Consequences

- PX-01 changes `SIDEBAR_GROUPS`, `MOUNTED_PANEL_KEYS`, and command-palette
  labels to implement this decision, with tests for each visible destination.
- D-05 and D-06 retain their scope: this decision neither promotes the graph
  nor prematurely merges reflection data or panels.
- Product copy calls Labs experimental and never presents developer diagnostics
  as a normal recall workflow.

## Verification

- Sidebar and palette tests enumerate the five primary destinations and Labs.
- Browser/native QA reaches each visible destination and verifies that a Labs
  entry has a real destination or a clear unavailable state.

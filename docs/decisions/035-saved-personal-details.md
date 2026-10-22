# 035. Smart Fill keeps the person's labelled details as facts

Status: Proposed (2026-10-22). Requested by Kunj. Not yet reviewed by the team.

## Context

A saved memory holds a short summary and keyword hints; the page text is dropped when the record is compacted (`memory_compaction.rs`). A summary of a page of personal details says "a document of personal details" and states none of them. Smart Fill on a whole form (ADR 034) found nine notes for a form that asks for a phone number and a passport number, and ChatGPT correctly returned no values, because no note stated one.

## Decision

While Smart Fill is on, the capture pipeline hands each captured page to `profile_facts::observe`, after the privacy gate has let it through and before its text is compacted away. Lines that state a detail with a label ("Phone: ...", "Date of birth: ...", "Current employer: ...") become facts, by fixed rules and not by a model, so a fact is always text that was on screen and a value that does not look like its label is dropped. Facts live in one file, `profile_facts.json`, in the data folder, readable only by the owner, each with the app, the window title and the time it was seen.

- **Whose details.** A labelled line can belong to someone else (an email signature, a company's contact page). A fact is used only when the page says it is the person's own ("my details", "about me", "my profile") or the same value was seen in three different windows.
- **Latest wins.** Of several values of one kind, the most recently seen usable one is used.
- **Opt-in.** Nothing is read unless Smart Fill is switched on, and Settings says what is kept and where, shows how many details are saved, and has a "Forget saved details" button.
- **Use.** On a form, the saved details are the first note sent to ChatGPT (with up to 23 memory notes, no screenshot). A field is filled only from a value a note states.

## Options not taken

- Keep the raw text of every capture. Rejected: it reverses the compaction policy for all memory, and a page of text holds far more than the details a form asks for.
- Have the on-device model extract facts. Rejected by AGENTS.md: the 2B model extracts and does not synthesize, and a claim is kept only with evidence from the capture.
- Read the person's details from the operating system's contact card. Rejected: it covers few of the fields job applications ask for and ties FNDR to one account.

## Consequences

- Identifiers the person chose to show on screen (a passport number, a phone number) are kept in a local file until forgotten. Turning Smart Fill off stops new reading; it does not delete what is saved, so Settings keeps the forget button.
- The rules read `Label: value` lines. A resume laid out as prose, or a form that shows a value without a label, yields nothing; keeping a resume's own text as a document fact is a follow-up.
- Sensitive demographic questions, consent and signature boxes are never filled (follow-up in the form writer).

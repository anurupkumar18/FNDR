# 035. Smart Fill keeps the person's own details verbatim and fills any form from them

Status: Proposed (2026-10-22). Requested by Kunj. Not yet reviewed by the team.

## Context

A saved memory holds a short summary and keyword hints; the page text is dropped when the record is compacted (`memory_compaction.rs`). A summary of a page of personal details says "a document of personal details" and states none of them. Smart Fill on a whole form (ADR 034) found nine notes for a form that asks for a phone number and a passport number, and ChatGPT correctly returned no values, because no note stated one.

A first version of the fix listed the details a form might ask for (name, phone, employer, resume) and parsed each by type. That fails for the next form nobody planned for, so it was replaced.

## Decision

Nothing in Smart Fill knows what kind of form it is looking at.

- **Details.** While Smart Fill is on, capture hands each page to `profile_facts::observe`, after the privacy gate and before compaction. Every `Label: value` line is kept as the page wrote it, with its app, window and time. There is no list of labels. The model decides which saved label answers a field, by meaning ("Employer" answers "Current company").
- **Documents.** A page that is plainly the person's own is kept as an excerpt of its text (at most 3, 3,000 characters each).
- **Whose page.** A page is the person's own when it says so ("my details", "about me") or carries the name of this Mac's account in its opening lines. Lines from any other page are kept only after the same line was seen in three different windows; until then only a hash of it is remembered, never the text. An email signature does not become the person's phone number.
- **Fields.** Any text box, dropdown, date box, radio button or checkbox the window exposes through Accessibility. A radio group's question comes from its group title. Dropdowns showing a placeholder count as empty. A checkbox is pressed only when the page says whether it is ticked, because a browser reports nothing for it and Space would untick a ticked one; radio buttons are always safe.
- **File boxes.** Smart Fill cannot attach a file. The model is shown what each box asks for and the names of the person's recent document-like files (Documents, Desktop, Downloads, names only, nothing opened) and names the best fit, which Smart Fill shows as "Attach: ...". No document types are listed in code.
- **Never filled, whatever memory says.** Consent, certification, terms and signature fields, passwords and one time codes, card and account numbers, government ID numbers, and the voluntary demographic questions of a job application (race, ethnicity, veteran, disability, sexual orientation, religion, marital status). This is a privacy policy, not a form type, and it is enforced in code as well as in the prompt.
- **Opt-in and visible.** Nothing is read unless Smart Fill is on. Settings says what is kept and sent, shows the count, and has "Forget saved details".

## Options not taken

- Keep the raw text of every capture. Rejected here: it reverses the compaction policy for all memory (a separate change is proposed in ADR 036).
- Extract facts with the on-device model. Rejected by AGENTS.md: the 2B model extracts and does not synthesize, and a claim is kept only with evidence from the capture.
- A fixed list of detail kinds with a parser for each. Tried and replaced: it is a hard-coded form type.
- Read the person's details from the operating system's contact card. Rejected: it covers few fields and ties FNDR to one account.

## Consequences

- Identifiers the person chose to show on screen are kept in a local file (owner read only) until forgotten. Turning Smart Fill off stops new reading but does not delete what is saved.
- The rules read `Label: value` lines. A page that states a value without a label yields no detail, though a document the person owns is kept as text and the model can read it.
- File names (not contents) leave the Mac when a form has a file box.
- The never-filled list is a deterministic backstop. A new kind of sensitive field is not covered until someone adds it.

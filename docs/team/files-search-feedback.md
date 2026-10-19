# Files search: testing and feedback

Files search is Kunj's rank 3 feature ([ADR 033](../decisions/033-files-search-from-digup.md)). Only Kunj writes its code. Everyone else is asked to use it hard and say what they find: what breaks, what is slow, what reads badly, what is confusing, what you expected instead.

## How to give feedback

- Add an entry at the bottom of this file in a merge request that touches only this file, or open an issue with the label `area::files-search`, or comment on Kunj's merge request.
- One finding per entry. Say what you did, what happened, what you expected, your Mac (chip, memory, macOS) and how many files you indexed.
- For speed, give numbers: seconds to the first result, time to index a folder against the estimate onboarding showed.
- Never attach your own files, screenshots of their contents, or the index (`files/index.sqlite`, `files/code.sqlite`). Describe the file instead ("a 40-page scanned PDF").
- Do not send a patch, even a small one. Describe the fix if you see one; Kunj makes it.

## What to try

- Onboarding: the "Search your files" step, folder choice, the time estimate per folder, the model download, skipping it.
- Setup Center: adding and skipping folders, skipped file types, code folders and the editor, indexing hours, pause, forget.
- Quick Find (Alt+Space) and the notch: file rows under memory rows, the `file:` and `code:` prefixes, Enter (open), Shift+Enter (Finder), Cmd+Y (preview), Cmd+C (copy).
- Search and Ask: the Files section.
- Open at the place: a PDF opens at the matched page, a recording or video at the matched moment, code at the line in your editor.
- Kinds: documents, PDFs (scanned too), photos, screenshots by their text, audio, video, code, other languages.
- Live index: drop a new file in a chosen folder and time how long until it is found.
- Load: a large folder while you work; does FNDR or the Mac feel slower.

## Entries

Template:

```
### <date>, <name>: <one-line summary>
Area: onboarding | settings | quick find | notch | search and ask | open | indexing | speed | results
Did: <steps>
Got: <what happened>
Expected: <what you expected>
Mac: <chip, memory, macOS>; indexed: <about how many files>
```

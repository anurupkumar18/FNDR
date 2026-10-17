---
name: readout-docx
description: Render a one-page readout (.docx, with an optional bar chart) from a JSON spec. Use for any readout, report, brief or memo the task asks for as a document.
---
# readout-docx

1. Do the analysis with the Python named under TOOLS in the task (pandas is there). Compute numbers from the data; never copy them from notes.
2. Write `work/readout-spec.json`:

```json
{
  "title": "Q4 onboarding readout",
  "subtitle": "One line: scope and data source",
  "summary": "Two or three sentences with the answer first.",
  "sections": [{"heading": "...", "paragraphs": ["..."], "bullets": ["..."]}],
  "table": {"columns": ["Driver", "Churned", "Share"], "rows": [["...", "24", "31%"]]},
  "chart": {"title": "...", "labels": ["..."], "values": [24, 21], "value_label": "Churned respondents"},
  "sources_note": "Data: inputs/<file>; notes: memory ids"
}
```

Every key except `title` and `summary` is optional. Keep it to one page: at most 3 sections, 6 table rows, 6 chart bars.

3. Render with that Python: `<python> .agents/skills/readout-docx/render.py work/readout-spec.json out/<name>.docx`. Set `MPLCONFIGDIR=work/matplotlib` first. A chart also writes `out/<name>-chart.png`.
4. Check the document by reading it back with python-docx before you answer.

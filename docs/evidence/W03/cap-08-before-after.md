# CAP-08 dedupe comparison: img_hash (before) vs dHash + A-B-A (after)

| Sequence | Expected keep | img_hash keeps | dHash+ABA keeps |
|---|---|---|---|
| idle | 1 | 1 | 1 |
| cursor_blink | 1 | 1 | 1 |
| clock_tick | 1 | 1 | 1 |
| scroll | 10 | 10 | 10 |
| tab_flicker | 2 | 2 | 2 |
| typing | 10 | 1 | 10 |

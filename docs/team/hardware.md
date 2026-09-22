# Hardware

| Person | Chip | RAM | macOS | Can fine-tune a 1B to 2B model (16 GB or more)? |
|---|---|---|---|---|
| Anurup | Apple M1 | 8 GB | 26.6.2 | Tight |

Add one row per person once the team runs the checkpoint script themselves:

```bash
sysctl -n machdep.cpu.brand_string; echo "$(( $(sysctl -n hw.memsize) / 1073741824 )) GB"; sw_vers -productVersion
```

Anurup's machine is the bench machine: every number in `docs/evidence/` for
baselines and bake-offs comes from it, so they stay comparable across weeks.
The reference profile is the M1 8 GB (`FNDR_MODEL_PROFILE`).

At 8 GB, memory pressure is a real constraint during normal work, not just
under model load: two agents each running `cargo build`/`npm run tauri dev`
in parallel, or one agent's Python venv install (Laya's isolated venv alone
installed at roughly 900 MB), can push free memory into the low tens of MB.
See `docs/team/agent-switch.md`'s "Running truly in parallel" section for
what this means when two agents are active on this machine at once.

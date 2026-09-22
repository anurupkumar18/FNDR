#!/usr/bin/env python3
"""Checkpoint 6: how sampler settings change FNDR-style JSON extraction. Synthetic OCR only.
Usage: sampling_lab.py [runs_per_setting]   (default 3)"""
import hashlib, json, os, subprocess, sys, tempfile

RUNS = int(sys.argv[1]) if len(sys.argv) > 1 else 3
# The app's live text model is Qwen3-VL-2B (models.rs:181). Override with FNDR_LAB_MODEL and FNDR_LAB_TEMPLATE=llama3.
MODEL = os.path.expanduser(os.environ.get(
    "FNDR_LAB_MODEL", "~/Library/Application Support/com.fndr.app/models/Qwen3VL-2B-Instruct-Q4_K_M.gguf"))
TEMPLATE = os.environ.get("FNDR_LAB_TEMPLATE", "chatml")
SYS = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "sysmsg.txt")).read()
OCR = """cargo test --lib capture::tests
running 12 tests
test capture::tests::skip_reason_counts ... ok
test capture::tests::dedup_hash_stable ... ok
test capture::tests::flush_ms_recorded ... FAILED

failures:
---- capture::tests::flush_ms_recorded stdout ----
thread 'capture::tests::flush_ms_recorded' panicked at src/capture/mod.rs:2210:9:
assertion `left == right` failed: expected flush_ms samples 3, got 2

test result: FAILED. 11 passed; 1 failed; 0 ignored"""
USER = f'APP: Terminal\nWINDOW: fndr - zsh\nOCR TEXT:\n"""\n{OCR}\n"""\n\nReturn JSON only.'
if TEMPLATE == "llama3":
    PROMPT = ("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n" + SYS +
              "<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n" + USER +
              "<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n")
else:
    PROMPT = ("<|im_start|>system\n" + SYS + "<|im_end|>\n<|im_start|>user\n" + USER +
              "<|im_end|>\n<|im_start|>assistant\n")

SETTINGS = [
    ("S1 greedy + repeat 1.1 (FNDR text path)", ["--temp", "0", "--repeat-penalty", "1.1", "--repeat-last-n", "64"]),
    ("S2 greedy, no penalty",                    ["--temp", "0", "--repeat-penalty", "1.0"]),
    ("S3 temp 0.7 top-p 0.9 (like vlm.rs)",      ["--temp", "0.7", "--top-p", "0.9", "--top-k", "40", "--repeat-penalty", "1.0"]),
    ("S4 hot: temp 1.5, no truncation",           ["--temp", "1.5", "--top-p", "1.0", "--top-k", "0", "--repeat-penalty", "1.0"]),
]

def run_once(extra):
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write(PROMPT); path = f.name
    cmd = ["llama-completion", "-m", MODEL, "-f", path, "-n", os.environ.get("FNDR_LAB_MAXTOK", "400"), "-c", "2048", "--no-warmup",
           "-no-cnv", "--no-display-prompt"] + extra
    out = subprocess.run(cmd, capture_output=True, text=True).stdout
    os.unlink(path)
    return out.replace("[end of text]", "").strip()

def valid_json(text):
    try:
        i, j = text.index("{"), text.rindex("}")
        json.loads(text[i:j + 1]); return True
    except Exception:
        return False

print(f"{'setting':44} {'runs':>4} {'distinct':>8} {'valid JSON':>10} {'avg chars':>9}")
for label, extra in SETTINGS:
    outs = [run_once(extra) for _ in range(RUNS)]
    distinct = len({hashlib.sha1(o.encode()).hexdigest() for o in outs})
    ok = sum(valid_json(o) for o in outs)
    avg = sum(len(o) for o in outs) // max(1, len(outs))
    print(f"{label:44} {RUNS:>4} {distinct:>8} {ok:>10} {avg:>9}")

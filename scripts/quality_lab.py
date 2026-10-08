#!/usr/bin/env python3
"""Run FNDR's synthetic quality corpus in the native app's isolated profile."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
from datetime import datetime
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SUITES = ("knowledge-worker", "office-pm", "software-engineer")


def profile_for(home: Path, suite: str) -> Path:
    if suite not in SUITES:
        raise ValueError(f"unknown synthetic suite {suite!r}; choose from {', '.join(SUITES)}")
    return home / "Library" / "Application Support" / "com.fndr.app.quality-lab" / suite


def assert_isolated(candidate: Path, owner_profile: Path) -> None:
    candidate = candidate.expanduser().resolve()
    owner_profile = owner_profile.expanduser().resolve()
    if candidate == owner_profile or candidate in owner_profile.parents or owner_profile in candidate.parents:
        raise ValueError(f"refusing to use the FNDR owner profile or a parent/child path: {candidate}")


def paths(suite: str) -> tuple[Path, Path]:
    profile = profile_for(Path.home(), suite)
    owner = Path.home() / "Library" / "Application Support" / "com.fndr.app"
    assert_isolated(profile, owner)
    return profile, owner


def run(command: list[str], *, env: dict[str, str] | None = None) -> None:
    subprocess.run(command, cwd=ROOT, env=env, check=True)


def assert_no_running_fndr() -> None:
    app = subprocess.run(["pgrep", "-x", "fndr"], capture_output=True, text=True)
    if app.returncode == 0:
        raise ValueError("FNDR is already running; quit it normally before scoring or starting another profile")
    dev_server = subprocess.run(
        ["lsof", "-tiTCP:1420", "-sTCP:LISTEN"], capture_output=True, text=True
    )
    if dev_server.returncode == 0:
        raise ValueError("a dev server is already listening on FNDR's port 1420; stop it before starting or scoring")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fingerprint_paths() -> list[Path]:
    candidates = list((ROOT / "src-tauri" / "src").rglob("*.rs"))
    candidates += list((ROOT / "src-tauri" / "examples").rglob("*.rs"))
    candidates += list((ROOT / "src-tauri" / "tests").rglob("*.rs"))
    for pattern in ("*.ts", "*.tsx", "*.css"):
        candidates += list((ROOT / "src").rglob(pattern))
    candidates += list((ROOT / "scripts" / "audit").rglob("*.py"))
    candidates += list((ROOT / "scripts" / "demo").rglob("*.py"))
    candidates += list((ROOT / "scripts" / "demo").rglob("*.sh"))
    candidates += [
        ROOT / "scripts" / "quality_lab.py",
        ROOT / "scripts" / "test_quality_lab.py",
        ROOT / "Makefile",
        ROOT / "package.json",
        ROOT / "package-lock.json",
        ROOT / "src-tauri" / "Cargo.toml",
        ROOT / "src-tauri" / "Cargo.lock",
        ROOT / "src-tauri" / "tauri.conf.json",
    ]
    return sorted({path for path in candidates if path.is_file()})


def source_fingerprint() -> tuple[str, int]:
    digest = hashlib.sha256()
    included = 0
    for path in fingerprint_paths():
        digest.update(path.relative_to(ROOT).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
        included += 1
    return digest.hexdigest(), included


def profile_snapshot(profile: Path) -> dict[str, object]:
    """Fingerprint lab data files without reading shared model assets."""
    root = profile.expanduser().resolve()
    digest = hashlib.sha256()
    file_count = 0
    total_bytes = 0
    shared_asset_links: list[str] = []
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        top_level = relative.split("/", 1)[0]
        if path.is_symlink():
            if top_level in {"models", "speech_models"}:
                shared_asset_links.append(relative)
            digest.update(relative.encode())
            digest.update(b"\0shared-link\0")
            continue
        if not path.is_file() or top_level in {"models", "speech_models"}:
            continue
        digest.update(relative.encode())
        digest.update(b"\0")
        with path.open("rb") as file:
            for chunk in iter(lambda: file.read(1024 * 1024), b""):
                digest.update(chunk)
                total_bytes += len(chunk)
        digest.update(b"\0")
        file_count += 1
    return {
        "sha256": digest.hexdigest(),
        "file_count": file_count,
        "bytes": total_bytes,
        "shared_asset_links": shared_asset_links,
    }


def parse_capture_fixture_metrics(log: str, fixture_rows: list[dict[str, object]]) -> dict[str, object]:
    """Extract structured measurements from the deterministic Rust fixture lane."""
    ocr_pattern = re.compile(
        r"^(?P<id>[\w-]+)\s+class=(?P<class>\S+)\s+cer=(?P<cer>[0-9.]+)\s+budget=(?P<budget>[0-9.]+)$",
        re.MULTILINE,
    )
    ocr_cases = [
        {
            "fixture_id": match["id"],
            "app_class": match["class"],
            "cer": float(match["cer"]),
            "budget": float(match["budget"]),
            "within_budget": float(match["cer"]) <= float(match["budget"]),
        }
        for match in ocr_pattern.finditer(log)
    ]
    expected_store_ids = {
        str(row["id"]) for row in fixture_rows if row.get("expected_outcome") == "store"
    }
    measured_store_ids = {str(row["fixture_id"]) for row in ocr_cases}
    if not ocr_cases or measured_store_ids != expected_store_ids:
        missing = sorted(expected_store_ids - measured_store_ids)
        extra = sorted(measured_store_ids - expected_store_ids)
        raise ValueError(f"missing OCR measurements or unexpected fixture rows (missing={missing}, extra={extra})")
    ocr_worst = max(ocr_cases, key=lambda row: float(row["cer"]))

    dedupe_pattern = re.compile(r"^\|\s*(?P<sequence>[\w-]+)\s*\|\s*(?P<expected>\d+)\s*\|\s*(?P<img_hash>\d+)\s*\|\s*(?P<dhash>\d+)\s*\|$", re.MULTILINE)
    dedupe_sequences = [
        {
            "sequence": match["sequence"],
            "expected_keep": int(match["expected"]),
            "img_hash_kept": int(match["img_hash"]),
            "dhash_aba_kept": int(match["dhash"]),
            "matches_expected": int(match["expected"]) == int(match["dhash"]),
        }
        for match in dedupe_pattern.finditer(log)
    ]
    if not dedupe_sequences:
        raise ValueError("missing frame dedupe sequence measurements")

    admission_marker = "fixture | expected admission | actual admission"
    admission_tail = log.split(admission_marker, maxsplit=1)[-1] if admission_marker in log else ""
    admission_cases = []
    for line in admission_tail.splitlines():
        cells = [cell.strip() for cell in line.split("|")]
        if len(cells) != 3 or not cells[0] or cells[0] == "fixture":
            continue
        admission_cases.append({
            "fixture_id": cells[0],
            "expected": cells[1],
            "actual": cells[2],
            "matches_expected": cells[1] == cells[2],
        })
    expected_admission_ids = {str(row["id"]) for row in fixture_rows}
    measured_admission_ids = {str(row["fixture_id"]) for row in admission_cases}
    if measured_admission_ids != expected_admission_ids:
        missing = sorted(expected_admission_ids - measured_admission_ids)
        extra = sorted(measured_admission_ids - expected_admission_ids)
        raise ValueError(f"missing pre-frame admission measurements (missing={missing}, extra={extra})")

    summary_pattern = re.compile(
        r"test result: (?P<status>ok|FAILED)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; (?P<ignored>\d+) ignored"
    )
    test_summaries = [
        {
            "status": match["status"],
            "passed": int(match["passed"]),
            "failed": int(match["failed"]),
            "ignored": int(match["ignored"]),
        }
        for match in summary_pattern.finditer(log)
    ]
    if not test_summaries:
        raise ValueError("missing Rust test summary lines")
    hasher_test_name = "production_perceptual_hasher_accepts_novel_fixture_and_skips_repeats"
    hasher_test = re.search(rf"test {hasher_test_name} \.\.\. (?P<result>ok|FAILED)", log)
    return {
        "ocr_cleanup": {
            "case_count": len(ocr_cases),
            "within_budget_count": sum(bool(row["within_budget"]) for row in ocr_cases),
            "mean_cer": sum(float(row["cer"]) for row in ocr_cases) / len(ocr_cases),
            "worst_case": ocr_worst,
            "cases": ocr_cases,
            "budget_interpretation": "regression ceilings; passing does not imply low character error",
        },
        "dedupe_sequences": dedupe_sequences,
        "pre_frame_admission": {
            "case_count": len(admission_cases),
            "store_count": sum(row["expected"] == "store" for row in admission_cases),
            "privacy_skip_count": sum(str(row["expected"]).startswith("skip:") for row in admission_cases),
            "mismatch_count": sum(not bool(row["matches_expected"]) for row in admission_cases),
            "all_expected": all(bool(row["matches_expected"]) for row in admission_cases),
            "cases": admission_cases,
        },
        "production_fixture_dedupe": {
            "test_name": hasher_test_name,
            "passed": bool(hasher_test and hasher_test["result"] == "ok"),
        },
        "test_summaries": test_summaries,
    }


def write_run_manifest(
    report_dir: Path,
    lane: str,
    suite: str | None,
    inputs: list[Path],
    exit_code: int,
    profile_states: dict[str, dict[str, object]] | None = None,
    metrics: dict[str, object] | None = None,
) -> None:
    contracts = [
        ROOT / "src-tauri" / "src" / "inference" / "model_config.rs",
        ROOT / "src-tauri" / "src" / "inference" / "prompts.rs",
        ROOT / "docs" / "product" / "llm-task-catalog.md",
    ]
    fingerprint, fingerprint_file_count = source_fingerprint()
    manifest = {
        "schema_version": 1,
        "lane": lane,
        "suite": suite,
        "created_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "working_tree": "dirty" if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip() else "clean",
        "source_fingerprint_sha256": fingerprint,
        "source_fingerprint_file_count": fingerprint_file_count,
        "platform": platform.platform(),
        "python": sys.version.split()[0],
        "input_sha256": {str(path.relative_to(ROOT)): sha256(path) for path in inputs},
        "contract_source_sha256": {
            str(path.relative_to(ROOT)): sha256(path) for path in contracts if path.is_file()
        },
        "profile_states": profile_states,
        "exit_code": exit_code,
    }
    if metrics is not None:
        manifest["metrics"] = metrics
    report_dir.mkdir(parents=True, exist_ok=True)
    (report_dir / "run.json").write_text(json.dumps(manifest, indent=2) + "\n")


def prepare(suite: str, reset: bool) -> None:
    profile, owner = paths(suite)
    if profile.exists() and not reset:
        raise ValueError(f"lab profile already exists at {profile}; use --reset to reseed it")
    corpus = ROOT / "scripts" / "demo" / f"{suite}-week.json"
    env = os.environ.copy()
    env["FNDR_DEMO_DIR"] = str(profile)
    env["FNDR_DEMO_CORPUS"] = str(corpus)
    command = [str(ROOT / "scripts" / "demo" / "seed-demo-profile.sh")]
    if reset:
        command.append("--reset")
    run(command, env=env)

    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    digest = hashlib.sha256(corpus.read_bytes()).hexdigest()
    marker = {
        "schema_version": 1,
        "suite": suite,
        "profile_kind": "synthetic_quality_lab",
        "source_corpus": str(corpus.relative_to(ROOT)),
        "source_sha256": digest,
        "git_head_at_seed": commit,
        "created_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "models": "models and speech_models are symlinked from the existing FNDR owner profile; no model copy; avoid model install/remove here",
        "memory_provenance": "synthetic pre-seeded records; not a capture-pipeline replay",
    }
    (profile / ".fndr-quality-lab.json").write_text(json.dumps(marker, indent=2) + "\n")
    print(f"Quality Lab profile ready: {profile}")
    print("This corpus exercises native Vault/Home/Search/Ask presentation and retrieval.")
    print("Its pre-seeded records do not claim to have passed live capture or extraction.")


def validate_profile(suite: str) -> Path:
    profile, owner = paths(suite)
    marker = profile / ".fndr-quality-lab.json"
    if not marker.is_file():
        raise ValueError(f"no prepared Quality Lab profile at {profile}; run prepare first")
    data = json.loads(marker.read_text())
    if data.get("profile_kind") != "synthetic_quality_lab" or data.get("suite") != suite:
        raise ValueError(f"profile marker does not match suite {suite!r}: {profile}")
    assert_isolated(profile, owner)
    return profile


def score(suite: str) -> None:
    assert_no_running_fndr()
    profile = validate_profile(suite)
    run_id = datetime.now().astimezone().strftime("%Y%m%d-%H%M%S")
    report_dir = ROOT / "src-tauri" / "target" / "quality-lab" / suite / run_id
    profile_before = profile_snapshot(profile)
    env = os.environ.copy()
    env.update({"TZ": "America/Denver", "CARGO_BUILD_JOBS": "1"})
    corpus = ROOT / "scripts" / "demo" / f"{suite}-week.json"
    queries = ROOT / "scripts" / "demo" / f"{suite}-queries.json"
    reference = ROOT / "scripts" / "demo" / "retrieval-reference" / f"{suite}.json"
    command = [
        "make",
        "qa-retrieval-check",
        f"PERSONA={suite}",
        "QA_SKIP_SEED=1",
        f"QA_PROFILE={profile}",
        f"QA_CHECK_DIR={report_dir}",
    ]
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
    (report_dir / "command.log").write_text(result.stdout + result.stderr)
    profile_after = profile_snapshot(profile)
    write_run_manifest(
        report_dir,
        "production_retrieval",
        suite,
        [corpus, queries, reference],
        result.returncode,
        {"before": profile_before, "after": profile_after},
    )
    check_report = report_dir / "check.md"
    if check_report.is_file():
        print(check_report.read_text())
    if result.returncode:
        print(f"Full command output: {report_dir / 'command.log'}", file=sys.stderr)
        raise subprocess.CalledProcessError(result.returncode, command)
    print(f"Quality Lab report: {report_dir / 'check.md'}")


def _read_report_run(report_dir: Path) -> tuple[dict, dict]:
    report_dir = report_dir.expanduser().resolve()
    try:
        run_manifest = json.loads((report_dir / "run.json").read_text())
        metrics = json.loads((report_dir / "current.json").read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"could not read a complete run in {report_dir}: {error}") from error
    if run_manifest.get("lane") != "production_retrieval" or run_manifest.get("exit_code") != 0:
        raise ValueError(f"{report_dir} is not a successful production retrieval run")
    if metrics.get("schema_version") != 2:
        raise ValueError(f"unsupported retrieval report schema in {report_dir}")
    return run_manifest, metrics


def compare_reports(before_dir: Path, after_dir: Path) -> str:
    before_run, before = _read_report_run(Path(before_dir))
    after_run, after = _read_report_run(Path(after_dir))
    if before_run.get("suite") != after_run.get("suite") or before.get("case_set") != after.get("case_set"):
        raise ValueError("comparison requires the same suite and case set")
    if before_run.get("input_sha256") != after_run.get("input_sha256"):
        raise ValueError("comparison inputs differ; use reports with identical corpus, queries, and reference")
    if before.get("case_count") != after.get("case_count"):
        raise ValueError("comparison case counts differ despite matching inputs")

    def query_map(metrics: dict) -> dict[tuple[str, str], dict]:
        result: dict[tuple[str, str], dict] = {}
        for row in metrics.get("queries", []):
            key = (str(row.get("kind", "")), str(row.get("query", "")))
            if not key[1] or key in result:
                raise ValueError("query report has an empty or duplicate (kind, query) key")
            result[key] = row
        return result

    before_queries = query_map(before)
    after_queries = query_map(after)
    if before_queries.keys() != after_queries.keys():
        raise ValueError("comparison query sets differ; use reports with identical labeled cases")

    lines = [
        f"# Quality Lab comparison: {before_run['suite']}",
        "",
        f"- Baseline: `{Path(before_dir).name}` · commit `{before_run.get('git_head', 'unknown')}`",
        f"- Candidate: `{Path(after_dir).name}` · commit `{after_run.get('git_head', 'unknown')}`",
        f"- Cases: {before['case_count']} · identical suite and input hashes verified",
        f"- Source fingerprint: {'unchanged' if before_run.get('source_fingerprint_sha256') == after_run.get('source_fingerprint_sha256') else 'differs; this is expected when evaluating a code change'}",
    ]
    if before_run.get("contract_source_sha256") != after_run.get("contract_source_sha256"):
        lines.append("- Model/prompt contract sources changed; inspect their hashes in both `run.json` files before attributing quality deltas.")
    before_profile = (before_run.get("profile_states") or {}).get("after")
    after_profile = (after_run.get("profile_states") or {}).get("after")
    if before_profile and after_profile:
        state_label = "same" if before_profile.get("sha256") == after_profile.get("sha256") else "differs"
        lines.append(
            f"- Profile state: {state_label} · baseline {before_profile.get('file_count')} files / {before_profile.get('bytes')} bytes; "
            f"candidate {after_profile.get('file_count')} files / {after_profile.get('bytes')} bytes."
        )
        if state_label == "differs":
            lines.append("  Profile contents changed; inspect the saved state hashes before attributing metric deltas to source changes alone.")
    else:
        lines.append("- Profile state: unavailable in at least one run manifest; profile-data reproducibility cannot be confirmed.")
    lines += [
        "",
        "Metrics are reported separately; this comparison does not average away privacy, grounding, or usability failures.",
        "",
        "| Path | Metric | Baseline | Candidate | Delta |",
        "| --- | --- | ---: | ---: | ---: |",
    ]
    metric_specs = (
        ("recall_at_5", "Recall@5", 3),
        ("mrr_at_10", "MRR@10", 3),
        (("latency_ms", "p95"), "p95 latency ms", 0),
        (("no_match", "no_strong_match"), "negatives below strong-match bar", 0),
        (("no_match", "positive_without_strong_match"), "positives below strong-match bar", 0),
    )

    def metric_value(path_metrics: dict, key: str | tuple[str, str]) -> float:
        value = path_metrics
        for part in key if isinstance(key, tuple) else (key,):
            value = value[part]
        return float(value)

    for path_name in ("search", "ask", "retrieve"):
        old_path, new_path = before["paths"][path_name], after["paths"][path_name]
        for key, label, precision in metric_specs:
            old_value = metric_value(old_path, key)
            new_value = metric_value(new_path, key)
            fmt = f"{{:.{precision}f}}"
            lines.append(
                f"| {path_name} | {label} | {fmt.format(old_value)} | {fmt.format(new_value)} | {fmt.format(new_value - old_value,)} |"
            )

    rank_fields = (
        ("search_rank_at_10", "search"),
        ("ask_rank_at_10", "ask"),
        ("retrieve_rank_at_10", "retrieve"),
    )
    changed: list[str] = []
    for kind, query in sorted(before_queries):
        old_row, new_row = before_queries[(kind, query)], after_queries[(kind, query)]
        for field, path_name in rank_fields:
            old_rank, new_rank = old_row.get(field), new_row.get(field)
            if old_rank != new_rank:
                old_label = f"rank {old_rank}" if old_rank is not None else "miss"
                new_label = f"rank {new_rank}" if new_rank is not None else "miss"
                changed.append(f"| {kind} | {query.replace('|', '\\|')} | {path_name} | {old_label} -> {new_label} |")
    lines.extend(["", f"Rank changes: {len(changed)} path/query pairs out of {len(before_queries) * len(rank_fields)}."])
    if changed:
        lines += ["", "| Kind | Query | Path | Baseline -> candidate |", "| --- | --- | --- | --- |", *changed]
    else:
        lines.append("No query ranks changed.")
    lines.append("")
    return "\n".join(lines)


def fixtures() -> None:
    run_id = datetime.now().astimezone().strftime("%Y%m%d-%H%M%S")
    report_dir = ROOT / "src-tauri" / "target" / "quality-lab" / "capture-fixtures" / run_id
    report_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "1"
    logs = []
    exit_code = 0
    commands = (
        ["cargo", "test", "--test", "capture_fixtures", "--", "--nocapture"],
        [
            "cargo",
            "test",
            "--lib",
            "capture::tests::committed_capture_fixtures_match_the_pre_frame_privacy_gate",
            "--",
            "--nocapture",
        ],
    )
    for command in commands:
        result = subprocess.run(
            command,
            cwd=ROOT / "src-tauri",
            env=env,
            capture_output=True,
            text=True,
        )
        logs.append(f"$ {' '.join(command)}\n{result.stdout}{result.stderr}")
        if result.returncode and not exit_code:
            exit_code = result.returncode
    log = "\n\n".join(logs)
    fixture_root = ROOT / "src-tauri" / "tests" / "fixtures" / "screens"
    fixture_manifest = fixture_root / "manifest.json"
    quality_cases = fixture_root / "quality-cases.json"
    fixture_rows = json.loads(fixture_manifest.read_text())
    inputs = [fixture_manifest, quality_cases] + [fixture_root / row["file"] for row in fixture_rows]
    metrics = None
    if not exit_code:
        try:
            metrics = parse_capture_fixture_metrics(log, fixture_rows)
        except ValueError as error:
            exit_code = 1
            log += f"\n\nQuality Lab metrics parse failed: {error}\n"
    (report_dir / "capture-fixtures.log").write_text(log)
    write_run_manifest(report_dir, "capture_privacy_ocr_cleanup_dedupe", None, inputs, exit_code, metrics=metrics)
    print(log, end="")
    print(f"Capture fixture report: {report_dir / 'capture-fixtures.log'}")
    if exit_code:
        raise subprocess.CalledProcessError(exit_code, "capture fixture suite")


def start(suite: str) -> None:
    profile = validate_profile(suite)
    assert_no_running_fndr()
    env = os.environ.copy()
    env["FNDR_DATA_DIR"] = str(profile)
    print(f"Starting this checkout's native Tauri dev build with FNDR_DATA_DIR={profile}")
    print("The app will use the synthetic profile; model files are shared by symlink to avoid a second copy.")
    print("Avoid installing or removing text or speech models from the Lab app; those directories are shared.")
    run(["npm", "run", "tauri", "dev", "--", "--no-watch"], env=env)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "score", "start"):
        command = sub.add_parser(name)
        command.add_argument("--suite", choices=SUITES, default="knowledge-worker")
        if name == "prepare":
            command.add_argument("--reset", action="store_true", help="move only this lab suite to Trash, then reseed")
    compare = sub.add_parser("compare", help="compare two paired retrieval reports without averaging their metrics")
    compare.add_argument("--before", required=True, type=Path, help="baseline run report directory")
    compare.add_argument("--after", required=True, type=Path, help="candidate run report directory")
    sub.add_parser("fixtures", help="run the committed capture image OCR/cleanup fixture suite")
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare(args.suite, args.reset)
        elif args.command == "score":
            score(args.suite)
        elif args.command == "fixtures":
            fixtures()
        elif args.command == "compare":
            print(compare_reports(args.before, args.after), end="")
        else:
            start(args.suite)
    except (OSError, ValueError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"Quality Lab: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

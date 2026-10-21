//! Compare the two text backends on the same synthetic gold fixtures.
//! Run one backend and task per process so model cold start is measurable.

use fndr_lib::inference::foundation;
use fndr_lib::inference::InferenceEngine;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Deserialize)]
struct Capture {
    id: String,
    evidence: String,
    app: String,
    window_title: String,
}

#[derive(Deserialize)]
struct Question {
    id: String,
    question: String,
    relevant: Vec<String>,
}

fn cases<T: for<'de> Deserialize<'de>>(text: &str) -> Vec<T> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("synthetic fixture"))
        .collect()
}

fn percentile(values: &[u128], percentile: f64) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() as f64 * percentile).ceil() as usize).saturating_sub(1);
    sorted[index]
}

async fn run_case(
    engine: &InferenceEngine,
    task: &str,
    capture: &Capture,
    question: Option<&Question>,
) -> (bool, bool) {
    match task {
        "extraction" => {
            let draft = engine
                .extract_structured_memory(&capture.app, &capture.window_title, &capture.evidence)
                .await;
            let evidence_pass = draft.as_ref().is_some_and(|draft| {
                !draft.memory_context.trim().is_empty()
                    && draft
                        .source_evidence
                        .as_ref()
                        .is_some_and(|evidence| evidence.issues.is_empty())
            });
            (draft.is_some(), evidence_pass)
        }
        "snippet" => {
            let passed = !engine
                .summarize_memory_node(&capture.app, &capture.window_title, &capture.evidence)
                .await
                .is_empty();
            (passed, passed)
        }
        "answer" => {
            let passed = !engine
                .answer(
                    &question.expect("answer question").question,
                    &format!("Memory {}: {}", capture.id, capture.evidence),
                )
                .await
                .is_empty();
            (passed, passed)
        }
        _ => panic!("unknown task"),
    }
}

fn raw_validity(trace_path: &Path, task: &str) -> bool {
    let task_id = match task {
        "extraction" => "memory_extraction",
        "snippet" => "memory_snippet",
        "answer" => "answer",
        _ => unreachable!(),
    };
    fs::read_to_string(trace_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|trace| trace["task"] == task_id)
        .last()
        .and_then(|trace| trace["output"].as_str().map(str::to_string))
        .is_some_and(|raw| {
            if task == "extraction" {
                serde_json::from_str::<serde_json::Value>(&raw).is_ok_and(|value| value.is_object())
            } else {
                !raw.trim().is_empty()
            }
        })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || !matches!(args[1].as_str(), "apple" | "qwen") {
        return Err(
            "usage: foundation_benchmark apple|qwen extraction|snippet|answer [qwen model path]"
                .into(),
        );
    }
    let backend = args[1].as_str();
    let task = args[2].as_str();
    if !matches!(task, "extraction" | "snippet" | "answer") {
        return Err("unknown task".into());
    }
    let app_dir =
        std::env::temp_dir().join(format!("fndr-foundation-benchmark-{}", std::process::id()));
    fs::create_dir_all(&app_dir)?;
    if backend == "qwen" {
        let model = PathBuf::from(args.get(3).ok_or("Qwen model path required")?);
        if !model.is_file() {
            return Err("Qwen model path is not a file".into());
        }
        let models = app_dir.join("models");
        fs::create_dir_all(&models)?;
        std::os::unix::fs::symlink(model, models.join("Qwen3VL-2B-Instruct-Q4_K_M.gguf"))?;
        foundation::write_setting(&app_dir, false)?;
    }
    std::env::set_var("FNDR_TRACE_CONTENT", "1");

    let captures: Vec<Capture> = cases(include_str!("../tests/fixtures/gold/v0/extraction.jsonl"));
    let questions: Vec<Question> = cases(include_str!("../tests/fixtures/gold/v0/questions.jsonl"));
    let ids: &[&str] = match task {
        "extraction" | "snippet" => &["g-003", "g-006", "g-044"],
        "answer" => &["g-003", "g-009", "g-014"],
        _ => unreachable!(),
    };
    let question_ids: &[&str] = &["q-002", "q-005", "q-008"];
    let first = captures
        .iter()
        .find(|case| case.id == ids[0])
        .expect("first case");
    let first_question = if task == "answer" {
        Some(
            questions
                .iter()
                .find(|question| question.id == question_ids[0])
                .expect("first question"),
        )
    } else {
        None
    };

    let cold_started = Instant::now();
    let engine =
        InferenceEngine::new(Some(app_dir.clone()), Some("qwen3-vl-2b".to_string())).await?;
    let _ = run_case(&engine, task, first, first_question).await;
    let cold_ms = cold_started.elapsed().as_millis();
    let trace_path = app_dir.join("llm_traces.jsonl");
    let mut latencies = Vec::new();
    let mut raw_valid = 0;
    let mut method_pass = 0;
    let mut evidence_clean = 0;
    for (index, id) in ids.iter().enumerate() {
        let capture = captures.iter().find(|case| &case.id == id).expect("case");
        let question = if task == "answer" {
            Some(
                questions
                    .iter()
                    .find(|question| question.id == question_ids[index])
                    .expect("question"),
            )
        } else {
            None
        };
        if let Some(question) = question {
            assert!(question.relevant.contains(&capture.id));
        }
        let started = Instant::now();
        let (method, clean) = run_case(&engine, task, capture, question).await;
        method_pass += usize::from(method);
        evidence_clean += usize::from(clean);
        latencies.push(started.elapsed().as_millis());
        raw_valid += usize::from(raw_validity(&trace_path, task));
    }
    let result = serde_json::json!({
        "backend": backend,
        "task": task,
        "samples": ids.len(),
        "raw_valid": raw_valid,
        "method_pass": method_pass,
        "validator_pass": method_pass,
        "evidence_clean": evidence_clean,
        "cold_ms": cold_ms,
        "p50_ms": percentile(&latencies, 0.50),
        "p95_ms": percentile(&latencies, 0.95),
        "latencies_ms": latencies,
    });
    println!("{}", serde_json::to_string(&result)?);
    fs::remove_dir_all(app_dir)?;
    Ok(())
}

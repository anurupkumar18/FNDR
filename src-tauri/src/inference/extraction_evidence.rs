//! Source statements are observations, never user authorization or pending tasks.

use super::StructuredMemoryExtraction;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const MAX_STATEMENTS: usize = 6;
const MAX_QUOTE_CHARS: usize = 1024;
const MAX_ISSUES: usize = 16;
const MAX_SNAPSHOTS: usize = 4;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SourceReferences {
    #[serde(default)]
    pub intent: Vec<Value>,
    #[serde(default)]
    pub actions: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStatement {
    pub kind: String,
    pub line: u32,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionEvidence {
    pub version: u8,
    pub source_sha256: String,
    pub statements: Vec<SourceStatement>,
    pub issues: Vec<String>,
}

pub fn extraction_source_text(input: &str) -> String {
    input.chars().take(4000).collect()
}

pub fn numbered_source_text(input: &str) -> String {
    extraction_source_text(input)
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{}: {}", i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn source_hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn quote_at(lines: &[&str], line: usize, incomplete_last_line: bool) -> Option<String> {
    if line == 0 || line > lines.len() || lines[line - 1].trim().is_empty() {
        return None;
    }
    if incomplete_last_line && line + 1 >= lines.len() {
        return None;
    }
    // Keep adjacent lines rather than stripping speaker, completion, negation
    // or qualification. The selected line is one-based; quotes are never cut.
    Some(lines[line.saturating_sub(2)..(line + 1).min(lines.len())].join("\n"))
}

fn clear_unverified_actions(draft: &mut StructuredMemoryExtraction, issues: &mut Vec<String>) {
    for (field, had_value) in [
        ("intent", !draft.user_intent.trim().is_empty()),
        ("next_steps", !draft.next_steps.is_empty()),
        ("todos", !draft.todos.is_empty()),
    ] {
        if had_value {
            issues.push(format!("unverified_{field}_removed"));
        }
    }
    draft.user_intent.clear();
    draft.next_steps.clear();
    draft.todos.clear();
}

fn remove_novel_numeric_narrative(
    draft: &mut StructuredMemoryExtraction,
    source: &str,
    issues: &mut Vec<String>,
) {
    static NUMBERS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let numbers = NUMBERS
        .get_or_init(|| regex::Regex::new(r"\b[0-9]+(?:[.,][0-9]+)*\b").expect("numeric literals"));
    let known: HashSet<_> = numbers.find_iter(source).map(|m| m.as_str()).collect();
    if numbers
        .find_iter(&draft.memory_context)
        .any(|m| !known.contains(m.as_str()))
    {
        draft.memory_context.clear();
        issues.push("memory_context_novel_number".into());
    }
}

/// Resolve model references against the exact truncated source supplied in the
/// prompt. A selected statement says what appeared, not who owns it or whether
/// it is a pending obligation. Generated action wording never becomes a task.
pub fn finalize_extraction(draft: &mut StructuredMemoryExtraction, input: &str) {
    let source = extraction_source_text(input);
    let lines: Vec<_> = source.lines().collect();
    let incomplete_last_line = input.chars().nth(4000).is_some() && !source.ends_with('\n');
    let refs = std::mem::take(&mut draft.source_refs);
    let mut issues = Vec::new();
    let mut statements = Vec::new();
    for (kind, values, limit) in [("intent", refs.intent, 2), ("action", refs.actions, 4)] {
        let mut invalid = 0;
        if values.len() > limit {
            issues.push(format!("{kind}_source_refs_truncated"));
        }
        for value in values.into_iter().take(limit) {
            let line = value
                .as_u64()
                .or_else(|| value.as_str()?.parse::<u64>().ok())
                .and_then(|n| usize::try_from(n).ok())
                .or_else(|| {
                    let selected = value.as_str()?;
                    let mut matches = lines.iter().enumerate().filter(|(i, line)| {
                        **line == selected || format!("{}: {}", i + 1, line) == selected
                    });
                    let first = matches.next()?.0 + 1;
                    // Only an exact, unambiguous selection is a reference.
                    // No fuzzy match, substring search or generated wording.
                    matches.next().is_none().then_some(first)
                });
            let Some((line, quote)) =
                line.and_then(|n| quote_at(&lines, n, incomplete_last_line).map(|q| (n, q)))
            else {
                invalid += 1;
                continue;
            };
            if quote.chars().count() > MAX_QUOTE_CHARS {
                issues.push("source_quote_too_long".into());
                continue;
            }
            let statement = SourceStatement {
                kind: kind.into(),
                line: line as u32,
                quote,
            };
            if !statements.contains(&statement) {
                statements.push(statement);
            }
        }
        if invalid > 0 {
            issues.push(format!("invalid_{kind}_source_refs:{invalid}"));
        }
    }
    clear_unverified_actions(draft, &mut issues);
    remove_novel_numeric_narrative(draft, &source, &mut issues);
    issues.truncate(MAX_ISSUES);
    draft.source_evidence = Some(ExtractionEvidence {
        version: 1,
        source_sha256: source_hash(&source),
        statements,
        issues,
    });
}

/// Recheck host evidence after capture fusion. Evidence points into its original
/// model input, not into subsequently cleaned or concatenated record text.
pub fn validate_source_evidence(
    draft: &mut StructuredMemoryExtraction,
    input: &str,
) -> Vec<String> {
    let Some(mut evidence) = draft.source_evidence.take() else {
        return Vec::new();
    };
    let source = extraction_source_text(input);
    let lines: Vec<_> = source.lines().collect();
    let incomplete_last_line = input.chars().nth(4000).is_some() && !source.ends_with('\n');
    let mut issues = evidence.issues.clone();
    if evidence.version != 1 || evidence.source_sha256 != source_hash(&source) {
        evidence.statements.clear();
        issues.push("source_evidence_hash_mismatch".into());
    } else {
        evidence.statements.retain(|statement| {
            let valid = quote_at(&lines, statement.line as usize, incomplete_last_line).as_deref()
                == Some(statement.quote.as_str())
                && matches!(statement.kind.as_str(), "intent" | "action")
                && statement.quote.chars().count() <= MAX_QUOTE_CHARS;
            if !valid {
                issues.push("source_quote_mismatch".into());
            }
            valid
        });
        evidence.statements.truncate(MAX_STATEMENTS);
    }
    clear_unverified_actions(draft, &mut issues);
    remove_novel_numeric_narrative(draft, &source, &mut issues);
    let mut seen = HashSet::new();
    issues.retain(|issue| seen.insert(issue.clone()));
    issues.truncate(MAX_ISSUES);
    evidence.issues = issues.clone();
    draft.source_evidence = Some(evidence);
    issues
}

fn parse_evidence(value: &Value) -> Option<ExtractionEvidence> {
    let evidence: ExtractionEvidence = serde_json::from_value(value.clone()).ok()?;
    if evidence.version != 1
        || evidence.source_sha256.len() != 64
        || !evidence
            .source_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || evidence.statements.len() > MAX_STATEMENTS
        || evidence.issues.len() > MAX_ISSUES
        || evidence.statements.iter().any(|s| {
            s.line == 0
                || !matches!(s.kind.as_str(), "intent" | "action")
                || s.quote.trim().is_empty()
                || s.quote.chars().count() > MAX_QUOTE_CHARS
        })
    {
        return None;
    }
    Some(evidence)
}

/// Presence is deliberately separate from successful parsing: an unsupported
/// contract must not fall back to promoting stale generated fields as tasks.
pub fn has_source_evidence(raw: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    value.get("source_evidence").is_some_and(Value::is_object)
        || value
            .get("source_evidence_history")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
}

/// Current snapshot first, then newest history; retain original snapshot hashes
/// across merges. A later empty/rejected snapshot supersedes an older same-hash
/// snapshot. This validates shape, not the truth or current status of a quote.
pub fn source_evidence_sets_from_raw(raw: &str) -> Vec<ExtractionEvidence> {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let history = value
        .get("source_evidence_history")
        .and_then(Value::as_array);
    let mut seen = HashSet::new();
    value
        .get("source_evidence")
        .into_iter()
        .chain(history.into_iter().flatten().take(MAX_SNAPSHOTS - 1))
        // Reserve even an unsupported current snapshot before parsing so its
        // older same-hash history cannot silently replace it.
        .filter(|value| {
            value
                .get("source_sha256")
                .and_then(Value::as_str)
                .map_or(true, |hash| seen.insert(hash.to_string()))
        })
        .filter_map(parse_evidence)
        .take(MAX_SNAPSHOTS)
        .collect()
}

pub fn render_source_statements(evidence: &ExtractionEvidence) -> String {
    if evidence.statements.is_empty() {
        return String::new();
    }
    let statements = evidence
        .statements
        .iter()
        .map(|s| format!("Source {} at L{}: {}", s.kind, s.line, s.quote))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Source statements (speaker ownership and pending status unverified):\n{statements}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_refs_reject_invented_actions_and_intent_at_any_confidence() {
        for confidence in [0.42, 0.95] {
            let mut draft = StructuredMemoryExtraction {
                user_intent: "User wants to open GitLab".into(),
                next_steps: vec!["Open GitLab".into()],
                todos: vec!["Review inbox messages".into()],
                confidence,
                ..Default::default()
            };
            finalize_extraction(
                &mut draft,
                "Search or enter website name\nFrequently visited\nGitLab Gmail Calendar",
            );
            assert!(draft.user_intent.is_empty());
            assert!(draft.next_steps.is_empty() && draft.todos.is_empty());
            assert!(draft.source_evidence.unwrap().statements.is_empty());
        }
    }

    #[test]
    fn source_refs_preserve_explicit_actions_speaker_and_completion_context() {
        let source = "Jo 10:00\nI will write the test tomorrow.\nAlex 10:01\nDo not reopen the finished review.";
        let mut draft = StructuredMemoryExtraction::default();
        draft.source_refs.actions = vec![json!(2), json!(4)];
        finalize_extraction(&mut draft, source);
        let evidence = draft.source_evidence.unwrap();
        assert_eq!(evidence.statements.len(), 2);
        assert!(evidence.statements[0]
            .quote
            .contains("Jo 10:00\nI will write the test tomorrow."));
        assert!(evidence.statements[1]
            .quote
            .contains("Do not reopen the finished review."));
        assert!(draft.next_steps.is_empty() && draft.todos.is_empty());
        let rendered = render_source_statements(&evidence);
        assert!(rendered.contains("Source statements") && rendered.contains("status unverified"));
    }

    #[test]
    fn source_refs_reject_invalid_references_and_model_forged_provenance() {
        let mut draft: StructuredMemoryExtraction = serde_json::from_value(json!({
            "source_refs":{"intent":[0,99,"L1",-1],"actions":[1,1]},
            "source_evidence":{"version":1,"source_sha256":"forged","statements":[],"issues":[]}
        }))
        .unwrap();
        assert!(draft.source_evidence.is_none());
        finalize_extraction(&mut draft, "A source statement.");
        let evidence = draft.source_evidence.unwrap();
        assert_eq!(evidence.source_sha256.len(), 64);
        assert_eq!(evidence.statements.len(), 1);
        assert!(evidence
            .issues
            .iter()
            .any(|v| v.starts_with("invalid_intent_source_refs")));
    }

    #[test]
    fn source_refs_reject_stale_evidence_independently_of_confidence() {
        let mut draft = StructuredMemoryExtraction::default();
        draft.source_refs.intent = vec![json!(1)];
        finalize_extraction(&mut draft, "I intend to write tests.");
        draft.confidence = 0.99;
        let issues = validate_source_evidence(&mut draft, "Unrelated changed source.");
        assert!(issues.iter().any(|v| v == "source_evidence_hash_mismatch"));
        assert!(draft.source_evidence.unwrap().statements.is_empty());
    }

    #[test]
    fn source_refs_reject_novel_numbers_without_claiming_general_entailment() {
        let cases: Vec<(String, String, String, String)> =
            serde_json::from_str(include_str!("../../tests/fixtures/extraction_cases.json"))
                .unwrap();
        let source = &cases.iter().find(|c| c.0 == "spreadsheet").unwrap().3;
        let mut draft = StructuredMemoryExtraction {
            memory_context: "The table contains 20 rows.".into(),
            ..Default::default()
        };
        finalize_extraction(&mut draft, source);
        assert!(draft.memory_context.is_empty());
        assert!(draft
            .source_evidence
            .as_ref()
            .unwrap()
            .issues
            .contains(&"memory_context_novel_number".to_string()));
        draft.memory_context = "The table declares 40 rows.".into();
        finalize_extraction(&mut draft, "Rows: 40");
        assert_eq!(draft.memory_context, "The table declares 40 rows.");
    }

    #[test]
    fn source_refs_resolve_only_unique_verbatim_source_lines() {
        let source = "Sam\nPlease review the draft after approval.\nMira\nThe earlier review is complete; do not reopen it.";
        let mut draft = StructuredMemoryExtraction::default();
        draft.source_refs.actions = vec![
            json!("2: Please review the draft after approval."),
            json!("The earlier review is complete; do not reopen it."),
            json!("Review the draft now"),
            json!("3: Please review the draft after approval."),
        ];
        finalize_extraction(&mut draft, source);
        let evidence = draft.source_evidence.unwrap();
        assert_eq!(
            evidence
                .statements
                .iter()
                .map(|s| s.line)
                .collect::<Vec<_>>(),
            [2, 4]
        );
        assert!(evidence.statements[0].quote.contains("after approval"));
        assert!(evidence.statements[1].quote.contains("do not reopen"));
        assert!(evidence
            .issues
            .contains(&"invalid_action_source_refs:2".into()));
    }

    #[test]
    fn source_refs_reject_ambiguous_verbatim_lines_but_accept_exact_numbered_line() {
        let mut draft = StructuredMemoryExtraction::default();
        draft.source_refs.actions = vec![
            json!("Review after approval."),
            json!("3: Review after approval."),
        ];
        finalize_extraction(
            &mut draft,
            "Review after approval.\nSam\nReview after approval.",
        );
        let evidence = draft.source_evidence.unwrap();
        assert_eq!(evidence.statements.len(), 1);
        assert_eq!(evidence.statements[0].line, 3);
        assert!(evidence
            .issues
            .contains(&"invalid_action_source_refs:1".into()));

        let mut ambiguous = StructuredMemoryExtraction::default();
        ambiguous.source_refs.actions = vec![json!("2: Review after approval.")];
        finalize_extraction(
            &mut ambiguous,
            "Sam\nReview after approval.\n2: Review after approval.",
        );
        assert!(ambiguous.source_evidence.unwrap().statements.is_empty());
    }

    #[test]
    fn source_refs_reject_cutoff_line_and_its_neighboring_quote() {
        let source = format!(
            "{}\nSpeaker\nPlease publish this draft{} only after approval.",
            "x\n".repeat(1950),
            " ".repeat(200)
        );
        let lines: Vec<_> = extraction_source_text(&source)
            .lines()
            .map(str::to_string)
            .collect();
        assert!(lines.last().unwrap().starts_with("Please publish"));
        assert!(!lines.last().unwrap().contains("only after approval"));
        let mut draft = StructuredMemoryExtraction::default();
        draft.source_refs.actions = vec![json!(lines.len()), json!(lines.len() - 1)];
        finalize_extraction(&mut draft, &source);
        assert!(
            draft.source_evidence.unwrap().statements.is_empty(),
            "cutoff must not remove the qualifier from a quoted statement"
        );
    }

    #[test]
    fn source_refs_unsupported_current_snapshot_suppresses_same_hash_history() {
        let hash = "a".repeat(64);
        let raw = json!({
            "source_evidence": {"version":99, "source_sha256":hash},
            "source_evidence_history": [{"version":1, "source_sha256":hash,
                "statements":[{"kind":"action","line":1,"quote":"Outdated action"}], "issues":[]}]
        })
        .to_string();
        assert!(has_source_evidence(&raw));
        assert!(
            source_evidence_sets_from_raw(&raw).is_empty(),
            "unsupported current snapshot must not resurrect same-hash history"
        );
    }

    #[test]
    fn source_refs_bound_quotes_and_never_truncate_away_qualifiers() {
        let source = format!("{} Do not run this.", "x".repeat(1500));
        let mut draft = StructuredMemoryExtraction::default();
        draft.source_refs.actions = vec![json!(1)];
        finalize_extraction(&mut draft, &source);
        let evidence = draft.source_evidence.unwrap();
        assert!(evidence.statements.is_empty());
        assert!(evidence.issues.contains(&"source_quote_too_long".into()));
        assert_eq!(
            extraction_source_text(&"é".repeat(4001)).chars().count(),
            4000
        );
    }
}

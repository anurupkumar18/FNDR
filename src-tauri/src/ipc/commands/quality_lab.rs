//! Debug-only replay of committed synthetic image fixtures in the Quality Lab.
//! The command shares FNDR's existing photo-import pipeline and refuses to
//! write unless the active data directory is a marked Quality Lab profile.

use super::glasses_import::import_meta_glasses_photo_at_path;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

const SUITES: &[&str] = &["knowledge-worker", "office-pm", "software-engineer"];

#[derive(Debug, Clone, Deserialize)]
struct FixtureManifestRow {
    id: String,
    file: String,
    app_class: String,
    expected_outcome: String,
    expected_text: String,
    cer_budget: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QualityLabEvaluationCase {
    pub fixture_id: String,
    pub required_facts: Vec<String>,
    pub exact_query: String,
    pub paraphrase_query: String,
    pub grounded_question: String,
    pub unsupported_question: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EvaluationManifest {
    schema_version: u32,
    cases: Vec<QualityLabEvaluationCase>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualityLabFixture {
    pub id: String,
    pub app_class: String,
    pub file_name: String,
    pub expected_text: String,
    pub cer_budget: f64,
    pub evaluation: Option<QualityLabEvaluationCase>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualityLabFixtureImport {
    pub fixture_id: String,
    pub memory_id: String,
}

fn validate_marker(profile: &Path, quality_lab_root: &Path) -> Result<String, String> {
    let root = quality_lab_root
        .canonicalize()
        .map_err(|e| format!("Quality Lab profile root is unavailable: {e}"))?;
    let profile = profile
        .canonicalize()
        .map_err(|e| format!("active app data directory is unavailable: {e}"))?;
    if profile.parent() != Some(root.as_path()) {
        return Err(
            "Synthetic fixture replay is allowed only in a direct Quality Lab suite profile".into(),
        );
    }
    let suite = profile
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Quality Lab profile has no suite name".to_string())?;
    if !SUITES.contains(&suite) {
        return Err(format!("unsupported Quality Lab suite {suite:?}"));
    }
    let marker_path = profile.join(".fndr-quality-lab.json");
    let marker_real_path = marker_path.canonicalize().map_err(|_| {
        "active profile has no Quality Lab marker; refusing fixture import".to_string()
    })?;
    if !marker_real_path.starts_with(&profile) {
        return Err("Quality Lab marker resolves outside the active profile".into());
    }
    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(marker_real_path).map_err(|e| format!("read Quality Lab marker: {e}"))?,
    )
    .map_err(|e| format!("invalid Quality Lab marker: {e}"))?;
    if marker.get("profile_kind").and_then(|v| v.as_str()) != Some("synthetic_quality_lab")
        || marker.get("suite").and_then(|v| v.as_str()) != Some(suite)
    {
        return Err("Quality Lab marker does not match the active suite profile".into());
    }
    Ok(suite.to_string())
}

fn fixture_root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens")
        .canonicalize()
        .map_err(|e| format!("synthetic screen fixture corpus is unavailable: {e}"))
}

fn store_fixtures(root: &Path) -> Result<Vec<FixtureManifestRow>, String> {
    let manifest_path = root.join("manifest.json");
    let rows: Vec<FixtureManifestRow> = serde_json::from_slice(
        &std::fs::read(&manifest_path).map_err(|e| format!("read fixture manifest: {e}"))?,
    )
    .map_err(|e| format!("invalid fixture manifest: {e}"))?;
    let mut output = Vec::new();
    for row in rows
        .into_iter()
        .filter(|row| row.expected_outcome == "store")
    {
        let candidate = root.join(&row.file);
        let real_path = candidate
            .canonicalize()
            .map_err(|e| format!("fixture {} is unavailable: {e}", row.id))?;
        if !real_path.starts_with(root) || !real_path.is_file() {
            return Err(format!(
                "fixture {} resolves outside the committed corpus",
                row.id
            ));
        }
        if !matches!(
            real_path.extension().and_then(|v| v.to_str()),
            Some("png" | "jpg" | "jpeg")
        ) {
            return Err(format!(
                "fixture {} has an unsupported image extension",
                row.id
            ));
        }
        output.push(row);
    }
    Ok(output)
}

fn fixture_evaluations(
    root: &Path,
    store_fixture_ids: &HashSet<String>,
) -> Result<HashMap<String, QualityLabEvaluationCase>, String> {
    let manifest_path = root.join("quality-cases.json");
    let manifest: EvaluationManifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|e| format!("read fixture evaluation manifest: {e}"))?,
    )
    .map_err(|e| format!("invalid fixture evaluation manifest: {e}"))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported fixture evaluation schema {}",
            manifest.schema_version
        ));
    }
    let mut cases = HashMap::new();
    for case in manifest.cases {
        if !store_fixture_ids.contains(&case.fixture_id) {
            return Err(format!(
                "evaluation case references unknown or non-store fixture {}",
                case.fixture_id
            ));
        }
        if case.required_facts.is_empty()
            || case
                .required_facts
                .iter()
                .any(|fact| fact.trim().is_empty())
            || [
                &case.exact_query,
                &case.paraphrase_query,
                &case.grounded_question,
                &case.unsupported_question,
            ]
            .iter()
            .any(|query| query.trim().is_empty())
        {
            return Err(format!(
                "evaluation case {} is missing gold facts or queries",
                case.fixture_id
            ));
        }
        let id = case.fixture_id.clone();
        if cases.insert(id.clone(), case).is_some() {
            return Err(format!("duplicate fixture evaluation case {id}"));
        }
    }
    Ok(cases)
}

fn app_quality_lab_root() -> Result<PathBuf, String> {
    let home =
        dirs::home_dir().ok_or_else(|| "could not resolve the macOS home directory".to_string())?;
    Ok(home.join("Library/Application Support/com.fndr.app.quality-lab"))
}

fn validated_profile(state: &AppState) -> Result<String, String> {
    validate_marker(&state.app_data_dir, &app_quality_lab_root()?)
}

#[tauri::command]
pub async fn get_quality_lab_fixtures(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<QualityLabFixture>, String> {
    validated_profile(state.inner().as_ref())?;
    let root = fixture_root()?;
    let rows = store_fixtures(&root)?;
    let store_fixture_ids = rows
        .iter()
        .map(|row| row.id.clone())
        .collect::<HashSet<_>>();
    let evaluations = fixture_evaluations(&root, &store_fixture_ids)?;
    Ok(rows
        .into_iter()
        .map(|row| QualityLabFixture {
            evaluation: evaluations.get(&row.id).cloned(),
            id: row.id,
            app_class: row.app_class,
            file_name: row.file,
            expected_text: row.expected_text,
            cer_budget: row.cer_budget,
        })
        .collect())
}

#[tauri::command]
pub async fn replay_quality_lab_fixture(
    fixture_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<QualityLabFixtureImport, String> {
    validated_profile(state.inner().as_ref())?;
    let root = fixture_root()?;
    let row = store_fixtures(&root)?
        .into_iter()
        .find(|row| row.id == fixture_id)
        .ok_or_else(|| "fixture is unknown or excluded from storage replay".to_string())?;
    let path = root
        .join(&row.file)
        .canonicalize()
        .map_err(|e| format!("resolve fixture path: {e}"))?;
    if !path.starts_with(&root) {
        return Err("fixture resolves outside the committed corpus".into());
    }
    let memory_id =
        import_meta_glasses_photo_at_path(state.inner().clone(), path, Some(&row.id)).await?;
    Ok(QualityLabFixtureImport {
        fixture_id: row.id,
        memory_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn accepts_only_marked_direct_suite_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("com.fndr.app.quality-lab");
        let profile = root.join("knowledge-worker");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join(".fndr-quality-lab.json"),
            r#"{"profile_kind":"synthetic_quality_lab","suite":"knowledge-worker"}"#,
        )
        .unwrap();
        assert_eq!(
            validate_marker(&profile, &root).unwrap(),
            "knowledge-worker"
        );
        assert!(validate_marker(&temp.path().join("com.fndr.app"), &root).is_err());
        assert!(validate_marker(&profile.join("nested"), &root).is_err());
    }

    #[test]
    fn fixture_replay_excludes_privacy_negative_examples() {
        let root = fixture_root().unwrap();
        let rows = store_fixtures(&root).unwrap();
        assert!(!rows.iter().any(|row| row.app_class == "privacy_negative"));
        assert!(rows.iter().all(|row| row.expected_outcome == "store"));
    }

    #[test]
    fn fixture_evaluations_match_only_store_fixtures_and_have_four_gold_queries() {
        let root = fixture_root().unwrap();
        let ids = store_fixtures(&root)
            .unwrap()
            .into_iter()
            .map(|row| row.id)
            .collect::<HashSet<_>>();
        let evaluations = fixture_evaluations(&root, &ids).unwrap();
        assert_eq!(evaluations.len(), 3);
        assert!(evaluations.values().all(|case| {
            !case.exact_query.is_empty()
                && !case.paraphrase_query.is_empty()
                && !case.grounded_question.is_empty()
                && !case.unsupported_question.is_empty()
                && !case.required_facts.is_empty()
        }));
    }

    #[test]
    fn rejects_profile_marker_for_another_suite() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("lab");
        let profile = root.join("knowledge-worker");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join(".fndr-quality-lab.json"),
            r#"{"profile_kind":"synthetic_quality_lab","suite":"office-pm"}"#,
        )
        .unwrap();
        assert!(validate_marker(&profile, &root).is_err());
    }

    #[test]
    fn rejects_fixture_paths_that_escape_the_committed_corpus() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("screens");
        fs::create_dir_all(&root).unwrap();
        fs::write(temp.path().join("outside.png"), b"synthetic image bytes").unwrap();
        fs::write(
            root.join("manifest.json"),
            r#"[{"id":"escape","file":"../outside.png","app_class":"editor","expected_outcome":"store","expected_text":"expected","cer_budget":0.1}]"#,
        )
        .unwrap();
        assert!(store_fixtures(&root).is_err());
    }
}

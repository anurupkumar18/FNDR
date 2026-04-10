use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct ModelDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub size_bytes: u64,
    pub size_label: &'static str,
    pub quality_label: &'static str,
    pub speed_label: &'static str,
    pub ram_gb: f32,
    pub recommended: bool,
    pub filename: &'static str,
    pub download_url: &'static str,
}

pub const MODEL_CATALOG: [ModelDefinition; 1] = [ModelDefinition {
    id: "gemma-4-e4b",
    name: "Gemma 4 · E4B",
    description: "Required local model for memory summaries, Q&A, and screen understanding.",
    size_bytes: 2_500_000_000,
    size_label: "2.5 GB",
    quality_label: "Best",
    speed_label: "Balanced",
    ram_gb: 6.0,
    recommended: true,
    filename: "gemma-4-E4B-it-Q4_K_M.gguf",
    download_url:
        "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf",
}];

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub definition: &'static ModelDefinition,
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct StoredOnboardingState {
    model_id: Option<String>,
}

pub fn catalog() -> &'static [ModelDefinition] {
    &MODEL_CATALOG
}

pub fn model_by_id(model_id: &str) -> Option<&'static ModelDefinition> {
    MODEL_CATALOG.iter().find(|model| model.id == model_id)
}

pub fn models_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models")
}

pub fn partial_model_path(app_data_dir: &Path, filename: &str) -> PathBuf {
    models_dir(app_data_dir).join(format!("{filename}.partial"))
}

pub fn preferred_model_id_from_onboarding(app_data_dir: &Path) -> Option<String> {
    let onboarding_path = app_data_dir.join("onboarding.json");
    let raw = std::fs::read_to_string(onboarding_path).ok()?;
    serde_json::from_str::<StoredOnboardingState>(&raw)
        .ok()?
        .model_id
}

pub fn candidate_model_dirs(app_data_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(data_dir) = app_data_dir {
        dirs.push(models_dir(data_dir));
    }

    dirs.push(PathBuf::from("models"));
    dirs.push(PathBuf::from("src-tauri/models"));

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            dirs.push(parent.join("models"));
            dirs.push(parent.join("../Resources/models"));
        }
    }

    if let Some(data_dir) = dirs::data_dir() {
        dirs.push(data_dir.join("fndr/models"));
    }

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|dir| seen.insert(dir.clone()))
        .collect()
}

pub fn is_model_available(model_id: &str, app_data_dir: Option<&Path>) -> bool {
    resolve_specific_model(model_id, app_data_dir).is_some()
}

pub fn resolve_model(
    preferred_model_id: Option<&str>,
    app_data_dir: Option<&Path>,
) -> Option<ResolvedModel> {
    let mut ordered_models: Vec<&'static ModelDefinition> = Vec::new();

    if let Some(model_id) = preferred_model_id {
        if let Some(model) = model_by_id(model_id) {
            ordered_models.push(model);
        }
    }

    for model in MODEL_CATALOG.iter() {
        if ordered_models
            .iter()
            .all(|candidate| candidate.id != model.id)
        {
            ordered_models.push(model);
        }
    }

    let candidate_dirs = candidate_model_dirs(app_data_dir);
    for model in ordered_models {
        for dir in &candidate_dirs {
            let path = dir.join(model.filename);
            if path.exists() {
                return Some(ResolvedModel {
                    definition: model,
                    path,
                });
            }
        }
    }

    None
}

fn resolve_specific_model(model_id: &str, app_data_dir: Option<&Path>) -> Option<ResolvedModel> {
    let definition = model_by_id(model_id)?;

    candidate_model_dirs(app_data_dir)
        .into_iter()
        .map(|dir| dir.join(definition.filename))
        .find(|path| path.exists())
        .map(|path| ResolvedModel { definition, path })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("fndr-model-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resolve_model_prefers_app_data_dir() {
        let temp_dir = make_temp_dir();
        let model_dir = models_dir(&temp_dir);
        std::fs::create_dir_all(&model_dir).unwrap();
        let expected_path = model_dir.join("gemma-4-E4B-it-Q4_K_M.gguf");
        std::fs::write(&expected_path, b"test").unwrap();

        let resolved = resolve_model(Some("gemma-4-e4b"), Some(temp_dir.as_path())).unwrap();

        assert_eq!(resolved.definition.id, "gemma-4-e4b");
        assert_eq!(resolved.path, expected_path);

        std::fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn partial_file_does_not_count_as_downloaded() {
        let temp_dir = make_temp_dir();
        let model_dir = models_dir(&temp_dir);
        std::fs::create_dir_all(&model_dir).unwrap();
        let partial_path = partial_model_path(&temp_dir, "gemma-4-E4B-it-Q4_K_M.gguf");
        std::fs::write(&partial_path, b"partial").unwrap();

        let resolved = resolve_model(Some("gemma-4-e4b"), Some(temp_dir.as_path()));
        assert_ne!(
            resolved.as_ref().map(|model| model.path.clone()),
            Some(partial_path)
        );

        std::fs::remove_dir_all(temp_dir).unwrap();
    }
}

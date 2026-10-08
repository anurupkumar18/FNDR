//! Scores task suggestion on the labeled screens in
//! `tests/fixtures/task_screens.json`: for each screen, how many tasks a
//! person would write down. Runs three ways: the model with the screen
//! check, the model-free finder alone, and both together. Prints per-screen
//! results and a table. All screens are synthetic.
//! Usage: cargo run --example task_suggestion_eval [-- --no-model]

use fndr_lib::inference::InferenceEngine;
use fndr_lib::tasks::suggest::{
    find_stated_tasks, is_task_source, parse_suggestions, suggestions_for, surface_of,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Screen {
    name: String,
    app: String,
    url: Option<String>,
    tasks: usize,
    text: String,
}

#[derive(Default)]
struct Score {
    screens_right: usize,
    false_screens: usize,
    missed_screens: usize,
    kept: usize,
    expected: usize,
    matched: usize,
}

impl Score {
    fn add(&mut self, expected: usize, kept: usize) {
        self.screens_right += usize::from((expected > 0) == (kept > 0));
        self.false_screens += usize::from(expected == 0 && kept > 0);
        self.missed_screens += usize::from(expected > 0 && kept == 0);
        self.kept += kept;
        self.expected += expected;
        self.matched += kept.min(expected);
    }
    fn line(&self, name: &str, screens: usize) -> String {
        let pct = |a: usize, b: usize| {
            if b == 0 {
                "n/a".to_string()
            } else {
                format!("{:.0}%", 100.0 * a as f64 / b as f64)
            }
        };
        format!(
            "{name:22} screens right {}/{screens}  false {}  missed {}  suggestions {}  precision {}  recall {}",
            self.screens_right, self.false_screens, self.missed_screens, self.kept,
            pct(self.matched, self.kept), pct(self.matched, self.expected),
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let screens: Vec<Screen> =
        serde_json::from_str(include_str!("../tests/fixtures/task_screens.json"))?;
    let no_model = std::env::args().any(|arg| arg == "--no-model");
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let engine = if no_model {
            None
        } else {
            let app_data_dir = dirs::data_dir().ok_or("no data dir")?.join("com.fndr.app");
            Some(
                InferenceEngine::new(Some(app_data_dir), None)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        };
        let (mut model_score, mut finder_score, mut both_score) =
            (Score::default(), Score::default(), Score::default());
        for screen in &screens {
            let surface = surface_of(&screen.app, screen.url.as_deref());
            let asked = is_task_source(&screen.app, screen.url.as_deref());
            let raw = match (&engine, asked) {
                (Some(engine), true) => Some(engine.suggest_tasks(&screen.text).await),
                _ => None,
            };
            let model = raw
                .as_deref()
                .map(|raw| parse_suggestions(raw, &screen.text, surface))
                .unwrap_or_default();
            let found = if asked {
                find_stated_tasks(&screen.text, surface)
            } else {
                Vec::new()
            };
            let both = if asked {
                suggestions_for(raw.as_deref().unwrap_or("NONE"), &screen.text, surface)
            } else {
                Vec::new()
            };
            model_score.add(screen.tasks, model.len());
            finder_score.add(screen.tasks, found.len());
            both_score.add(screen.tasks, both.len());
            let ok = (screen.tasks > 0) == (!both.is_empty());
            println!(
                "{:24} expected {} | model {} finder {} together {} {}",
                screen.name,
                screen.tasks,
                model.len(),
                found.len(),
                both.len(),
                if ok { "" } else { "WRONG" }
            );
            for suggestion in &both {
                println!(
                    "    {:?} | {} | \"{}\"",
                    suggestion.task_type, suggestion.title, suggestion.quote
                );
            }
        }
        println!();
        if engine.is_some() {
            println!("{}", model_score.line("model with check", screens.len()));
        }
        println!("{}", finder_score.line("finder, no model", screens.len()));
        if engine.is_some() {
            println!("{}", both_score.line("both together", screens.len()));
        }
        Ok::<(), String>(())
    })?;
    Ok(())
}

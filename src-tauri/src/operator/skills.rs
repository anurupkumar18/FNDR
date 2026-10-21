//! Skills: a Notch Do run that worked, saved under a name and run again by
//! saying it ("my Monday setup", "run my Monday setup"). A skill keeps the
//! plan's steps, so running it plans nothing; every step still goes through
//! the same executor, `operator::policy`, verification and journal.

use crate::operator::plan::PlanStep;
use crate::operator::runs;
use crate::storage::StateStore;
use serde::{Deserialize, Serialize};

const SKILLS_KEY: &str = "notch_do_skills";
const MAX_SKILLS: usize = 30;
const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    /// The request the skill was saved from.
    pub request: String,
    pub steps: Vec<PlanStep>,
    pub saved_at: String,
}

pub fn list(store: &StateStore) -> Result<Vec<Skill>, String> {
    Ok(store
        .load_json::<Vec<Skill>>(SKILLS_KEY)?
        .unwrap_or_default())
}

fn words(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Saves a run that ended well under a name, unique ignoring case.
pub fn save_from_run(store: &StateStore, run_id: &str, name: &str) -> Result<Skill, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "A skill's name is 1 to {MAX_NAME_CHARS} characters."
        ));
    }
    let run = runs::get(store, run_id)?.ok_or_else(|| "That run is no longer kept.".to_string())?;
    if !run.repeatable() {
        let worked = run.ok == Some(true) && run.rating != Some(runs::Rating::DidNotWork);
        return Err(if worked && !run.steps.is_empty() {
            "A work set opens again by its name; it is not saved as a skill."
        } else {
            "Only a run that worked can be saved as a skill."
        }
        .to_string());
    }
    let mut skills = list(store)?;
    if skills.iter().any(|skill| words(&skill.name) == words(name)) {
        return Err(format!(
            "A skill called \u{201c}{name}\u{201d} already exists."
        ));
    }
    if skills.len() >= MAX_SKILLS {
        return Err(format!(
            "FNDR keeps up to {MAX_SKILLS} skills; delete one first."
        ));
    }
    let skill = Skill {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        request: run.request,
        steps: run.steps,
        saved_at: chrono::Utc::now().to_rfc3339(),
    };
    skills.push(skill.clone());
    store.save_json(SKILLS_KEY, &skills)?;
    Ok(skill)
}

pub fn delete(store: &StateStore, id: &str) -> Result<bool, String> {
    let mut skills = list(store)?;
    let before = skills.len();
    skills.retain(|skill| skill.id != id);
    store.save_json(SKILLS_KEY, &skills)?;
    Ok(skills.len() < before)
}

/// Which saved skill a request runs: its name, alone or after "run", "do"
/// or "start", with "my" or "the" allowed before it. `names` are the saved
/// names; the result is an index into them.
pub fn skill_asked_for(request: &str, names: &[String]) -> Option<usize> {
    let mut said = words(request);
    for lead in ["please ", "hey fndr ", "fndr "] {
        if let Some(rest) = said.strip_prefix(lead) {
            said = rest.to_string();
        }
    }
    for verb in ["run ", "do ", "start "] {
        if let Some(rest) = said.strip_prefix(verb) {
            said = rest.to_string();
            break;
        }
    }
    for article in ["my ", "the "] {
        if let Some(rest) = said.strip_prefix(article) {
            said = rest.to_string();
            break;
        }
    }
    names.iter().position(|name| {
        let name = words(name);
        let name = name
            .strip_prefix("my ")
            .or_else(|| name.strip_prefix("the "))
            .unwrap_or(&name)
            .to_string();
        !name.is_empty() && name == said
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::plan::{StepAction, StepCheck};

    fn open(app: &str) -> PlanStep {
        PlanStep {
            action: StepAction::OpenApp,
            label: format!("Open {app}"),
            app: app.into(),
            url: String::new(),
            goal: String::new(),
            text: String::new(),
            check: StepCheck::Frontmost,
            item: None,
        }
    }

    #[test]
    fn a_run_that_worked_is_saved_once_and_found_by_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        runs::record_plan(
            &store,
            "r1",
            "open Notes and open Slack",
            "local",
            &[open("Notes"), open("Slack")],
        )
        .unwrap();
        assert!(
            save_from_run(&store, "r1", "Monday setup").is_err(),
            "not ended yet"
        );
        runs::record_outcome(&store, "r1", true, "Done").unwrap();
        let skill = save_from_run(&store, "r1", "Monday setup").unwrap();
        assert_eq!(skill.steps.len(), 2);
        assert!(
            save_from_run(&store, "r1", "monday SETUP").is_err(),
            "names are unique ignoring case"
        );

        let names: Vec<String> = list(&store)
            .unwrap()
            .into_iter()
            .map(|skill| skill.name)
            .collect();
        for request in [
            "Monday setup",
            "run my Monday setup",
            "please do the monday setup",
            "start Monday setup.",
        ] {
            assert_eq!(skill_asked_for(request, &names), Some(0), "{request}");
        }
        for request in ["open Monday setup", "Monday", "run my Monday setup now"] {
            assert_eq!(skill_asked_for(request, &names), None, "{request}");
        }
        assert!(delete(&store, &skill.id).unwrap());
        assert!(list(&store).unwrap().is_empty());
    }

    #[test]
    fn a_run_that_failed_or_was_rated_wrong_is_not_a_skill() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        runs::record_plan(&store, "bad", "open Notes", "local", &[open("Notes")]).unwrap();
        runs::record_outcome(&store, "bad", false, "Stopped").unwrap();
        assert!(save_from_run(&store, "bad", "Notes").is_err());
        runs::record_plan(&store, "wrong", "open Notes", "local", &[open("Notes")]).unwrap();
        runs::record_outcome(&store, "wrong", true, "Done").unwrap();
        runs::rate(&store, "wrong", runs::Rating::DidNotWork).unwrap();
        assert!(save_from_run(&store, "wrong", "Notes").is_err());
    }
}

//! Synthetic proposals from captured or imported text may be shown as data,
//! but cannot by themselves authorize a tool call.

use fndr_lib::action_layer::gate::{decide, Call, Caller, Gate, Settings};
use fndr_lib::action_layer::queue::{ApprovalQueue, Via, APPROVAL_TTL};
use fndr_lib::action_layer::registry::{all, spec_for, Effect, Surface};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Deserialize)]
struct Attack {
    id: String,
    source_type: String,
    text: String,
    tool: String,
    args: Value,
    request: String,
}

fn attacks() -> Vec<Attack> {
    serde_json::from_str(include_str!("fixtures/action_injection.json")).unwrap()
}

fn settings() -> Settings {
    Settings {
        kill_switch: false,
        agent_notes_enabled: false,
        confirm_unnamed_apps: false,
    }
}

#[test]
fn captured_and_imported_instructions_cannot_start_a_call_across_the_registry_and_callers() {
    let attacks = attacks();
    let mut rows = 0;
    println!("origin | caller | registry rows | unattended actions");
    for attack in &attacks {
        let mut unattended = 0;
        for caller in Caller::ALL {
            let mut checked = 0;
            for spec in all().iter().filter(|spec| spec.effect != Effect::Read) {
                let call = Call::new(caller, spec.surface, spec.tool, &attack.args);
                if decide(&call, &settings()).runs_unattended() {
                    unattended += 1;
                }
                checked += 1;
                rows += 1;
            }
            println!(
                "{} | {caller:?} | {checked} | {unattended}",
                attack.source_type
            );
        }
        assert_eq!(
            unattended, 0,
            "{} from {} started an action without a person's request: {}",
            attack.id, attack.source_type, attack.text
        );
    }
    assert!(rows > 100, "the matrix must walk the registry");
}

#[test]
fn hostile_systemone_gate_cases_are_not_treated_as_person_requests() {
    let attacks = attacks();
    for attack in &attacks {
        let call = Call::new(
            Caller::CommandBar,
            Surface::NotchPlan,
            &attack.tool,
            &attack.args,
        )
        .asked(&attack.request);
        assert!(
            !decide(&call, &settings()).runs_unattended(),
            "{}: {}",
            attack.id,
            attack.text
        );
    }
}

#[test]
fn a_persons_request_can_still_open_an_app_and_an_accounted_for_link() {
    let app = json!({"app": "Notes"});
    let direct =
        Call::new(Caller::CommandBar, Surface::NotchPlan, "open_app", &app).asked("open Notes");
    assert_eq!(decide(&direct, &settings()), Gate::Run);
    let unnamed = Call::new(Caller::Voice, Surface::NotchPlan, "open_app", &app)
        .asked("open the app for my notes");
    assert_eq!(decide(&unnamed, &settings()), Gate::Run);
    let link = json!({"url": "https://github.com"});
    let direct =
        Call::new(Caller::CommandBar, Surface::NotchPlan, "open_url", &link).asked("open github");
    assert_eq!(decide(&direct, &settings()), Gate::Run);
}

#[test]
fn tool_registry_covers_every_notch_step_and_the_actions_named_in_the_attack_corpus() {
    for name in ["open_app", "open_url", "operate", "reopen_memory", "paste"] {
        assert!(spec_for(Surface::NotchPlan, name).is_some(), "{name}");
    }
    for attack in attacks() {
        if ["open_app", "open_url", "operate", "reopen_memory", "paste"]
            .contains(&attack.tool.as_str())
        {
            assert!(
                spec_for(Surface::NotchPlan, &attack.tool).is_some(),
                "{}",
                attack.id
            );
        }
    }
    assert!(all()
        .iter()
        .any(|spec| spec.surface == Surface::ComputerUse));
    assert!(all().iter().any(|spec| spec.surface == Surface::Mcp));
    assert!(all().iter().any(|spec| spec.surface == Surface::Agent));
}

#[test]
fn operator_tool_listing_and_plan_steps_cannot_drift_past_the_registry() {
    let plan = include_str!("../src/operator/plan.rs");
    let variants = plan
        .split("pub enum StepAction {")
        .nth(1)
        .expect("StepAction enum")
        .split("\n}")
        .next()
        .unwrap();
    let variant = Regex::new(r"(?m)^    ([A-Z][A-Za-z]+),$").unwrap();
    let mut step_count = 0;
    for found in variant.captures_iter(variants) {
        let mut tool = String::new();
        for (index, ch) in found[1].chars().enumerate() {
            if index > 0 && ch.is_uppercase() {
                tool.push('_');
            }
            tool.extend(ch.to_lowercase());
        }
        assert!(spec_for(Surface::NotchPlan, &tool).is_some(), "{tool}");
        step_count += 1;
    }
    assert!(step_count >= 5, "StepAction parsing lost the plan steps");

    let operator = include_str!("../src/operator/mcp.rs");
    let definitions = operator
        .split("fn tool_definitions() -> Value {")
        .nth(1)
        .expect("operator tool listing")
        .split("#[cfg(test)]")
        .next()
        .unwrap();
    let listed = Regex::new(r#"(?m)^            "name": "([^"]+)",$"#).unwrap();
    let mut tool_count = 0;
    for found in listed.captures_iter(definitions) {
        assert!(
            spec_for(Surface::ComputerUse, &found[1]).is_some(),
            "operator tool {} is absent from the action registry",
            &found[1]
        );
        tool_count += 1;
    }
    assert!(tool_count >= 11, "operator tool listing parser lost tools");

    let mcp = include_str!("../src/mcp/mod.rs");
    let listing = mcp
        .split("fn tools_list_result() -> Value {")
        .nth(1)
        .expect("MCP tool listing")
        .split("fn mcp_action_policy(")
        .next()
        .unwrap();
    let listed = Regex::new(r#"(?m)^                "name": "([^"]+)",$"#).unwrap();
    let mut mcp_count = 0;
    for found in listed.captures_iter(listing) {
        assert!(
            spec_for(Surface::Mcp, &found[1]).is_some(),
            "MCP tool {} is absent from the action registry",
            &found[1]
        );
        mcp_count += 1;
    }
    assert!(mcp_count >= 30, "MCP listing parser lost tools");
}

#[test]
fn announced_mcp_import_readers_are_classified_as_memory_text() {
    for name in [
        "fndr.recent_activity",
        "fndr.search_memories",
        "fndr.standup",
    ] {
        let row = spec_for(Surface::Mcp, name).expect(name);
        assert_eq!(row.effect, Effect::Read);
        assert!(row.returns_memory_text, "{name}");
    }
    let sources = spec_for(Surface::Mcp, "fndr.list_sources").expect("fndr.list_sources");
    assert_eq!(sources.effect, Effect::Read);
}

#[tokio::test]
async fn synthetic_executor_waits_for_the_structured_card_and_a_tap() {
    let queue = Arc::new(ApprovalQueue::default());
    let executed = Arc::new(AtomicBool::new(false));
    let waiting_queue = queue.clone();
    let executed_after_approval = executed.clone();
    let handle = tokio::spawn(async move {
        let args = json!({"app": "Notes", "text": "Synthetic release note"});
        let call = Call::new(Caller::Mcp, Surface::NotchPlan, "paste", &args);
        let outcome = waiting_queue
            .request(&call, &settings(), APPROVAL_TTL)
            .await;
        if outcome.proceeds() {
            executed_after_approval.store(true, Ordering::SeqCst);
        }
    });
    let card = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(card) = queue
                .list(chrono::Utc::now().timestamp_millis())
                .into_iter()
                .find(|card| card.status == fndr_lib::action_layer::queue::Status::Pending)
            {
                break card;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!executed.load(Ordering::SeqCst));
    assert_eq!(card.tool, "paste");
    assert_eq!(card.target.as_deref(), Some("Notes"));
    assert_eq!(card.args[1].value, "Synthetic release note");
    queue
        .resolve(
            &card.id,
            true,
            Via::Tap,
            &settings(),
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    handle.await.unwrap();
    assert!(executed.load(Ordering::SeqCst));
}

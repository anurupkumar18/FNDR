//! A task suggestion survives only when it quotes words that are on the
//! captured screen and those words state a commitment or a request.

use fndr_lib::storage::{Task, TaskType};
use fndr_lib::tasks::suggest::{
    accept, is_offered, is_suggestion, is_task_source, parse_suggestions, retire_unoffered,
    ExtractionGate, MIN_GAP_BETWEEN_EXTRACTIONS_MS, SUGGESTION_LIFESPAN_MS,
};

const EMAIL: &str = "From: Priya Nair\nSubject: Lab 4 report\nHi, can you send me the draft report by Friday? \
    Also I need to book the conference room for the demo.\nThe build cache is 51 GB and Chrome's cache is 1 GB.";

#[test]
fn ai_chat_and_system_screens_are_not_asked_for_tasks() {
    for app in [
        "Claude",
        "ChatGPT",
        "Codex",
        "UserNotificationCenter",
        "coreautha",
        "",
    ] {
        assert!(!is_task_source(app, None), "{app}");
    }
    assert!(!is_task_source(
        "Google Chrome",
        Some("https://chatgpt.com/c/123")
    ));
    assert!(!is_task_source(
        "Safari",
        Some("https://www.claude.ai/chat/9")
    ));
}

#[test]
fn mail_documents_and_ordinary_pages_are_asked() {
    assert!(is_task_source("Mail", None));
    assert!(is_task_source(
        "Google Chrome",
        Some("https://canvas.example.edu/courses/1")
    ));
    assert!(is_task_source("Notion", None));
}

#[test]
fn a_request_quoted_from_the_screen_becomes_a_suggestion() {
    let raw =
        "FOLLOWUP | Send Priya the draft report | can you send me the draft report by Friday\n\
               TODO | Book the conference room | I need to book the conference room for the demo";
    let kept = parse_suggestions(raw, EMAIL);
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].title, "Send Priya the draft report");
    assert_eq!(kept[0].quote, "can you send me the draft report by Friday");
    assert_eq!(kept[1].task_type, TaskType::Todo);
}

#[test]
fn a_quote_that_is_not_on_the_screen_is_dropped() {
    let raw = "TODO | Email the dean about funding | I need to email the dean about funding";
    assert!(parse_suggestions(raw, EMAIL).is_empty());
}

#[test]
fn a_description_of_the_screen_is_not_a_task() {
    // On screen, word for word, but nobody committed to or asked for anything.
    let raw = "TODO | Clear the build cache | The build cache is 51 GB";
    assert!(parse_suggestions(raw, EMAIL).is_empty());
}

#[test]
fn lines_without_a_quote_or_echoing_the_format_are_dropped() {
    for raw in [
        "TODO: Book the conference room",
        "TODO | Book the conference room",
        "FOLLOWUP | [Team/Person] - review failures | can you send me the draft report by Friday",
        "FOLLOWUP | Priya + draft report | can you send me the draft report by Friday",
        "NONE",
        "",
    ] {
        assert!(parse_suggestions(raw, EMAIL).is_empty(), "{raw}");
    }
}

#[test]
fn a_reminder_needs_a_date_and_a_follow_up_needs_a_name() {
    let raw =
        "REMINDER | Book the conference room | I need to book the conference room for the demo";
    assert_eq!(parse_suggestions(raw, EMAIL)[0].task_type, TaskType::Todo);

    let screen = "Reminder: please submit the timesheet by 5 pm on Thursday. Please ask Jordan to review the plan.";
    let dated = "REMINDER | Submit the timesheet | please submit the timesheet by 5 pm on Thursday";
    assert_eq!(
        parse_suggestions(dated, screen)[0].task_type,
        TaskType::Reminder
    );
    let named = "FOLLOWUP | Ask Jordan to review the plan | Please ask Jordan to review the plan";
    assert_eq!(
        parse_suggestions(named, screen)[0].task_type,
        TaskType::Followup
    );
    let unnamed =
        "FOLLOWUP | Submit the timesheet | please submit the timesheet by 5 pm on Thursday";
    assert_eq!(
        parse_suggestions(unnamed, screen)[0].task_type,
        TaskType::Todo
    );
}

#[test]
fn at_most_two_suggestions_and_never_the_same_quote_twice() {
    let screen = "I need to call the bank. I need to renew the lease. I need to pay the invoice.";
    let raw = "TODO | Call the bank | I need to call the bank\n\
               TODO | Phone the bank today | I need to call the bank\n\
               TODO | Renew the lease | I need to renew the lease\n\
               TODO | Pay the invoice | I need to pay the invoice";
    let kept = parse_suggestions(raw, screen);
    assert_eq!(
        kept.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
        vec!["Call the bank", "Renew the lease"]
    );
}

#[test]
fn a_cue_inside_another_word_does_not_count() {
    // "due" in "produced", "to do" across "into document".
    let screen = "The report was produced by the residue analysis team.";
    let raw =
        "TODO | Read the residue analysis | The report was produced by the residue analysis team";
    assert!(parse_suggestions(raw, screen).is_empty());
}

#[test]
fn one_extraction_per_app_per_window() {
    let mut gate = ExtractionGate::default();
    let start = 1_790_000_000_000;
    assert!(gate.admit("Mail", start));
    assert!(!gate.admit("Mail", start + 60_000));
    assert!(!gate.admit("mail ", start + MIN_GAP_BETWEEN_EXTRACTIONS_MS - 1));
    assert!(
        gate.admit("Notion", start + 60_000),
        "another app has its own window"
    );
    assert!(gate.admit("Mail", start + MIN_GAP_BETWEEN_EXTRACTIONS_MS));
}

const NOW: i64 = 1_790_000_000_000;

fn stored(id: &str, source_app: &str, quote: &str, age_ms: i64) -> Task {
    Task {
        id: id.to_string(),
        title: format!("Send the draft report {id}"),
        description: quote.to_string(),
        source_app: source_app.to_string(),
        source_memory_id: Some(format!("mem-{id}")),
        created_at: NOW - age_ms,
        due_date: None,
        is_completed: false,
        is_dismissed: false,
        task_type: TaskType::Todo,
        linked_urls: Vec::new(),
        linked_memory_ids: Vec::new(),
    }
}

#[test]
fn a_suggestion_is_offered_only_with_its_quote_and_only_for_a_few_days() {
    let quote = "can you send me the draft report by Friday";
    assert!(is_offered(
        &stored("fresh", "Memory:Mail", quote, 60_000),
        NOW
    ));
    assert!(!is_offered(
        &stored("unquoted", "Memory:Mail", "", 60_000),
        NOW
    ));
    assert!(!is_offered(
        &stored("old", "Memory:Mail", quote, SUGGESTION_LIFESPAN_MS + 1),
        NOW
    ));
    // The person's own and accepted tasks are not suggestions and never expire here.
    assert!(!is_suggestion(&stored(
        "mine",
        "manual",
        "",
        SUGGESTION_LIFESPAN_MS * 9
    )));
}

#[test]
fn accepting_keeps_the_task_and_its_memory_and_ends_its_expiry() {
    let mut task = stored(
        "s",
        "Memory:Mail",
        "please send the report",
        SUGGESTION_LIFESPAN_MS * 2,
    );
    accept(&mut task);
    assert_eq!(task.source_app, "Accepted:Mail");
    assert_eq!(task.source_memory_id.as_deref(), Some("mem-s"));
    assert!(!is_suggestion(&task));
}

#[test]
fn retiring_dismisses_what_is_no_longer_offered_and_nothing_else() {
    let quote = "please send the report";
    let mut done = stored("done", "Memory:Mail", "", 60_000);
    done.is_completed = true;
    let mut accepted = stored("accepted", "Memory:Mail", "", SUGGESTION_LIFESPAN_MS * 2);
    accept(&mut accepted);
    let mut tasks = vec![
        stored("unquoted", "Memory:ChatGPT", "", 60_000),
        stored("expired", "Memory:Mail", quote, SUGGESTION_LIFESPAN_MS + 1),
        stored("fresh", "Memory:Mail", quote, 60_000),
        stored("mine", "manual", "", SUGGESTION_LIFESPAN_MS * 2),
        stored("meeting", "Meeting:Standup", "", SUGGESTION_LIFESPAN_MS * 2),
        accepted,
        done,
    ];
    assert_eq!(retire_unoffered(&mut tasks, NOW), 2);
    let dismissed: Vec<&str> = tasks
        .iter()
        .filter(|task| task.is_dismissed)
        .map(|task| task.id.as_str())
        .collect();
    assert_eq!(dismissed, vec!["unquoted", "expired"]);
    assert_eq!(
        retire_unoffered(&mut tasks, NOW),
        0,
        "a second pass changes nothing"
    );
}

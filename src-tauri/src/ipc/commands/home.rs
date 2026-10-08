//! The Home greeting and the daily briefing.

use crate::AppState;
use chrono::Timelike;
use std::sync::Arc;
use tauri::State;

/// The briefing shown in To-dos, written without a model (`briefing.rs`).
/// `mode` is "morning" or "evening"; without one the hour decides.
#[tauri::command]
pub async fn generate_daily_briefing(
    state: State<'_, Arc<AppState>>,
    mode: Option<String>,
) -> Result<String, String> {
    // The part of the day decides the briefing, unless the caller names one.
    // It is written without a model: see `briefing.rs` for why.
    let now = chrono::Local::now();
    let resolved_mode =
        mode.unwrap_or_else(|| crate::briefing::mode_for_hour(now.hour()).to_string());
    Ok(crate::briefing::briefing_for(&state, &resolved_mode, now).await)
}

#[tauri::command]
pub fn get_fun_greeting(name: Option<String>) -> Result<String, String> {
    use rand::prelude::IndexedRandom;
    let base_name = name.unwrap_or_else(|| "there".to_string());

    let hour = chrono::Local::now().hour();

    let prefix = if (4..12).contains(&hour) {
        "Good Morning"
    } else if (12..16).contains(&hour) {
        "Good Afternoon"
    } else if (16..20).contains(&hour) {
        "Good Evening"
    } else {
        "Good Night"
    };

    let fun_suffixes = [
        "Ready to conquer the day?",
        "Let's dive into your memories.",
        "What are we exploring today?",
        "Time to make some magic happen.",
        "Welcome back to the matrix.",
        "Let's get productive.",
        "System fully operational.",
    ];

    let mut rng = rand::rng();
    let random_suffix = fun_suffixes.choose(&mut rng).unwrap_or(&"");

    Ok(format!("{}, {}! {}", prefix, base_name, random_suffix))
}

//! Action approval queue and settings IPC.

use crate::action_layer::gate::Settings;
use crate::action_layer::queue::{self, ActionCard, Via};
use crate::config::Config;
use crate::AppState;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSettings {
    pub actions_kill_switch: bool,
    pub confirm_unnamed_apps: bool,
}

impl From<&Config> for ActionSettings {
    fn from(config: &Config) -> Self {
        Self {
            actions_kill_switch: config.actions_kill_switch,
            confirm_unnamed_apps: config.confirm_unnamed_apps,
        }
    }
}

#[tauri::command]
pub async fn get_action_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<ActionSettings, String> {
    Ok(ActionSettings::from(&*state.config.read()))
}

#[tauri::command]
pub async fn set_action_settings(
    state: State<'_, Arc<AppState>>,
    settings: ActionSettings,
) -> Result<ActionSettings, String> {
    update_action_settings(&state.config, settings, |config| {
        config.save().map_err(|error| error.to_string())
    })?;
    Ok(settings)
}

fn update_action_settings(
    config: &RwLock<Config>,
    settings: ActionSettings,
    save: impl FnOnce(&Config) -> Result<(), String>,
) -> Result<(), String> {
    let mut current = config.write();
    let mut next = current.clone();
    next.actions_kill_switch = settings.actions_kill_switch;
    next.confirm_unnamed_apps = settings.confirm_unnamed_apps;
    save(&next)?;
    *current = next;
    Ok(())
}

#[tauri::command]
pub async fn list_action_approvals() -> Result<Vec<ActionCard>, String> {
    Ok(queue::global().list(chrono::Utc::now().timestamp_millis()))
}

#[tauri::command]
pub async fn resolve_action_approval(
    state: State<'_, Arc<AppState>>,
    id: String,
    approve: bool,
) -> Result<ActionCard, String> {
    let settings = Settings::from(&*state.config.read());
    queue::global()
        .resolve(
            &id,
            approve,
            Via::Tap,
            &settings,
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(|error| error.message())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use parking_lot::RwLock;

    #[test]
    fn action_settings_save_is_atomic_and_keeps_the_optional_app_confirmation() {
        let config = RwLock::new(Config::default());
        let desired = ActionSettings {
            actions_kill_switch: true,
            confirm_unnamed_apps: true,
        };
        assert!(update_action_settings(&config, desired, |_| Err("disk full".into())).is_err());
        assert!(!config.read().actions_kill_switch);
        assert!(!config.read().confirm_unnamed_apps);

        update_action_settings(&config, desired, |next| {
            assert!(next.actions_kill_switch);
            assert!(next.confirm_unnamed_apps);
            Ok(())
        })
        .unwrap();
        assert!(config.read().actions_kill_switch);
        assert!(config.read().confirm_unnamed_apps);
    }
}

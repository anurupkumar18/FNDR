//! Meeting prep from the calendar (ADR 029): its switch and macOS calendar
//! access. The calendar is read on this Mac only; see
//! `proactive_signals::calendar`.

use crate::proactive_signals::calendar::{CalendarSource, CalendarStatus};
use crate::proactive_signals::eventkit::EventKitCalendar;
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

const CALENDAR_PRIVACY_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPrepState {
    pub enabled: bool,
    pub status: CalendarStatus,
}

/// Turning the switch on asks macOS for access (the system prompt appears
/// only the first time) and stays on only when access is granted. Turning it
/// off asks for nothing.
fn switch_calendar_prep(source: &dyn CalendarSource, requested: bool) -> CalendarPrepState {
    let status = if requested {
        source.request_access()
    } else {
        source.status()
    };
    CalendarPrepState {
        enabled: requested && status == CalendarStatus::Granted,
        status,
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn calendar_meeting_prep_state(
    state: State<'_, Arc<AppState>>,
) -> Result<CalendarPrepState, String> {
    let status = blocking(|| EventKitCalendar.status()).await?;
    Ok(CalendarPrepState {
        enabled: state
            .inner()
            .config
            .read()
            .proactive_signals
            .calendar_meeting_prep,
        status,
    })
}

#[tauri::command]
pub async fn set_calendar_meeting_prep(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<CalendarPrepState, String> {
    let next = blocking(move || switch_calendar_prep(&EventKitCalendar, enabled)).await?;
    let mut config = state.inner().config.write();
    let previous = config.proactive_signals.calendar_meeting_prep;
    config.proactive_signals.calendar_meeting_prep = next.enabled;
    if let Err(err) = config.save() {
        config.proactive_signals.calendar_meeting_prep = previous;
        return Err(err.to_string());
    }
    Ok(next)
}

/// Opens System Settings at Privacy and Security, Calendars.
#[tauri::command]
pub async fn open_calendar_privacy_settings() -> Result<(), String> {
    tokio::process::Command::new("open")
        .arg(CALENDAR_PRIVACY_URL)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proactive_signals::calendar::tests::FakeCalendar;

    #[test]
    fn turning_on_asks_for_access_and_stays_on_only_when_granted() {
        let fake = FakeCalendar::granted(Vec::new());
        assert_eq!(
            switch_calendar_prep(&fake, true),
            CalendarPrepState {
                enabled: true,
                status: CalendarStatus::Granted
            }
        );
        assert_eq!(fake.prompts.get(), 1);

        for status in [
            CalendarStatus::Denied,
            CalendarStatus::Restricted,
            CalendarStatus::NotDetermined,
        ] {
            let mut fake = FakeCalendar::granted(Vec::new());
            fake.status = status;
            assert_eq!(
                switch_calendar_prep(&fake, true),
                CalendarPrepState {
                    enabled: false,
                    status
                }
            );
        }
    }

    #[test]
    fn turning_off_asks_for_nothing() {
        let fake = FakeCalendar::granted(Vec::new());
        assert!(!switch_calendar_prep(&fake, false).enabled);
        assert_eq!(fake.prompts.get(), 0);
    }
}

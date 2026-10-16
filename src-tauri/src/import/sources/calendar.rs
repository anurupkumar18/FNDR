use crate::import::*;
use crate::proactive_signals::calendar::{calendars_to_read, CalendarSource, CalendarStatus};
use crate::proactive_signals::eventkit::EventKitCalendar;

pub const SKIP: &[&str] = &[
    "declined or cancelled events",
    "subscribed calendars by default",
    "calendars outside the selected IDs",
];

pub struct CalendarImport {
    pub selected_calendar_ids: Vec<String>,
}

impl CalendarImport {
    pub fn new(selected_calendar_ids: Vec<String>) -> Self {
        Self {
            selected_calendar_ids,
        }
    }
}

impl Default for CalendarImport {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl ImportSource for CalendarImport {
    fn kind(&self) -> ImportKind {
        ImportKind::Calendar
    }
    fn label(&self) -> &'static str {
        "Calendar"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::Calendar]
    }
    fn scan(
        &self,
        since: Option<Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, ImportError> {
        scan_calendar(
            &EventKitCalendar,
            &self.selected_calendar_ids,
            since,
            window_days,
            limit,
            chrono::Utc::now().timestamp_millis(),
        )
    }
}

pub(crate) fn scan_calendar(
    source: &dyn CalendarSource,
    selected: &[String],
    since: Option<Cursor>,
    window_days: u32,
    limit: usize,
    now_ms: i64,
) -> Result<ScanBatch, ImportError> {
    if limit == 0 {
        return Err(ImportError::InvalidData(
            "scan limit must be positive".into(),
        ));
    }
    let status = if source.status() == CalendarStatus::NotDetermined {
        source.request_access()
    } else {
        source.status()
    };
    if status != CalendarStatus::Granted {
        return Err(ImportError::PermissionDenied(Permission::Calendar));
    }
    let calendars = source.calendars().map_err(ImportError::InvalidData)?;
    let ids = calendars_to_read(&calendars, selected);
    if ids.is_empty() {
        return Ok(ScanBatch {
            items: Vec::new(),
            next: None,
            done: true,
        });
    }
    let day_ms = 86_400_000_i64;
    let from_ms = now_ms.saturating_sub(i64::from(window_days).saturating_mul(day_ms));
    let to_ms = now_ms.saturating_add(7 * day_ms);
    let mut events = source
        .events(&ids, from_ms, to_ms)
        .map_err(ImportError::InvalidData)?;
    events.retain(|event| {
        !event.declined
            && ids.contains(&event.calendar_id)
            && (from_ms..=to_ms).contains(&event.starts_at)
    });
    events.sort_by_key(|event| (event.starts_at, event.id.clone()));
    let offset = since
        .as_ref()
        .and_then(|cursor| cursor.0.parse::<usize>().ok())
        .unwrap_or(0);
    let total = events.len();
    let items = events
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|event| {
            let calendar_name = calendars
                .iter()
                .find(|calendar| calendar.id == event.calendar_id)
                .map(|calendar| calendar.name.as_str())
                .unwrap_or("");
            let title = event.title.unwrap_or_default();
            let mut fields = vec![
                format!("Calendar: {calendar_name}"),
                format!("Start: {}", display_time(event.starts_at)),
                format!("End: {}", display_time(event.ends_at)),
            ];
            if !event.attendees.is_empty() {
                fields.push(format!("Attendees: {}", event.attendees.join(", ")));
            }
            if let Some(location) = event.location {
                fields.push(format!("Location: {location}"));
            }
            if let Some(notes) = event.notes {
                fields.push(format!("Notes: {notes}"));
            }
            if let Some(url) = event.conference_url.as_ref() {
                fields.push(format!("Conference: {url}"));
            }
            let text: String = fields.join("\n").chars().take(20_000).collect();
            ImportItem {
                kind: ImportKind::Calendar,
                external_id: format!("{}@{}", event.id, event.starts_at),
                timestamp_ms: event.starts_at,
                title,
                text,
                reopen_target: None,
                app_label: "Calendar".into(),
                people: event.attendees,
                url: event.conference_url,
            }
        })
        .collect::<Vec<_>>();
    let next_offset = offset.saturating_add(items.len());
    Ok(ScanBatch {
        items,
        next: (next_offset < total).then(|| Cursor(next_offset.to_string())),
        done: next_offset >= total,
    })
}

fn display_time(timestamp_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp_ms)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| timestamp_ms.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proactive_signals::calendar::tests::{event, FakeCalendar, NOW};

    #[test]
    fn selected_calendar_imports_event_details_and_skips_declines() {
        let mut keep = event("shared", "work", "Planning", -60);
        keep.attendees = vec!["Ana".into()];
        keep.location = Some("Room 2".into());
        keep.notes = Some("Discuss launch".into());
        keep.conference_url = Some("https://meet.example/call".into());
        let mut declined = event("declined", "work", "Skip", -30);
        declined.declined = true;
        let fake =
            FakeCalendar::granted(vec![keep, declined, event("home", "home", "Private", -30)]);
        let batch = scan_calendar(&fake, &["work".into()], None, 30, 10, NOW).unwrap();
        assert_eq!(batch.items.len(), 1);
        let item = &batch.items[0];
        assert_eq!(item.external_id, format!("shared@{}", NOW - 60 * 60_000));
        assert!(item.text.contains("Room 2"));
        assert!(item.text.contains("Discuss launch"));
        assert_eq!(item.people, vec!["Ana"]);
        assert_eq!(item.url.as_deref(), Some("https://meet.example/call"));
        assert_eq!(fake.prompts.get(), 0);
    }

    #[test]
    fn explicit_scan_requests_permission_and_bounds_the_window() {
        let mut fake = FakeCalendar::granted(vec![
            event("old", "work", "Old", -32 * 24 * 60),
            event("soon", "work", "Soon", 6 * 24 * 60),
            event("far", "work", "Far", 8 * 24 * 60),
        ]);
        fake.status = CalendarStatus::NotDetermined;
        let batch = scan_calendar(&fake, &[], None, 30, 10, NOW).unwrap_err();
        assert!(matches!(
            batch,
            ImportError::PermissionDenied(Permission::Calendar)
        ));
        assert_eq!(fake.prompts.get(), 1);
    }

    #[test]
    fn window_pagination_and_unicode_cap() {
        let mut huge = event("huge", "work", "Long notes", -20);
        huge.notes = Some("é".repeat(25_000));
        let fake = FakeCalendar::granted(vec![
            event("old", "work", "Old", -32 * 24 * 60),
            huge,
            event("soon", "work", "Soon", 6 * 24 * 60),
            event("far", "work", "Far", 8 * 24 * 60),
        ]);
        let first = scan_calendar(&fake, &[], None, 30, 1, NOW).unwrap();
        assert_eq!(first.items[0].title, "Long notes");
        assert_eq!(first.items[0].text.chars().count(), 20_000);
        assert!(!first.done);
        let second = scan_calendar(&fake, &[], first.next, 30, 1, NOW).unwrap();
        assert_eq!(second.items[0].title, "Soon");
        assert!(second.done);
        assert_eq!(fake.prompts.get(), 0);
    }
}

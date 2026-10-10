//! EventKit on this Mac, read-only, as a [`CalendarSource`] (ADR 029).
//! Nothing here creates, changes or deletes an event, and no event text is
//! logged.

use super::calendar::{CalendarEvent, CalendarInfo, CalendarSource, CalendarStatus};

/// The calendar FNDR reads: the one Calendar.app syncs on this Mac.
pub struct EventKitCalendar;

#[cfg(not(target_os = "macos"))]
impl CalendarSource for EventKitCalendar {
    fn status(&self) -> CalendarStatus {
        CalendarStatus::Restricted
    }
    fn request_access(&self) -> CalendarStatus {
        CalendarStatus::Restricted
    }
    fn calendars(&self) -> Result<Vec<CalendarInfo>, String> {
        Err("No calendar on this platform".into())
    }
    fn events(&self, _: &[String], _: i64, _: i64) -> Result<Vec<CalendarEvent>, String> {
        Err("No calendar on this platform".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{msg_send, msg_send_id, sel};
    use objc2_foundation::{NSArray, NSString};
    use std::sync::mpsc;
    use std::time::Duration;

    // `AnyClass::get` finds EventKit's classes only once the framework is
    // loaded, which nothing else in the app does.
    #[link(name = "EventKit", kind = "framework")]
    extern "C" {}

    /// `EKEntityTypeEvent`.
    const ENTITY_EVENT: usize = 0;
    /// `EKCalendarTypeBirthday`: generated from contacts, someone else's dates.
    const CALENDAR_TYPE_BIRTHDAY: isize = 4;
    /// `EKEventStatusCanceled`.
    const EVENT_STATUS_CANCELED: isize = 3;
    /// `EKParticipantStatusDeclined`.
    const PARTICIPANT_DECLINED: isize = 3;
    /// How long the system prompt may wait for an answer.
    const PROMPT_TIMEOUT: Duration = Duration::from_secs(300);

    fn store_class() -> Option<&'static AnyClass> {
        AnyClass::get("EKEventStore")
    }

    unsafe fn new_store() -> Option<Retained<AnyObject>> {
        let store = msg_send_id![store_class()?, alloc];
        msg_send_id![store, init]
    }

    unsafe fn text(value: *const NSString) -> Option<String> {
        (!value.is_null()).then(|| (*value).to_string())
    }

    unsafe fn items(array: *const AnyObject) -> Vec<*const AnyObject> {
        if array.is_null() {
            return Vec::new();
        }
        let count: usize = msg_send![array, count];
        (0..count)
            .map(|index| msg_send![array, objectAtIndex: index])
            .collect()
    }

    /// Declined by the person, or cancelled by the organizer.
    unsafe fn declined(event: *const AnyObject) -> bool {
        let status: isize = msg_send![event, status];
        if status == EVENT_STATUS_CANCELED {
            return true;
        }
        let attendees: *const AnyObject = msg_send![event, attendees];
        items(attendees).into_iter().any(|participant| {
            let me: Bool = msg_send![participant, isCurrentUser];
            let answer: isize = msg_send![participant, participantStatus];
            me.as_bool() && answer == PARTICIPANT_DECLINED
        })
    }

    unsafe fn read_event(event: *const AnyObject) -> Option<CalendarEvent> {
        let id = text(msg_send![event, eventIdentifier])?;
        let calendar: *const AnyObject = msg_send![event, calendar];
        if calendar.is_null() {
            return None;
        }
        let start: *const AnyObject = msg_send![event, startDate];
        if start.is_null() {
            return None;
        }
        let seconds: f64 = msg_send![start, timeIntervalSince1970];
        let all_day: Bool = msg_send![event, isAllDay];
        Some(CalendarEvent {
            id,
            calendar_id: text(msg_send![calendar, calendarIdentifier])?,
            title: text(msg_send![event, title]),
            starts_at: (seconds * 1000.0) as i64,
            all_day: all_day.as_bool(),
            declined: declined(event),
        })
    }

    unsafe fn date(ms: i64) -> Option<Retained<AnyObject>> {
        let seconds = ms as f64 / 1000.0;
        msg_send_id![AnyClass::get("NSDate")?, dateWithTimeIntervalSince1970: seconds]
    }

    impl CalendarSource for EventKitCalendar {
        fn status(&self) -> CalendarStatus {
            let Some(class) = store_class() else {
                return CalendarStatus::Restricted;
            };
            let raw: isize =
                unsafe { msg_send![class, authorizationStatusForEntityType: ENTITY_EVENT] };
            CalendarStatus::from_raw(raw as i64)
        }

        fn request_access(&self) -> CalendarStatus {
            if self.status() != CalendarStatus::NotDetermined {
                return self.status();
            }
            let (sender, answer) = mpsc::channel::<()>();
            let done = RcBlock::new(move |_granted: Bool, _error: *mut AnyObject| {
                let _ = sender.send(());
            });
            autoreleasepool(|_| unsafe {
                let Some(store) = new_store() else {
                    return;
                };
                let full_access = sel!(requestFullAccessToEventsWithCompletion:);
                let has_full_access: Bool = msg_send![&store, respondsToSelector: full_access];
                if has_full_access.as_bool() {
                    let _: () = msg_send![&store, requestFullAccessToEventsWithCompletion: &*done];
                } else {
                    let _: () = msg_send![
                        &store,
                        requestAccessToEntityType: ENTITY_EVENT
                        completion: &*done
                    ];
                }
                // The store stays alive until macOS answers.
                let _ = answer.recv_timeout(PROMPT_TIMEOUT);
            });
            self.status()
        }

        fn calendars(&self) -> Result<Vec<CalendarInfo>, String> {
            autoreleasepool(|_| unsafe {
                let store = new_store().ok_or("EventKit is not available")?;
                let calendars: *const AnyObject =
                    msg_send![&store, calendarsForEntityType: ENTITY_EVENT];
                Ok(items(calendars)
                    .into_iter()
                    .filter_map(|calendar| {
                        let subscribed: Bool = msg_send![calendar, isSubscribed];
                        let kind: isize = msg_send![calendar, type];
                        Some(CalendarInfo {
                            id: text(msg_send![calendar, calendarIdentifier])?,
                            subscribed: subscribed.as_bool() || kind == CALENDAR_TYPE_BIRTHDAY,
                        })
                    })
                    .collect())
            })
        }

        fn events(
            &self,
            calendar_ids: &[String],
            from_ms: i64,
            to_ms: i64,
        ) -> Result<Vec<CalendarEvent>, String> {
            autoreleasepool(|_| unsafe {
                let store = new_store().ok_or("EventKit is not available")?;
                let all: *const AnyObject = msg_send![&store, calendarsForEntityType: ENTITY_EVENT];
                let chosen: Vec<Retained<AnyObject>> = items(all)
                    .into_iter()
                    .filter(|calendar| {
                        text(msg_send![*calendar, calendarIdentifier])
                            .is_some_and(|id| calendar_ids.contains(&id))
                    })
                    .filter_map(|calendar| Retained::retain(calendar as *mut AnyObject))
                    .collect();
                if chosen.is_empty() {
                    return Ok(Vec::new());
                }
                let chosen = NSArray::from_vec(chosen);
                let (Some(start), Some(end)) = (date(from_ms), date(to_ms)) else {
                    return Err("NSDate is not available".into());
                };
                let predicate: Option<Retained<AnyObject>> = msg_send_id![
                    &store,
                    predicateForEventsWithStartDate: &*start
                    endDate: &*end
                    calendars: &*chosen
                ];
                let predicate = predicate.ok_or("EventKit gave no predicate")?;
                let events: *const AnyObject =
                    msg_send![&store, eventsMatchingPredicate: &*predicate];
                Ok(items(events)
                    .into_iter()
                    .filter_map(|event| read_event(event))
                    .collect())
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Reads this Mac's real calendar. It shows the macOS prompt when
        /// access was never asked for, so it runs only by hand:
        /// `cargo test --lib eventkit -- --ignored --nocapture`.
        /// Prints counts only, never titles.
        #[test]
        #[ignore]
        fn reads_the_real_calendar_read_only() {
            let source = EventKitCalendar;
            println!("status before: {:?}", source.status());
            let status = source.request_access();
            println!("status after request: {status:?}");
            // Without access EventKit lists no calendars; the calls still run.
            let calendars = source.calendars().expect("calendars");
            let read = crate::proactive_signals::calendar::calendars_to_read(&calendars, &[]);
            let now = chrono::Utc::now().timestamp_millis();
            let events = source
                .events(&read, now - 86_400_000, now + 7 * 86_400_000)
                .expect("events");
            println!(
                "calendars: {} ({} read by default); events in the week ahead: {} \
                 (all-day {}, declined {}, untitled {})",
                calendars.len(),
                read.len(),
                events.len(),
                events.iter().filter(|e| e.all_day).count(),
                events.iter().filter(|e| e.declined).count(),
                events.iter().filter(|e| e.title.is_none()).count(),
            );
        }
    }
}

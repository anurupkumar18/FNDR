//! Routines: a work set the person opens around the same time on days like
//! today. Every opening is logged in the state store (local day and minute,
//! never while FNDR is private); `offers` mines that log. An offer is only
//! data for a card: FNDR never opens anything because of one.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
use serde::{Deserialize, Serialize};

use super::named;
use super::rank::eligible;
use crate::storage::StateStore;
use crate::AppState;

const LOG_KEY: &str = "work_set_open_log_v1";
const DISMISSED_KEY: &str = "routine_offers_dismissed_v1";
/// The log keeps at most this many openings, and none older than `MAX_AGE_DAYS`.
pub const MAX_ENTRIES: usize = 400;
pub const MAX_AGE_DAYS: i64 = 60;
/// Opened on at least this many distinct days to count as a routine.
pub const MIN_DAYS: usize = 3;
/// Half of the two-hour window, in minutes.
const HALF_WINDOW: i32 = 60;
/// An offer is due from this many minutes before the usual time.
const DUE_LEAD: i32 = 15;
const DAY_MINUTES: i32 = 24 * 60;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Home,
    Notch,
}

/// One opening of a work set, in the Mac's local time when it happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    /// The saved set's id, or the sorted memory ids.
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_id: Option<String>,
    pub memory_ids: Vec<String>,
    pub label: String,
    pub day: NaiveDate,
    /// Minutes after local midnight.
    pub minute: u16,
    pub at_ms: i64,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dismissal {
    pub id: String,
    pub day: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineOffer {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_id: Option<String>,
    pub memory_ids: Vec<String>,
    pub reason: String,
    /// It is about the usual time; before that the offer is coming up.
    pub due_now: bool,
}

pub fn signature(set_id: Option<&str>, memory_ids: &[String]) -> String {
    match set_id {
        Some(id) => format!("set:{id}"),
        None => {
            let mut ids = memory_ids.to_vec();
            ids.sort();
            ids.dedup();
            format!("ids:{}", ids.join(","))
        }
    }
}

pub fn opening(
    set_id: Option<&str>,
    memory_ids: &[String],
    label: &str,
    source: Source,
    local: NaiveDateTime,
    at_ms: i64,
) -> Opening {
    Opening {
        signature: signature(set_id, memory_ids),
        set_id: set_id.map(str::to_string),
        memory_ids: memory_ids.to_vec(),
        label: label.to_string(),
        day: local.date(),
        minute: (local.hour() * 60 + local.minute()) as u16,
        at_ms,
        source,
    }
}

/// Adds an opening and drops what is too old or beyond the cap, oldest first.
pub fn append(log: &mut Vec<Opening>, entry: Opening) {
    let now_ms = entry.at_ms;
    log.push(entry);
    log.retain(|seen| now_ms - seen.at_ms <= MAX_AGE_DAYS * DAY_MS);
    log.sort_by_key(|seen| seen.at_ms);
    let over = log.len().saturating_sub(MAX_ENTRIES);
    log.drain(..over);
}

fn weekend(day: NaiveDate) -> bool {
    matches!(day.weekday(), Weekday::Sat | Weekday::Sun)
}

/// `to - from` in minutes, the short way round midnight.
fn offset(from: i32, to: i32) -> i32 {
    (to - from + DAY_MINUTES + DAY_MINUTES / 2).rem_euclid(DAY_MINUTES) - DAY_MINUTES / 2
}

fn clock(minute: i32) -> String {
    let minute = minute.rem_euclid(DAY_MINUTES) as u32;
    NaiveTime::from_hms_opt(minute / 60, minute % 60, 0)
        .map(|time| time.format("%-I:%M %p").to_string())
        .unwrap_or_default()
}

/// The sets opened on at least three distinct days like today (weekday or
/// weekend) within a two-hour window that `now` falls in, and not yet opened
/// or dismissed today. Deterministic for the same log and clock.
pub fn offers(
    log: &[Opening],
    dismissed: &[Dismissal],
    now: NaiveDateTime,
    now_ms: i64,
) -> Vec<RoutineOffer> {
    let today = now.date();
    let now_minute = (now.hour() * 60 + now.minute()) as i32;
    let mut by_set: BTreeMap<&str, Vec<&Opening>> = BTreeMap::new();
    for entry in log {
        by_set.entry(&entry.signature).or_default().push(entry);
    }
    let mut found: Vec<(i32, RoutineOffer)> = Vec::new();
    for (id, entries) in by_set {
        if entries.iter().any(|entry| entry.day == today)
            || dismissed.iter().any(|d| d.id == id && d.day == today)
        {
            continue;
        }
        let alike: Vec<&Opening> = entries
            .into_iter()
            .filter(|entry| {
                weekend(entry.day) == weekend(today)
                    && entry.day < today
                    && now_ms - entry.at_ms <= MAX_AGE_DAYS * DAY_MS
            })
            .collect();
        let near = |anchor: i32| -> Vec<&Opening> {
            alike
                .iter()
                .copied()
                .filter(|entry| offset(anchor, entry.minute as i32).abs() <= HALF_WINDOW)
                .collect()
        };
        let days =
            |members: &[&Opening]| members.iter().map(|e| e.day).collect::<BTreeSet<_>>().len();
        let best = alike
            .iter()
            .map(|entry| entry.minute as i32)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|anchor| (anchor, near(anchor)))
            .max_by(|a, b| days(&a.1).cmp(&days(&b.1)).then(b.0.cmp(&a.0)));
        let Some((anchor, members)) = best else {
            continue;
        };
        let day_count = days(&members);
        if day_count < MIN_DAYS {
            continue;
        }
        let mut offsets: Vec<i32> = members
            .iter()
            .map(|entry| offset(anchor, entry.minute as i32))
            .collect();
        offsets.sort();
        let center = anchor + offsets[offsets.len() / 2];
        let distance = offset(center, now_minute);
        if distance.abs() > HALF_WINDOW {
            continue;
        }
        let latest = members
            .iter()
            .max_by_key(|entry| entry.at_ms)
            .expect("at least three members");
        let kind = if weekend(today) {
            "weekend days"
        } else {
            "weekdays"
        };
        found.push((
            distance.abs(),
            RoutineOffer {
                id: id.to_string(),
                label: latest.label.clone(),
                set_id: latest.set_id.clone(),
                memory_ids: latest.memory_ids.clone(),
                reason: format!("Opened on {day_count} {kind} around {}", clock(center)),
                due_now: distance >= -DUE_LEAD,
            },
        ));
    }
    found.sort_by(|a, b| {
        b.1.due_now
            .cmp(&a.1.due_now)
            .then(a.0.cmp(&b.0))
            .then(a.1.id.cmp(&b.1.id))
    });
    found.into_iter().map(|(_, offer)| offer).collect()
}

static WRITE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

fn private(state: &AppState) -> bool {
    state.is_incognito.load(std::sync::atomic::Ordering::SeqCst)
}

/// Logs one opening. Nothing is written while FNDR is private.
pub fn record(
    state: &AppState,
    set_id: Option<&str>,
    memory_ids: &[String],
    label: &str,
    source: Source,
) {
    if private(state) || memory_ids.is_empty() {
        return;
    }
    let now = chrono::Local::now();
    let entry = opening(
        set_id,
        memory_ids,
        label,
        source,
        now.naive_local(),
        now.timestamp_millis(),
    );
    let _held = WRITE.lock();
    let result = state
        .state_store
        .load_json::<Vec<Opening>>(LOG_KEY)
        .and_then(|log| {
            let mut log = log.unwrap_or_default();
            append(&mut log, entry);
            state.state_store.save_json(LOG_KEY, &log)
        });
    if let Err(error) = result {
        tracing::warn!(%error, "workset:routine_log_failed");
    }
}

/// Hides an offer for the rest of today.
pub fn dismiss(store: &StateStore, id: &str, today: NaiveDate) -> Result<(), String> {
    let _held = WRITE.lock();
    let mut dismissed: Vec<Dismissal> = store.load_json(DISMISSED_KEY)?.unwrap_or_default();
    dismissed.retain(|d| d.day == today && d.id != id);
    dismissed.push(Dismissal {
        id: id.to_string(),
        day: today,
    });
    store.save_json(DISMISSED_KEY, &dismissed)
}

/// The offers for now, each re-checked: a saved set that was deleted drops
/// out, a renamed one shows its name, and memories now private, blocklisted
/// or deleted are left out. None while FNDR is private.
pub async fn current(state: &AppState) -> Result<Vec<RoutineOffer>, String> {
    if private(state) {
        return Ok(Vec::new());
    }
    let log: Vec<Opening> = state.state_store.load_json(LOG_KEY)?.unwrap_or_default();
    let dismissed: Vec<Dismissal> = state
        .state_store
        .load_json(DISMISSED_KEY)?
        .unwrap_or_default();
    let now = chrono::Local::now();
    let found = offers(&log, &dismissed, now.naive_local(), now.timestamp_millis());
    if found.is_empty() {
        return Ok(found);
    }
    let sets = named::load(&state.state_store)?;
    let ids: Vec<String> = found
        .iter()
        .flat_map(|offer| offer.memory_ids.iter().cloned())
        .chain(sets.iter().flat_map(|set| set.memory_ids.iter().cloned()))
        .collect();
    let records = state
        .store
        .get_memories_by_ids(&ids)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    let blocklist = state.config.read().blocklist.clone();
    Ok(found
        .into_iter()
        .filter_map(|mut offer| {
            if let Some(set_id) = &offer.set_id {
                let set = sets.iter().find(|set| &set.id == set_id)?;
                let shown = named::present(set, &records, &blocklist);
                offer.label = shown.name;
                offer.memory_ids = shown.memory_ids;
            } else {
                offer.memory_ids.retain(|id| {
                    records
                        .get(id)
                        .is_some_and(|record| eligible(record, &blocklist))
                });
            }
            (!offer.memory_ids.is_empty()).then_some(offer)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONDAY: (i32, u32, u32) = (2026, 10, 5);

    fn at(day: NaiveDate, hour: u32, minute: u32) -> NaiveDateTime {
        day.and_hms_opt(hour, minute, 0).unwrap()
    }

    fn day(offset_days: i64) -> NaiveDate {
        NaiveDate::from_ymd_opt(MONDAY.0, MONDAY.1, MONDAY.2).unwrap()
            + chrono::Duration::days(offset_days)
    }

    /// Milliseconds for a local time in a zone `utc_offset_hours` from UTC.
    fn ms(local: NaiveDateTime, utc_offset_hours: i64) -> i64 {
        (local - chrono::Duration::hours(utc_offset_hours))
            .and_utc()
            .timestamp_millis()
    }

    fn opened(set: &str, local: NaiveDateTime) -> Opening {
        opening(
            Some(set),
            &["m1".to_string(), "m2".to_string()],
            set,
            Source::Home,
            local,
            ms(local, -6),
        )
    }

    fn ids_of(found: &[RoutineOffer]) -> Vec<&str> {
        found.iter().map(|offer| offer.id.as_str()).collect()
    }

    #[test]
    fn three_weekdays_around_nine_make_an_offer_inside_the_window_only() {
        // Monday, Tuesday, Wednesday around 9; today is the next Monday.
        let log = vec![
            opened("standup", at(day(0), 8, 50)),
            opened("standup", at(day(1), 9, 10)),
            opened("standup", at(day(2), 9, 25)),
        ];
        let today = day(7);
        let found = offers(&log, &[], at(today, 9, 0), ms(at(today, 9, 0), -6));
        assert_eq!(ids_of(&found), ["set:standup"]);
        assert!(found[0].due_now);
        assert_eq!(found[0].set_id.as_deref(), Some("standup"));
        assert_eq!(found[0].reason, "Opened on 3 weekdays around 9:10 AM");
        let early = offers(&log, &[], at(today, 8, 20), ms(at(today, 8, 20), -6));
        assert_eq!(ids_of(&early), ["set:standup"]);
        assert!(
            !early[0].due_now,
            "fifty minutes early is coming up, not due"
        );
        for (hour, minute) in [(7, 50), (10, 30), (15, 0)] {
            let now = at(today, hour, minute);
            assert!(
                offers(&log, &[], now, ms(now, -6)).is_empty(),
                "{hour}:{minute}"
            );
        }
    }

    #[test]
    fn two_days_or_scattered_times_or_a_weekend_are_not_a_routine() {
        let today = day(7);
        let now = at(today, 9, 0);
        let two_days = vec![
            opened("a", at(day(0), 9, 0)),
            opened("a", at(day(0), 9, 30)),
            opened("a", at(day(1), 9, 0)),
        ];
        assert!(offers(&two_days, &[], now, ms(now, -6)).is_empty());
        let scattered = vec![
            opened("b", at(day(0), 7, 0)),
            opened("b", at(day(1), 9, 30)),
            opened("b", at(day(2), 12, 0)),
        ];
        assert!(offers(&scattered, &[], now, ms(now, -6)).is_empty());
        let weekend = vec![
            opened("c", at(day(-2), 9, 0)),
            opened("c", at(day(-1), 9, 0)),
            opened("c", at(day(5), 9, 0)),
        ];
        assert!(
            offers(&weekend, &[], now, ms(now, -6)).is_empty(),
            "weekend openings say nothing about a Monday"
        );
        let saturday = at(day(13), 9, 0);
        assert_eq!(
            ids_of(&offers(&weekend, &[], saturday, ms(saturday, -6))),
            ["set:c"]
        );
    }

    #[test]
    fn opened_or_dismissed_today_means_no_offer_until_tomorrow() {
        let mut log = vec![
            opened("standup", at(day(0), 9, 0)),
            opened("standup", at(day(1), 9, 0)),
            opened("standup", at(day(2), 9, 0)),
        ];
        let today = day(3);
        let now = at(today, 9, 5);
        let dismissed = vec![Dismissal {
            id: "set:standup".into(),
            day: today,
        }];
        assert!(offers(&log, &dismissed, now, ms(now, -6)).is_empty());
        let yesterday = vec![Dismissal {
            id: "set:standup".into(),
            day: day(2),
        }];
        assert_eq!(offers(&log, &yesterday, now, ms(now, -6)).len(), 1);
        log.push(opened("standup", at(today, 8, 0)));
        assert!(offers(&log, &[], now, ms(now, -6)).is_empty());
    }

    #[test]
    fn a_window_across_midnight_counts_both_sides() {
        let log = vec![
            opened("late", at(day(0), 23, 40)),
            opened("late", at(day(2), 0, 10)),
            opened("late", at(day(3), 23, 55)),
        ];
        let now = at(day(8), 23, 50);
        let found = offers(&log, &[], now, ms(now, -6));
        assert_eq!(ids_of(&found), ["set:late"]);
        assert_eq!(found[0].reason, "Opened on 3 weekdays around 11:55 PM");
        let after = at(day(9), 0, 20);
        assert_eq!(offers(&log, &[], after, ms(after, -6)).len(), 1);
    }

    #[test]
    fn local_time_is_kept_across_a_daylight_saving_change() {
        // 9:00 local on both sides of the change: an hour apart in UTC.
        let log = vec![
            opening(
                None,
                &["b".into(), "a".into()],
                "Lab",
                Source::Notch,
                at(day(-14), 9, 0),
                ms(at(day(-14), 9, 0), -6),
            ),
            opening(
                None,
                &["a".into(), "b".into()],
                "Lab",
                Source::Home,
                at(day(-7), 9, 0),
                ms(at(day(-7), 9, 0), -6),
            ),
            opening(
                None,
                &["a".into(), "b".into()],
                "Lab",
                Source::Home,
                at(day(0), 9, 0),
                ms(at(day(0), 9, 0), -7),
            ),
        ];
        let now = at(day(7), 9, 0);
        let found = offers(&log, &[], now, ms(now, -7));
        assert_eq!(
            ids_of(&found),
            ["ids:a,b"],
            "the same memories in any order"
        );
        assert_eq!(found[0].memory_ids, ["a", "b"]);
        assert_eq!(found[0].set_id, None);
    }

    #[test]
    fn the_log_drops_old_entries_and_keeps_at_most_four_hundred() {
        let mut log = Vec::new();
        let start = at(day(-70), 9, 0);
        append(&mut log, opened("old", start));
        for n in 0..MAX_ENTRIES as i64 + 5 {
            let local = at(day(0), 9, 0) + chrono::Duration::minutes(n);
            append(&mut log, opened("new", local));
        }
        assert_eq!(log.len(), MAX_ENTRIES);
        assert!(log.iter().all(|entry| entry.signature == "set:new"));
        assert_eq!(log[0].minute, 9 * 60 + 5, "the oldest go first");
        let json = serde_json::to_value(&log[0]).unwrap();
        assert_eq!(json["source"], "home");
        assert_eq!(json["day"], "2026-10-05");
    }

    #[test]
    fn an_offer_serializes_in_the_shape_the_home_card_reads() {
        let offer = RoutineOffer {
            id: "ids:a".into(),
            label: "Lab".into(),
            set_id: None,
            memory_ids: vec!["a".into()],
            reason: "r".into(),
            due_now: true,
        };
        let value = serde_json::to_value(&offer).unwrap();
        assert_eq!(value["memoryIds"][0], "a");
        assert_eq!(value["dueNow"], true);
        assert!(value.get("setId").is_none());
    }
}

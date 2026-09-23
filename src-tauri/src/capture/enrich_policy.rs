//! Enrichment scheduling (MEM-06): whether to spend local-model time on a
//! captured frame right now, defer it, or skip straight to the deterministic
//! fallback. Order matters: system health first, then value, then backlog.

/// What to do with the local model for one captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichNow {
    Yes,
    /// Keep the frame in the queue and try later.
    Defer(&'static str),
    /// Do not spend model time on this frame; store the deterministic fallback.
    Skip(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct EnrichInputs {
    pub host_memory_high: bool,
    pub on_battery: bool,
    pub queue_depth: usize,
    pub text_chars: usize,
    pub is_duplicate_story: bool,
}

pub const MIN_TEXT_CHARS: usize = 40;

/// Order matters: system health first, then value, then backlog.
pub fn should_enrich_now(i: &EnrichInputs, max_queue: usize) -> EnrichNow {
    if i.host_memory_high {
        return EnrichNow::Defer("host_memory_high");
    }
    if i.is_duplicate_story {
        return EnrichNow::Skip("duplicate_story");
    }
    if i.text_chars < MIN_TEXT_CHARS {
        return EnrichNow::Skip("too_little_text");
    }
    if i.on_battery && i.queue_depth > 0 {
        return EnrichNow::Defer("battery_backlog");
    }
    if i.queue_depth >= max_queue {
        return EnrichNow::Defer("queue_full");
    }
    EnrichNow::Yes
}

/// `mem.enrich.*` counter name for a scheduling outcome, for
/// `runtime_metrics::bump`.
pub fn enrich_outcome_counter(outcome: EnrichNow) -> String {
    match outcome {
        EnrichNow::Yes => "mem.enrich.yes".to_string(),
        EnrichNow::Defer(reason) => format!("mem.enrich.defer.{reason}"),
        EnrichNow::Skip(reason) => format!("mem.enrich.skip.{reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> EnrichInputs {
        EnrichInputs {
            host_memory_high: false,
            on_battery: false,
            queue_depth: 0,
            text_chars: 500,
            is_duplicate_story: false,
        }
    }

    #[test]
    fn healthy_valuable_frame_is_enriched() {
        assert_eq!(should_enrich_now(&base(), 4), EnrichNow::Yes);
    }

    #[test]
    fn memory_pressure_defers_even_a_duplicate() {
        let i = EnrichInputs {
            host_memory_high: true,
            is_duplicate_story: true,
            ..base()
        };
        assert_eq!(should_enrich_now(&i, 4), EnrichNow::Defer("host_memory_high"));
    }

    #[test]
    fn duplicates_and_thin_frames_are_skipped_not_deferred() {
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    is_duplicate_story: true,
                    ..base()
                },
                4
            ),
            EnrichNow::Skip("duplicate_story")
        );
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    text_chars: 39,
                    ..base()
                },
                4
            ),
            EnrichNow::Skip("too_little_text")
        );
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    text_chars: 40,
                    ..base()
                },
                4
            ),
            EnrichNow::Yes
        );
    }

    #[test]
    fn battery_with_a_backlog_and_a_full_queue_both_defer() {
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    on_battery: true,
                    queue_depth: 1,
                    ..base()
                },
                4
            ),
            EnrichNow::Defer("battery_backlog")
        );
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    on_battery: true,
                    queue_depth: 0,
                    ..base()
                },
                4
            ),
            EnrichNow::Yes
        );
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    queue_depth: 4,
                    ..base()
                },
                4
            ),
            EnrichNow::Defer("queue_full")
        );
        assert_eq!(
            should_enrich_now(
                &EnrichInputs {
                    queue_depth: 3,
                    ..base()
                },
                4
            ),
            EnrichNow::Yes
        );
    }

    #[test]
    fn counter_names_match_the_mem_enrich_prefix_convention() {
        assert_eq!(enrich_outcome_counter(EnrichNow::Yes), "mem.enrich.yes");
        assert_eq!(
            enrich_outcome_counter(EnrichNow::Defer("host_memory_high")),
            "mem.enrich.defer.host_memory_high"
        );
        assert_eq!(
            enrich_outcome_counter(EnrichNow::Skip("duplicate_story")),
            "mem.enrich.skip.duplicate_story"
        );
    }
}

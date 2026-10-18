use super::system_metrics::ProcessCounters;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FootprintReport {
    pub window_secs: f64,
    pub avg_cpu_percent: f64,
    pub wakeups_per_sec: f64,
    pub billed_system_ms_per_minute: f64,
    pub rss_bytes: u64,
    pub phys_footprint_bytes: u64,
    pub bytes_written_per_minute: f64,
    pub pixel_captures: u64,
    pub pixel_captures_per_hour: f64,
    pub stored_frames: u64,
}

impl FootprintReport {
    pub fn from_counters(
        before: ProcessCounters,
        after: ProcessCounters,
        window: Duration,
        pixel_captures: u64,
        stored_frames: u64,
    ) -> Self {
        let window_secs = window.as_secs_f64();
        let rate = |count: u64| {
            if window_secs > 0.0 {
                count as f64 / window_secs
            } else {
                0.0
            }
        };
        let wakeups = after.idle_wakeups.saturating_sub(before.idle_wakeups)
            + after
                .interrupt_wakeups
                .saturating_sub(before.interrupt_wakeups);
        Self {
            window_secs,
            avg_cpu_percent: rate(after.cpu_time_ms.saturating_sub(before.cpu_time_ms)) / 10.0,
            wakeups_per_sec: rate(wakeups),
            billed_system_ms_per_minute: rate(
                after
                    .billed_system_time_ns
                    .saturating_sub(before.billed_system_time_ns),
            ) * 60.0
                / 1_000_000.0,
            rss_bytes: after.rss_bytes,
            phys_footprint_bytes: after.phys_footprint_bytes,
            bytes_written_per_minute: rate(
                after
                    .disk_bytes_written
                    .saturating_sub(before.disk_bytes_written),
            ) * 60.0,
            pixel_captures,
            pixel_captures_per_hour: rate(pixel_captures) * 3_600.0,
            stored_frames,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub max_cpu_percent: f64,
    pub max_wakeups_per_sec: f64,
    pub max_pixel_captures_per_hour: f64,
}

impl Budget {
    pub fn check(&self, report: &FootprintReport) -> Result<(), Vec<String>> {
        let mut failures = Vec::new();
        if report.avg_cpu_percent > self.max_cpu_percent {
            failures.push(format!(
                "CPU {:.2}% exceeds {:.2}% of one core",
                report.avg_cpu_percent, self.max_cpu_percent
            ));
        }
        if report.wakeups_per_sec > self.max_wakeups_per_sec {
            failures.push(format!(
                "wakeups {:.2}/s exceed {:.2}/s",
                report.wakeups_per_sec, self.max_wakeups_per_sec
            ));
        }
        if report.pixel_captures_per_hour > self.max_pixel_captures_per_hour {
            failures.push(format!(
                "pixel captures {:.0}/h exceed {:.0}/h",
                report.pixel_captures_per_hour, self.max_pixel_captures_per_hour
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_counter_deltas_and_the_last_memory_sample() {
        let before = ProcessCounters {
            cpu_time_ms: 100,
            idle_wakeups: 10,
            interrupt_wakeups: 4,
            billed_system_time_ns: 100_000_000,
            disk_bytes_written: 2_000,
            rss_bytes: 100,
            phys_footprint_bytes: 80,
        };
        let after = ProcessCounters {
            cpu_time_ms: 1_100,
            idle_wakeups: 30,
            interrupt_wakeups: 14,
            billed_system_time_ns: 200_000_000,
            disk_bytes_written: 5_000,
            rss_bytes: 200,
            phys_footprint_bytes: 150,
        };
        let report = FootprintReport::from_counters(before, after, Duration::from_secs(10), 5, 2);
        assert_eq!(report.avg_cpu_percent, 10.0);
        assert_eq!(report.wakeups_per_sec, 3.0);
        assert_eq!(report.bytes_written_per_minute, 18_000.0);
        assert_eq!(report.billed_system_ms_per_minute, 600.0);
        assert_eq!(report.rss_bytes, 200);
        assert_eq!(report.phys_footprint_bytes, 150);
        assert_eq!(report.pixel_captures_per_hour, 1_800.0);
        assert_eq!(report.stored_frames, 2);
    }

    #[test]
    fn budget_reports_each_exceeded_limit() {
        let report = FootprintReport {
            avg_cpu_percent: 6.0,
            wakeups_per_sec: 4.0,
            pixel_captures_per_hour: 200.0,
            ..FootprintReport::default()
        };
        let budget = Budget {
            max_cpu_percent: 5.0,
            max_wakeups_per_sec: 3.0,
            max_pixel_captures_per_hour: 180.0,
        };
        let failures = budget.check(&report).unwrap_err();
        assert_eq!(failures.len(), 3);
        assert!(failures.iter().any(|s| s.contains("CPU")));
        assert!(failures.iter().any(|s| s.contains("wakeups")));
        assert!(failures.iter().any(|s| s.contains("pixel captures")));
    }

    #[test]
    fn zero_window_and_counter_reset_do_not_invent_usage() {
        let before = ProcessCounters {
            cpu_time_ms: 100,
            idle_wakeups: 10,
            interrupt_wakeups: 10,
            disk_bytes_written: 100,
            ..ProcessCounters::default()
        };
        let after = ProcessCounters::default();
        let report = FootprintReport::from_counters(before, after, Duration::ZERO, 0, 0);
        assert_eq!(report.avg_cpu_percent, 0.0);
        assert_eq!(report.wakeups_per_sec, 0.0);
        assert_eq!(report.bytes_written_per_minute, 0.0);
        assert!(Budget {
            max_cpu_percent: 5.0,
            max_wakeups_per_sec: 3.0,
            max_pixel_captures_per_hour: 180.0,
        }
        .check(&report)
        .is_ok());
    }
}

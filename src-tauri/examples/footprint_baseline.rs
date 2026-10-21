//! Synthetic capture cost probe. Reads only the checked-in screen fixtures.
//! It does not capture the display or inspect the current foreground app.

use fndr_lib::capture::{
    bgra_to_png, PerceptualHasher, ScreenObservation, TriggerGate, TriggerPolicy,
};
use fndr_lib::config::Config;
use fndr_lib::ocr::OcrEngine;
use fndr_lib::telemetry::footprint::{Budget, FootprintReport};
use fndr_lib::telemetry::system_metrics::{
    latest_snapshot_for_display, process_counters, refresh_once, sampler_plan,
};
use image::imageops::FilterType;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const WIDTH: usize = 2_880;
const HEIGHT: usize = 1_800;

struct Options {
    window: Duration,
    profile: String,
    policy: String,
    sampler: String,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            window: Duration::from_secs(120),
            profile: "idle".to_string(),
            policy: "event".to_string(),
            sampler: "background".to_string(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?;
            match flag.as_str() {
                "--window-secs" => {
                    let secs: u64 = value.parse().map_err(|_| "invalid window seconds")?;
                    if secs == 0 {
                        return Err("window seconds must be positive".to_string());
                    }
                    options.window = Duration::from_secs(secs);
                }
                "--profile" if matches!(value.as_str(), "idle" | "active") => {
                    options.profile = value
                }
                "--policy" if matches!(value.as_str(), "timer" | "event") => options.policy = value,
                "--sampler" if matches!(value.as_str(), "background" | "watched") => {
                    options.sampler = value
                }
                _ => return Err(format!("invalid option {flag} {value}")),
            }
        }
        Ok(options)
    }
}

struct SyntheticScreen {
    bgra: Vec<u8>,
    bundle_id: &'static str,
    title: &'static str,
}

fn fixture_screen(
    file: &str,
    bundle_id: &'static str,
    title: &'static str,
) -> Result<SyntheticScreen, Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens")
        .join(file);
    let rgba = image::open(path)?
        .resize_exact(WIDTH as u32, HEIGHT as u32, FilterType::Nearest)
        .to_rgba8();
    let mut bgra = rgba.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(SyntheticScreen {
        bgra,
        bundle_id,
        title,
    })
}

fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    let trigger_policy = TriggerPolicy::from_config(&config);
    let screens = [
        fixture_screen("editor-01.png", "com.microsoft.VSCode", "sum.js")?,
        fixture_screen("editor-02.png", "com.microsoft.VSCode", "sum.js")?,
        fixture_screen("terminal-01.png", "com.apple.Terminal", "cargo test")?,
        fixture_screen(
            "browser_article-01.png",
            "com.google.Chrome",
            "Getting Started",
        )?,
    ];
    let ocr = OcrEngine::new()?;
    let mut hasher = PerceptualHasher::new();
    let mut gate = TriggerGate::new();
    let poll_every = if options.profile == "idle" { 5.0 } else { 2.0 };
    let mut last_input_secs = -100.0;
    let mut last_input_slot = None;
    let mut next_poll_secs = 0.0;
    let mut next_sample_secs = 0.0;
    let mut pixel_captures = 0;
    let mut novel_frames = 0;
    let mut ocr_calls = 0;
    let before = process_counters();
    let started = Instant::now();

    while started.elapsed() < options.window {
        let now_secs = started.elapsed().as_secs_f64();
        if now_secs >= next_sample_secs {
            if options.sampler == "watched" {
                let _ = latest_snapshot_for_display();
            }
            refresh_once(Vec::new());
            let now_ms = chrono::Utc::now().timestamp_millis();
            let last_display_read_ms = if options.sampler == "watched" {
                now_ms
            } else {
                0
            };
            let plan = sampler_plan(now_ms, last_display_read_ms);
            next_sample_secs = now_secs + plan.next_interval.as_secs_f64();
        }
        if now_secs >= next_poll_secs {
            let second = now_secs as u64;
            if options.profile == "active" {
                let slot = second % 30;
                if matches!(slot, 4 | 6 | 8) && last_input_slot != Some(second) {
                    last_input_secs = now_secs;
                    last_input_slot = Some(second);
                }
            }
            let screen_index = if options.profile == "idle" {
                0
            } else {
                ((second / 30) as usize) % screens.len()
            };
            let screen = &screens[screen_index];
            let observation = ScreenObservation {
                bundle_id: Some(screen.bundle_id),
                window_title: screen.title,
                url: None,
                input_idle_secs: now_secs - last_input_secs,
            };
            let capture = options.policy == "timer"
                || gate
                    .decide(now_secs, &observation, &trigger_policy)
                    .is_some();
            if capture {
                pixel_captures += 1;
                let png = bgra_to_png(WIDTH, HEIGHT, WIDTH * 4, &screen.bgra)?;
                if !hasher.check(&png, config.dedupe_threshold).is_duplicate {
                    novel_frames += 1;
                    let _ = ocr.recognize(&png)?;
                    ocr_calls += 1;
                }
            }
            next_poll_secs = now_secs + poll_every;
        }
        let next_secs = next_poll_secs.min(next_sample_secs);
        let remaining = (next_secs - started.elapsed().as_secs_f64()).max(0.0);
        thread::sleep(Duration::from_secs_f64(remaining.min(0.1)));
    }

    let elapsed = started.elapsed();
    let after = process_counters();
    let report =
        FootprintReport::from_counters(before, after, elapsed, pixel_captures, novel_frames);
    let budget = Budget {
        max_cpu_percent: if options.profile == "idle" { 2.5 } else { 3.0 },
        max_wakeups_per_sec: 12.0,
        max_pixel_captures_per_hour: if options.profile == "idle" {
            180.0
        } else {
            600.0
        },
    };
    println!("| profile | policy | sampler | window s | CPU % of core | RSS MiB | physical MiB | wakeups/s | billed ms/min | bytes written/min | pixel captures | captures/h | novel frames | OCR calls |");
    println!("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    println!(
        "| {} | {} | {} | {:.1} | {:.2} | {:.1} | {:.1} | {:.2} | {:.0} | {:.0} | {} | {:.0} | {} | {} |",
        options.profile,
        options.policy,
        options.sampler,
        report.window_secs,
        report.avg_cpu_percent,
        report.rss_bytes as f64 / 1_048_576.0,
        report.phys_footprint_bytes as f64 / 1_048_576.0,
        report.wakeups_per_sec,
        report.billed_system_ms_per_minute,
        report.bytes_written_per_minute,
        report.pixel_captures,
        report.pixel_captures_per_hour,
        report.stored_frames,
        ocr_calls,
    );
    if let Err(failures) = budget.check(&report) {
        return Err(failures.join("; ").into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(Options::parse()?)
}

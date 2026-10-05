//! VS-15 manual matrix helper. Every 3 seconds prints the frontmost app's name
//! and how much Accessibility text `focused_text` returned: counts only, never
//! the text itself. Bring each app in the matrix to the front while it runs.
//!
//! cargo run --example ax_text_probe -- [seconds]

use objc2_app_kit::NSWorkspace;
use std::time::{Duration, Instant};

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    if !fndr_lib::accessibility::has_accessibility_permission() {
        eprintln!("Accessibility permission missing for this terminal; grant it and rerun.");
        return;
    }
    println!("app,chars,nodes,secure_skipped,web_area,truncated,ms,roles");
    let end = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < end {
        let front = unsafe {
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .map(|app| {
                    (
                        app.processIdentifier(),
                        app.localizedName()
                            .map(|n| n.to_string())
                            .unwrap_or_default(),
                    )
                })
        };
        if let Some((pid, name)) = front {
            match fndr_lib::accessibility::focused_text(pid, 20_000) {
                Some(t) => println!(
                    "{name},{},{},{},{},{},{},{:?}",
                    t.text.chars().count(),
                    t.nodes_visited,
                    t.secure_fields_skipped,
                    t.from_web_area,
                    t.truncated,
                    t.elapsed_ms,
                    t.role_counts
                ),
                None => println!("{name},0,0,0,false,false,0,none"),
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

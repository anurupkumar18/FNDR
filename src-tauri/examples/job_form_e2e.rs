//! Fills the mock job application page (fake values) through the same field
//! discovery, selection rules and writers Smart Fill uses, then reads the page
//! back. Brings the browser to the front.
//!
//! cargo run --example job_form_e2e -- <pid> <app name>

use fndr_lib::accessibility::form::{current_value, discover, write, FieldKind};
use fndr_lib::smart_fill::form::{model_fields, upload_fields};
use std::time::Duration;

fn main() {
    let pid: i32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("pid");
    let app = std::env::args().nth(2).expect("app name");
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(format!("tell application \"{app}\" to activate"))
        .status();
    std::thread::sleep(Duration::from_millis(900));

    let fields = discover(pid, Duration::from_millis(1500), 40_000);
    let offered = model_fields(&fields);
    let uploads = upload_fields(&fields);
    println!(
        "{} fields found, {} offered to the model, {} file boxes",
        fields.len(),
        offered.len(),
        uploads.len()
    );
    let withheld: Vec<&str> = fields
        .iter()
        .filter(|f| f.kind != FieldKind::Upload && !offered.iter().any(|o| o.index == f.index))
        .map(|f| f.label.as_str())
        .collect();
    println!("withheld from the model: {withheld:?}");
    for (index, label) in &uploads {
        println!("file box {index}: {label}");
    }

    // Fake answers keyed by label text.
    let answers: &[(&str, &str)] = &[
        ("First name", "Sam"),
        ("Last name", "Example"),
        ("Email", "sam@example.com"),
        ("LinkedIn URL", "https://www.linkedin.com/in/sam-example"),
        ("Current company", "Example Corp"),
        ("Current job title", "Software Engineer"),
        ("Years of experience", "2-3"),
        ("Earliest start date", "06012030"),
        ("How did you hear about us?", "Referral"),
        (
            "legally authorized to work in the United States? > Yes",
            "select",
        ),
        ("require visa sponsorship? > No", "select"),
    ];
    let mut written = 0;
    for info in &offered {
        let plain = info.label.replace(" *", "").replace("*", "");
        let Some((_, text)) = answers.iter().find(|(label, _)| {
            if info.kind == FieldKind::Option {
                plain.contains(label)
            } else {
                plain == *label
            }
        }) else {
            continue;
        };
        let Some(field) = fields.iter().find(|f| f.index == info.index) else {
            continue;
        };
        match write(field, text) {
            Ok(()) => written += 1,
            Err(err) => println!("could not write {:?}: {err}", info.label),
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    println!("wrote {written} fields");
    std::thread::sleep(Duration::from_millis(500));
    for field in &discover(pid, Duration::from_millis(1500), 40_000) {
        if matches!(field.kind, FieldKind::Option) {
            println!(
                "option {:<75} checked={}",
                format!("{} > {}", field.group, field.label),
                field.checked
            );
        } else if field.kind != FieldKind::Upload {
            println!("{:<34} => {:?}", field.label, current_value(field));
        }
    }
}

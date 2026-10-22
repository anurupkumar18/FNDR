//! Writes fake values into the open form of an app and reads the fields back,
//! to check the Smart Fill writer against a real page (the Autofill QA
//! specimen). Brings the app to the front.
//!
//! cargo run --example form_fill_e2e -- <pid> <app name>

use fndr_lib::accessibility::form::{current_value, discover, write, FieldKind};
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

    let fake: &[(&str, &str)] = &[
        ("given-name", "Testy"),
        ("additional-name", "Q"),
        ("family-name", "Specimen"),
        ("name", "Testy Q Specimen"),
        ("bday", "09092003"),
        ("email", "testy@example.com"),
        ("tel", "(385) 555-0100"),
        ("sex", "Female"),
        ("university", "Example University"),
        ("grad-year", "2026"),
        ("major", "Computer Science"),
        ("degree", "Bachelor"),
        ("gpa", "3.80"),
        ("passport-number", "X1234567"),
        ("passport-expiry", "06302031"),
        ("address-line1", "1 Test Street"),
        ("address-level2", "Springfield"),
        ("address-level1", "UT"),
        ("postal-code", "84101"),
        ("country", "United States"),
    ];
    let fields = discover(pid, Duration::from_millis(1500), 40_000);
    println!("{} fields found", fields.len());
    let started = std::time::Instant::now();
    let mut written = 0;
    for field in &fields {
        let Some((_, text)) = fake.iter().find(|(id, _)| *id == field.dom_id) else {
            continue;
        };
        match write(field, text) {
            Ok(()) => written += 1,
            Err(err) => println!("could not write {:?}: {err}", field.dom_id),
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    println!("wrote {written} fields in {:?}", started.elapsed());
    std::thread::sleep(Duration::from_millis(500));
    let after = discover(pid, Duration::from_millis(1500), 40_000);
    for field in &after {
        let kind = match field.kind {
            FieldKind::Text => "text",
            FieldKind::Date => "date",
            FieldKind::Select => "select",
        };
        println!("{:<16} {:<6} => {:?}", field.dom_id, kind, current_value(field));
    }
}

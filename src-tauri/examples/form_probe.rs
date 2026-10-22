//! Smart Fill form discovery probe: lists the fillable fields the front window
//! of an app exposes (kind, label, id, option count, whether it already holds
//! a value). Never prints values.
//!
//! cargo run --example form_probe -- <pid of the app, for example from `pgrep -x Dia`>

use std::time::Duration;

fn main() {
    let Some(pid) = std::env::args().nth(1).and_then(|s| s.parse::<i32>().ok()) else {
        eprintln!("usage: form_probe <pid>");
        return;
    };
    let started = std::time::Instant::now();
    let fields = fndr_lib::accessibility::form::discover(pid, Duration::from_millis(1500), 40_000);
    println!("{} fields in {:?}", fields.len(), started.elapsed());
    for f in &fields {
        println!(
            "{:>2} {:?} label={:?} ph={:?} id={:?} options={} has_value={}",
            f.index,
            f.kind,
            f.label,
            f.placeholder,
            f.dom_id,
            f.options.len(),
            !f.value.trim().is_empty()
        );
    }
}

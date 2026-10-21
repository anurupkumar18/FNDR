//! Prints the Notch Do runs the person rated as not working, as draft cases
//! for `scripts/operator/notch-do-cases.json`. Each holds the request and
//! what FNDR planned; fill in `route` (and `plan`) with what should have
//! happened, check the request holds nothing private, then add it to the
//! suite. Reads only; the drafts stay in the profile.
//!
//! Usage: cargo run --example notch_do_case_drafts -- --data-dir <profile>

fn data_dir() -> Option<std::path::PathBuf> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == "--data-dir" {
            return args.next().map(std::path::PathBuf::from);
        }
    }
    None
}

fn main() -> Result<(), String> {
    let dir = data_dir().ok_or("--data-dir <profile> is required")?;
    let store = fndr_lib::storage::StateStore::new(&dir).map_err(|error| error.to_string())?;
    let drafts = fndr_lib::operator::runs::case_drafts(&store)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&drafts).map_err(|error| error.to_string())?
    );
    eprintln!(
        "{} draft case(s) from runs rated as not working",
        drafts.len()
    );
    Ok(())
}

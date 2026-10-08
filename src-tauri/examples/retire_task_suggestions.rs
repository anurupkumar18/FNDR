//! One-time cleanup of task suggestions made before a suggestion had to
//! quote the screen (see `tasks::suggest`). Dismisses open suggestions that
//! are no longer offered: unquoted ones and ones older than the lifespan.
//! The person's own, accepted, meeting and completed tasks are untouched,
//! and nothing is deleted. Dry run unless `--apply` is given. Refuses the
//! real profile unless `--allow-real-profile` is also given. Prints counts
//! only, never task text.
//! Usage: cargo run --example retire_task_suggestions -- --data-dir <profile> [--apply]

use fndr_lib::storage::Store;
use fndr_lib::tasks::suggest::{is_suggestion, retire_unoffered};
use std::path::PathBuf;

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == name {
            return args.next();
        }
    }
    None
}

fn flag(name: &str) -> bool {
    std::env::args().any(|current| current == name)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?).canonicalize()?;
    let apply = flag("--apply");
    let real = dirs::data_dir().map(|dir| dir.join("com.fndr.app"));
    let is_real = real
        .and_then(|path| path.canonicalize().ok())
        .is_some_and(|real| data_dir.starts_with(&real));
    if is_real && !flag("--allow-real-profile") {
        return Err("refusing the real FNDR profile without --allow-real-profile".into());
    }

    let store = Store::new(&data_dir)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let report = runtime.block_on(async {
        let mut tasks = store.list_tasks().await.map_err(|e| e.to_string())?;
        let open = |tasks: &[fndr_lib::storage::Task]| {
            tasks
                .iter()
                .filter(|task| !task.is_completed && !task.is_dismissed)
                .count()
        };
        let open_before = open(&tasks);
        let open_suggestions_before = tasks
            .iter()
            .filter(|task| !task.is_completed && !task.is_dismissed && is_suggestion(task))
            .count();
        let retired = retire_unoffered(&mut tasks, chrono::Utc::now().timestamp_millis());
        let open_after = open(&tasks);
        if apply && retired > 0 {
            store
                .upsert_tasks(&tasks)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(serde_json::json!({
            "dry_run": !apply,
            "tasks_stored": tasks.len(),
            "open_before": open_before,
            "open_suggestions_before": open_suggestions_before,
            "retired": retired,
            "open_after": open_after,
            "written": apply && retired > 0,
        }))
    })?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

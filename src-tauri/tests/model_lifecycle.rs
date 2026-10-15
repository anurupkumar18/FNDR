//! Real-model process checks. These read model weights, not captures or stores.
use fndr_lib::inference::InferenceEngine;
use std::{ffi::OsStr, path::PathBuf, process::Command};

#[test]
#[ignore = "loads the installed GGUF; run explicitly and serially"]
fn real_text_engine_drop_exits_cleanly() {
    const CHILD: &str = "FNDR_MODEL_DROP_TEST_CHILD";
    const DROPPED: &str = "text engine and runtime dropped";

    if std::env::var_os(CHILD).as_deref() == Some(OsStr::new("1")) {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let app_dir = std::env::var_os("FNDR_INFERENCE_TEST_APP_DIR").map(PathBuf::from);
        let engine = runtime
            .block_on(InferenceEngine::new(app_dir, None))
            .expect("installed text model");
        drop(engine);
        drop(runtime);
        println!("{DROPPED}");
        return;
    }

    // A test-body assertion cannot observe a destructor abort after the test
    // harness finishes. Keep model initialization in a separate real process.
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "real_text_engine_drop_exits_cleanly",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("model lifecycle child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(DROPPED),
        "child did not finish: {stdout}\n{stderr}"
    );
    assert!(
        output.status.success(),
        "child exited {}: {stderr}",
        output.status
    );
}

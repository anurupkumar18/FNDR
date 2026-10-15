//! Real-model process checks. These read model weights, not captures or stores.
use fndr_lib::inference::InferenceEngine;
use std::{ffi::OsStr, path::PathBuf, process::Command};

static MODEL_CHECK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
#[ignore = "loads the installed GGUF; run explicitly and serially"]
fn real_text_engine_drop_exits_cleanly() {
    let _serial = MODEL_CHECK.lock().expect("serial model check");
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

#[test]
#[ignore = "loads the installed vision GGUF and matching projector; run serially"]
fn real_vision_request_and_shutdown_exit_cleanly() {
    let _serial = MODEL_CHECK.lock().expect("serial model check");
    const CHILD: &str = "FNDR_VISION_DROP_TEST_CHILD";
    const DROPPED: &str = "vision request and resource shutdown completed";
    if std::env::var_os(CHILD).as_deref() == Some(OsStr::new("1")) {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let app_dir = PathBuf::from(
            std::env::var_os("FNDR_VISION_TEST_APP_DIR").expect("isolated vision model directory"),
        );
        let mut pixels = image::RgbaImage::from_pixel(224, 224, image::Rgba([255, 255, 255, 255]));
        for y in 48..176 {
            for x in 48..176 {
                pixels.put_pixel(x, y, image::Rgba([220, 20, 20, 255]));
            }
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels)
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("synthetic PNG");
        let insight = runtime
            .block_on(fndr_lib::inference::extract_image_semantics(
                png.into_inner(),
                "synthetic-shape.png",
                fndr_lib::inference::ImageImportSource::FilePicker,
                app_dir.clone(),
            ))
            .expect("real pixel request");
        assert_eq!(insight.model_id, "qwen3-vl-2b");
        assert!(!insight.summary_short.is_empty() || !insight.summary_detailed.is_empty());
        assert!(fndr_lib::inference::pixel_vlm_loaded());
        fndr_lib::inference::shutdown_pixel_runtime();
        assert!(!fndr_lib::inference::pixel_vlm_loaded());
        assert!(
            runtime
                .block_on(fndr_lib::inference::extract_image_semantics(
                    Vec::new(),
                    "after-stop.png",
                    fndr_lib::inference::ImageImportSource::FilePicker,
                    app_dir,
                ))
                .is_err(),
            "shutdown must reject another lazy load"
        );
        drop(runtime);
        println!("{DROPPED}");
        return;
    }
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "real_vision_request_and_shutdown_exit_cleanly",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("vision lifecycle child");
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

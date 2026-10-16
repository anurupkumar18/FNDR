use super::*;
use std::sync::Mutex as StdMutex;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/operator/fake_codex_app_server.py")
}

/// A broker on the scripted app-server, with the bytes it put in the ledger.
fn broker_with(flags: &[&str]) -> (Broker, Arc<StdMutex<Vec<usize>>>) {
    let sent: Arc<StdMutex<Vec<usize>>> = Arc::default();
    let ledger = sent.clone();
    let broker = Broker::new(
        Launcher::Fixed {
            executable: fixture(),
            args: flags.iter().map(|f| f.to_string()).collect(),
        },
        Arc::new(move |bytes| ledger.lock().unwrap().push(bytes)),
    );
    (broker, sent)
}

fn logged(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn window(used: f64) -> Option<CodexUsageWindow> {
    Some(CodexUsageWindow {
        used_percent: used,
        window_minutes: Some(300),
        resets_at: None,
    })
}

#[test]
fn the_plan_voice_falls_back_above_ninety_five_percent_of_either_window() {
    let ready = classify(Some("chatgpt"), &[window(40.0), window(95.0)], false, true);
    assert_eq!(ready.state, VoiceOutState::Ready);
    assert_eq!(ready.used_percent, Some(95.0));
    assert_eq!(ready.fallback_above_percent, 95.0);

    let near = classify(Some("chatgpt"), &[window(10.0), window(95.5)], false, true);
    assert_eq!(near.state, VoiceOutState::NearLimit);
    assert!(near.detail.unwrap().contains("95 percent"));

    let over = classify(Some("chatgpt"), &[window(100.0), None], false, true);
    assert_eq!(over.state, VoiceOutState::OverLimit);
    let reached = classify(Some("chatgpt"), &[window(3.0), None], true, true);
    assert_eq!(reached.state, VoiceOutState::OverLimit);
}

#[test]
fn an_api_key_or_no_account_is_signed_out_and_an_old_codex_is_unsupported() {
    assert_eq!(
        classify(Some("apiKey"), &[], false, true).state,
        VoiceOutState::SignedOut
    );
    assert_eq!(
        classify(None, &[], false, true).state,
        VoiceOutState::SignedOut
    );
    assert_eq!(
        classify(Some("chatgpt"), &[], false, false).state,
        VoiceOutState::Unsupported
    );
}

#[test]
fn the_session_starts_without_startup_context_or_handoffs() {
    let params = start_params("t-1", "v=0\r\n", Some("marin"));
    assert_eq!(params["version"], "v3");
    assert_eq!(params["outputModality"], "audio");
    assert_eq!(params["includeStartupContext"], false);
    assert_eq!(params["clientManagedHandoffs"], true);
    assert_eq!(params["transport"]["type"], "webrtc");
    assert_eq!(params["prompt"], READ_ALOUD_INSTRUCTIONS);
    assert_eq!(params["voice"], "marin");
    assert!(start_params("t-1", "v=0", Some("x y"))
        .get("voice")
        .is_none());
}

#[test]
fn an_sdp_answer_settles_the_start_even_before_its_response() {
    let live = AtomicBool::new(false);
    let mut waiting = HashMap::new();
    let (reply, mut answer) = oneshot::channel();
    waiting.insert(
        7,
        Waiting {
            method: "thread/realtime/start",
            awaits: Some(SDP_NOTIFICATION),
            reply,
        },
    );
    settle(
        &mut waiting,
        &live,
        &json!({ "method": SDP_NOTIFICATION, "params": { "threadId": "t", "sdp": "answer" } }),
    );
    assert_eq!(answer.try_recv().unwrap().unwrap(), json!("answer"));
    settle(&mut waiting, &live, &json!({ "id": 7, "result": {} }));
    assert!(waiting.is_empty());
}

#[test]
fn a_realtime_error_fails_the_start_with_codex_reason() {
    let live = AtomicBool::new(true);
    let mut waiting = HashMap::new();
    let (reply, mut answer) = oneshot::channel();
    waiting.insert(
        1,
        Waiting {
            method: "thread/realtime/start",
            awaits: Some(SDP_NOTIFICATION),
            reply,
        },
    );
    settle(
        &mut waiting,
        &live,
        &json!({ "method": "thread/realtime/error", "params": { "threadId": "t", "message": "Offer did not have an audio media section." } }),
    );
    let error = answer.try_recv().unwrap().unwrap_err();
    assert!(error.contains("audio media section"), "{error}");
    settle(
        &mut waiting,
        &live,
        &json!({ "method": "thread/realtime/closed", "params": { "threadId": "t" } }),
    );
    assert!(!live.load(Ordering::SeqCst));
}

#[tokio::test]
async fn status_reads_sign_in_usage_and_voices_from_codex() {
    let (broker, _) = broker_with(&["fake:usage=42"]);
    let status = broker.status(false).await;
    assert_eq!(status.state, VoiceOutState::Ready, "{status:?}");
    assert_eq!(status.used_percent, Some(42.0));
    assert!(!status.connected);
    let voices = broker.voices().await;
    assert_eq!(voices.voices, ["juniper", "cove", "alloy", "marin"]);
}

#[tokio::test]
async fn signed_out_over_limit_and_old_codex_refuse_to_connect() {
    for (flag, state) in [
        ("fake:signed_out", VoiceOutState::SignedOut),
        ("fake:usage=97", VoiceOutState::NearLimit),
        ("fake:usage=100", VoiceOutState::OverLimit),
        ("fake:no_realtime", VoiceOutState::Unsupported),
    ] {
        let (broker, sent) = broker_with(&[flag]);
        assert_eq!(broker.status(false).await.state, state, "{flag}");
        assert!(broker.start("v=0\r\noffer", None, false).await.is_err());
        assert!(sent.lock().unwrap().is_empty(), "{flag} sent something");
    }
}

#[tokio::test]
async fn speaks_only_the_reply_text_and_logs_each_send() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("realtime.jsonl");
    let log_flag = format!("fake:log={}", log.display());
    let (broker, sent) = broker_with(&[&log_flag]);

    let answer = broker.start("v=0\r\noffer", Some("marin"), false).await;
    assert_eq!(answer.unwrap(), "v=0\r\nfake-answer\r\n");
    assert!(broker.status(false).await.connected);
    broker
        .speak("Your briefing is ready.", false)
        .await
        .unwrap();

    let calls = logged(&log);
    let methods: Vec<&str> = calls
        .iter()
        .map(|c| c["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        methods,
        [
            "thread/realtime/listVoices",
            "thread/realtime/start",
            "thread/realtime/appendSpeech"
        ]
    );
    assert_eq!(calls[1]["params"]["includeStartupContext"], false);
    assert_eq!(calls[2]["params"]["text"], "Your briefing is ready.");
    assert_eq!(
        calls[2]["params"].as_object().unwrap().len(),
        2,
        "appendSpeech carries the thread and the text, nothing more"
    );
    assert_eq!(
        *sent.lock().unwrap(),
        [
            READ_ALOUD_INSTRUCTIONS.len(),
            "Your briefing is ready.".len()
        ]
    );
    broker.stop().await;
    assert!(!broker.status(false).await.connected);
}

#[tokio::test]
async fn private_mode_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("realtime.jsonl");
    let log_flag = format!("fake:log={}", log.display());
    let (broker, sent) = broker_with(&[&log_flag]);
    broker.start("v=0\r\noffer", None, false).await.unwrap();
    let before = logged(&log).len();

    assert_eq!(
        broker
            .speak("Three tasks are open.", true)
            .await
            .unwrap_err(),
        PRIVATE_MODE_REFUSAL
    );
    assert!(broker.start("v=0\r\noffer", None, true).await.is_err());
    assert_eq!(broker.status(true).await.state, VoiceOutState::PrivateMode);
    assert_eq!(logged(&log).len(), before);
    assert_eq!(sent.lock().unwrap().len(), 1, "only the first start");
}

#[tokio::test]
async fn refuses_empty_or_overlong_text_and_speech_before_connecting() {
    let (broker, sent) = broker_with(&[]);
    assert!(broker.speak("Hello.", false).await.is_err());
    broker.start("v=0\r\noffer", None, false).await.unwrap();
    assert!(broker.speak("   ", false).await.is_err());
    let long = "a".repeat(MAX_SPOKEN_CHARS + 1);
    assert!(broker.speak(&long, false).await.is_err());
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_missing_sdp_answer_times_out_and_kills_the_session() {
    let (mut broker, _) = broker_with(&["fake:no_sdp"]);
    broker.start_timeout = Duration::from_millis(500);
    let started = broker.start("v=0\r\noffer", None, false);
    let error = tokio::time::timeout(Duration::from_secs(20), started)
        .await
        .expect("start gives up on its own")
        .unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    assert!(broker.state.lock().await.session.is_none());
}

#[tokio::test]
async fn a_dropped_session_is_noticed_and_the_next_start_reconnects() {
    let (broker, _) = broker_with(&["fake:drop_after_speak"]);
    broker.start("v=0\r\noffer", None, false).await.unwrap();
    broker.speak("First line.", false).await.unwrap();

    // Codex closed the call and exited; the session task sees it at once.
    for _ in 0..50 {
        if !broker.status(false).await.connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!broker.status(false).await.connected);
    assert!(broker.speak("Second line.", false).await.is_err());

    broker.start("v=0\r\noffer", None, false).await.unwrap();
    broker.speak("Second line.", false).await.unwrap();
}

#[tokio::test]
async fn cancel_ends_the_call_and_keeps_codex_for_the_next_one() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("realtime.jsonl");
    let log_flag = format!("fake:log={}", log.display());
    let (broker, _) = broker_with(&[&log_flag]);
    broker.start("v=0\r\noffer", None, false).await.unwrap();
    broker.cancel().await;
    assert!(!broker.status(false).await.connected);
    assert!(broker.speak("Late.", false).await.is_err());

    broker.start("v=0\r\noffer", None, false).await.unwrap();
    let starts: Vec<String> = logged(&log)
        .iter()
        .map(|c| c["params"]["threadId"].as_str().unwrap_or("").to_string())
        .filter(|id| !id.is_empty())
        .collect();
    assert_eq!(
        logged(&log)
            .iter()
            .filter(|c| c["method"] == "thread/realtime/stop")
            .count(),
        1
    );
    assert!(
        starts.windows(2).all(|pair| pair[0] == pair[1]),
        "same thread: {starts:?}"
    );
}

/// A real Codex and ChatGPT sign-in: status, voices, a realtime start with a
/// hand-written WebRTC offer, and one sentence handed over to be spoken. No
/// media flows (nothing answers ICE here), so this proves the broker's
/// protocol, not the audio; the audio was heard through a browser peer on
/// 2026-10-09 (ADR 028). Set `FNDR_LIVE_CODEX` to a Codex that runs.
#[tokio::test]
#[ignore = "Live ChatGPT voice; run: FNDR_LIVE_CODEX=/path/to/codex cargo test --lib live_voice_out_speaks_one_sentence -- --ignored --nocapture"]
async fn live_voice_out_speaks_one_sentence() {
    let executable = std::env::var_os("FNDR_LIVE_CODEX").map(PathBuf::from);
    let sent: Arc<StdMutex<Vec<usize>>> = Arc::default();
    let ledger = sent.clone();
    let broker = Broker::new(
        Launcher::Codex(executable),
        Arc::new(move |bytes| ledger.lock().unwrap().push(bytes)),
    );
    let status = broker.status(false).await;
    println!("status={status:?}");
    assert_eq!(status.state, VoiceOutState::Ready);
    println!("voices={:?}", broker.voices().await.voices);

    let answer = broker
        .start(LIVE_OFFER, None, false)
        .await
        .expect("Codex answers the offer");
    assert!(answer.contains("m=audio"), "answer: {answer}");
    broker
        .speak("Your briefing is ready.", false)
        .await
        .expect("appendSpeech accepted");
    broker.stop().await;
    println!("ledger bytes={:?}", sent.lock().unwrap());
}

const LIVE_OFFER: &str = "v=0\r\n\
o=- 4611731400430051336 2 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
a=group:BUNDLE 0 1\r\n\
a=extmap-allow-mixed\r\n\
a=msid-semantic: WMS\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
c=IN IP4 0.0.0.0\r\n\
a=rtcp:9 IN IP4 0.0.0.0\r\n\
a=ice-ufrag:fndr\r\n\
a=ice-pwd:fndrfndrfndrfndrfndrfndr\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 7E:74:E1:BF:A6:3A:28:DB:77:69:DF:C4:11:69:9A:42:E3:E6:F2:42:75:59:F1:90:16:B7:64:C2:1E:CD:1D:A1\r\n\
a=setup:actpass\r\n\
a=mid:0\r\n\
a=sendrecv\r\n\
a=msid:- fndr-silent\r\n\
a=rtcp-mux\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=fmtp:111 minptime=10;useinbandfec=1\r\n\
a=ssrc:1001 cname:fndr\r\n\
m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n\
c=IN IP4 0.0.0.0\r\n\
a=ice-ufrag:fndr\r\n\
a=ice-pwd:fndrfndrfndrfndrfndrfndr\r\n\
a=ice-options:trickle\r\n\
a=fingerprint:sha-256 7E:74:E1:BF:A6:3A:28:DB:77:69:DF:C4:11:69:9A:42:E3:E6:F2:42:75:59:F1:90:16:B7:64:C2:1E:CD:1D:A1\r\n\
a=setup:actpass\r\n\
a=mid:1\r\n\
a=sctp-port:5000\r\n\
a=max-message-size:262144\r\n";

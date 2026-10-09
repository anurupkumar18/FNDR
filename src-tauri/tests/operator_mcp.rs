//! `fndr operator-mcp` as Codex starts it: the real binary, over stdio.
//! Settings come from a throwaway HOME, so the owner's config is never read.
//! Reading an app through the real Accessibility API needs the permission and
//! is checked by hand, not here.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn ask(requests: &[Value]) -> (Vec<Value>, bool) {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fndr"))
        .arg("operator-mcp")
        .env("HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let replies = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    let exited_cleanly = child.wait().unwrap().success();
    (replies, exited_cleanly)
}

#[test]
fn serves_the_policy_tool_names_and_exits_when_codex_closes_the_pipe() {
    let (replies, exited_cleanly) = ask(&[
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_apps","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_app_state","arguments":{"app":"No Such App 91"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"click","arguments":{"app":"FNDR","element_index":"1"}}}),
    ]);
    assert!(exited_cleanly, "the server must stop when its input closes");
    assert_eq!(replies.len(), 5, "notifications get no reply: {replies:?}");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "fndr-operator");
    let tools: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for name in [
        "get_app_state",
        "click",
        "set_value",
        "type_text",
        "press_key",
        "scroll",
        "list_windows",
        "arrange_windows",
        "move_window",
        "resize_window",
    ] {
        assert!(tools.contains(&name), "{name} missing from {tools:?}");
    }
    assert!(
        replies[2]["result"]["isError"].is_null(),
        "{:?}",
        replies[2]
    );
    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(replies[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("not running"));
    assert_eq!(replies[4]["result"]["isError"], true);
}

#[test]
fn window_tools_refuse_ids_from_no_list_and_off_limits_apps_over_the_pipe() {
    let (replies, exited_cleanly) = ask(&[
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"arrange_windows","arguments":{"layout":"left_right_split","windows":[{"id":1,"app":"TextEdit"},{"id":2,"app":"Preview"}]}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"arrange_windows","arguments":{"layout":"cascade","windows":[{"id":1,"app":"TextEdit"}]}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"move_window","arguments":{"app":"FNDR","window":1,"x":0,"y":0}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_windows","arguments":{"app":"No Such App 91"}}}),
    ]);
    assert!(exited_cleanly);
    assert_eq!(replies.len(), 5, "{replies:?}");
    let text = |at: usize| {
        assert_eq!(replies[at]["result"]["isError"], true, "{:?}", replies[at]);
        replies[at]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(text(1).contains("Call list_windows"), "{}", text(1));
    assert!(text(2).contains("left_right_split"), "{}", text(2));
    let fndr = text(3);
    assert!(
        fndr.contains("off limits") || fndr.contains("not running"),
        "{fndr}"
    );
    assert!(text(4).contains("not running"), "{}", text(4));
}

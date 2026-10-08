# GS-11 risk policy test output

Command: `cd src-tauri && cargo test --lib agent::` (37 passed), plus `config::` (18 passed) and `mcp::` (7 passed).

```
test agent::risk_policy::tests::command_bar_and_voice_follow_the_tool_risk ... ok
test agent::risk_policy::tests::mcp_always_confirms ... ok
test agent::risk_policy::tests::mcp_call_tool_checks_the_gate_before_dispatching_any_tool ... ok
test agent::risk_policy::tests::the_four_audited_mcp_side_effects_are_gated_and_never_run_unattended ... ok
test agent::risk_policy::tests::unregistered_tool_is_an_error_not_a_decision ... ok
test agent::risk_policy::tests::every_registry_tool_is_decided_for_every_caller ... ok
test agent::risk_policy::tests::kill_switch_refuses_every_tool_and_caller ... ok
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 814 filtered out; finished in 0.07s
```

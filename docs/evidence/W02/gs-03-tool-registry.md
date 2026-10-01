# GS-03 tool registry test output

Command: `cd src-tauri && cargo test --lib agent::tools`

```
running 11 tests
test agent::tools::tests::registering_a_name_twice_is_refused ... ok
test agent::tools::tests::risk_levels_match_the_actions_policy ... ok
test agent::tools::tests::tools_the_policy_forbids_are_not_registered ... ok
test agent::tools::tests::registration_rejects_schemas_the_validator_cannot_enforce ... ok
test agent::tools::tests::unknown_tool_is_refused_before_arguments_are_read ... ok
test agent::tools::tests::string_length_counts_characters_not_bytes ... ok
test agent::tools::tests::valid_arguments_pass ... ok
test agent::tools::tests::invalid_arguments_are_refused ... ok
test agent::tools::tests::sensitive_arguments_come_from_the_schema ... ok
test agent::tools::tests::registers_the_twelve_october_tools ... ok
test agent::tools::tests::execute_validates_before_running_the_executor ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 833 filtered out; finished in 0.01s
```

//! Typed command-surface tool registry (GS-03).
//!
//! The registry is the only place a command-bar tool exists. The router can
//! name a tool and supply arguments, but a call is refused unless the tool is
//! registered and its arguments satisfy the schema the model was shown. Risk
//! levels are fixed here and are not model-selectable. See
//! `docs/product/command-surface.md` and `docs/product/actions-policy.md`.

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    Runs,
    OneTap,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments for {tool}: {reason}")]
    InvalidArguments { tool: String, reason: String },
    #[error("tool {0} has no executor yet")]
    NotImplemented(String),
    #[error("invalid tool definition {tool}: {reason}")]
    InvalidDefinition { tool: String, reason: String },
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolOutput {
    pub summary: String,
    pub data: Value,
}

/// What the stage 2 router is shown for each tool.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub arguments_schema: Value,
    pub risk: ToolRisk,
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// A JSON schema string using the subset `validate_arguments` supports.
    fn arguments_schema(&self) -> &'static str;
    fn risk(&self) -> ToolRisk;
    /// Called only with arguments that already passed schema validation.
    fn execute(&self, _arguments: Value) -> BoxFuture<'_, Result<ToolOutput, ToolError>> {
        let name = self.name().to_string();
        Box::pin(async move { Err(ToolError::NotImplemented(name)) })
    }
}

struct Registered {
    tool: Box<dyn Tool>,
    schema: Value,
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<&'static str, Registered>,
}

const SCHEMA_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "description",
];
const PROPERTY_KEYWORDS: &[&str] = &[
    "type",
    "description",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "enum",
    "x-sensitive",
];

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> Result<(), ToolError> {
        let name = tool.name();
        if self.tools.contains_key(name) {
            return Err(definition_error(name, "registered twice"));
        }
        let schema: Value = serde_json::from_str(tool.arguments_schema())
            .map_err(|e| definition_error(name, &format!("schema is not JSON: {e}")))?;
        check_schema_definition(name, &schema)?;
        self.tools.insert(name, Registered { tool, schema });
        Ok(())
    }

    pub fn risk_of(&self, name: &str) -> Result<ToolRisk, ToolError> {
        self.get(name).map(|r| r.tool.risk())
    }

    /// Validates `arguments` against the tool's schema. An unknown tool is
    /// refused before the arguments are read.
    pub fn validate(&self, name: &str, arguments: &Value) -> Result<(), ToolError> {
        let registered = self.get(name)?;
        validate_arguments(name, &registered.schema, arguments)
    }

    /// Names of arguments the schema marks `x-sensitive`, for journal hashing.
    pub fn sensitive_arguments(&self, name: &str) -> Result<Vec<String>, ToolError> {
        let registered = self.get(name)?;
        let properties = registered.schema["properties"].as_object();
        Ok(properties
            .into_iter()
            .flatten()
            .filter(|(_, p)| p["x-sensitive"].as_bool() == Some(true))
            .map(|(k, _)| k.clone())
            .collect())
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools
            .values()
            .map(|r| ToolSpec {
                name: r.tool.name(),
                description: r.tool.description(),
                arguments_schema: r.schema.clone(),
                risk: r.tool.risk(),
            })
            .collect()
    }

    /// Validates, then runs the executor. Confirmation for `OneTap` tools is
    /// the caller's job (GS-11); this method never skips validation.
    pub async fn execute(&self, name: &str, arguments: Value) -> Result<ToolOutput, ToolError> {
        let registered = self.get(name)?;
        validate_arguments(name, &registered.schema, &arguments)?;
        registered.tool.execute(arguments).await
    }

    fn get(&self, name: &str) -> Result<&Registered, ToolError> {
        self.tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))
    }
}

fn definition_error(tool: &str, reason: &str) -> ToolError {
    ToolError::InvalidDefinition {
        tool: tool.to_string(),
        reason: reason.to_string(),
    }
}

fn invalid(tool: &str, reason: impl Into<String>) -> ToolError {
    ToolError::InvalidArguments {
        tool: tool.to_string(),
        reason: reason.into(),
    }
}

/// Rejects schemas the validator cannot enforce, so a keyword the validator
/// ignores can never silently weaken a tool's argument check.
fn check_schema_definition(tool: &str, schema: &Value) -> Result<(), ToolError> {
    let object = schema
        .as_object()
        .ok_or_else(|| definition_error(tool, "schema must be an object"))?;
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err(definition_error(tool, "top-level type must be object"));
    }
    if object.get("additionalProperties") != Some(&Value::Bool(false)) {
        return Err(definition_error(
            tool,
            "additionalProperties must be false so unknown arguments are refused",
        ));
    }
    if let Some(keyword) = object
        .keys()
        .find(|k| !SCHEMA_KEYWORDS.contains(&k.as_str()))
    {
        return Err(definition_error(
            tool,
            &format!("unsupported keyword {keyword}"),
        ));
    }
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| definition_error(tool, "properties must be an object"))?;
    for (key, property) in properties {
        let property = property
            .as_object()
            .ok_or_else(|| definition_error(tool, &format!("{key} must be an object")))?;
        if let Some(keyword) = property
            .keys()
            .find(|k| !PROPERTY_KEYWORDS.contains(&k.as_str()))
        {
            return Err(definition_error(
                tool,
                &format!("{key}: unsupported keyword {keyword}"),
            ));
        }
        match property.get("type").and_then(Value::as_str) {
            Some("string" | "integer" | "number" | "boolean") => {}
            _ => return Err(definition_error(tool, &format!("{key}: unsupported type"))),
        }
    }
    for required in object
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = required.as_str().unwrap_or_default();
        if !properties.contains_key(name) {
            return Err(definition_error(
                tool,
                &format!("required {name} is not a property"),
            ));
        }
    }
    Ok(())
}

fn validate_arguments(tool: &str, schema: &Value, arguments: &Value) -> Result<(), ToolError> {
    let supplied = arguments
        .as_object()
        .ok_or_else(|| invalid(tool, "arguments must be a JSON object"))?;
    let properties = schema["properties"]
        .as_object()
        .expect("checked at registration");

    for required in schema["required"].as_array().into_iter().flatten() {
        let name = required.as_str().unwrap_or_default();
        if !supplied.contains_key(name) {
            return Err(invalid(tool, format!("missing required argument {name}")));
        }
    }
    for (key, value) in supplied {
        let property = properties
            .get(key)
            .ok_or_else(|| invalid(tool, format!("unknown argument {key}")))?;
        validate_value(tool, key, property, value)?;
    }
    Ok(())
}

fn validate_value(tool: &str, key: &str, property: &Value, value: &Value) -> Result<(), ToolError> {
    match property["type"].as_str().unwrap_or_default() {
        "string" => {
            let text = value
                .as_str()
                .ok_or_else(|| invalid(tool, format!("{key} must be a string")))?;
            let length = text.chars().count() as u64;
            if let Some(min) = property["minLength"].as_u64() {
                if length < min {
                    return Err(invalid(tool, format!("{key} is shorter than {min}")));
                }
            }
            if let Some(max) = property["maxLength"].as_u64() {
                if length > max {
                    return Err(invalid(tool, format!("{key} is longer than {max}")));
                }
            }
            if let Some(allowed) = property["enum"].as_array() {
                if !allowed.iter().any(|a| a == value) {
                    return Err(invalid(tool, format!("{key} is not an allowed value")));
                }
            }
        }
        "integer" => {
            let number = value
                .as_i64()
                .ok_or_else(|| invalid(tool, format!("{key} must be an integer")))?;
            check_range(tool, key, property, number as f64)?;
        }
        "number" => {
            let number = value
                .as_f64()
                .ok_or_else(|| invalid(tool, format!("{key} must be a number")))?;
            check_range(tool, key, property, number)?;
        }
        "boolean" => {
            if !value.is_boolean() {
                return Err(invalid(tool, format!("{key} must be a boolean")));
            }
        }
        _ => unreachable!("checked at registration"),
    }
    Ok(())
}

fn check_range(tool: &str, key: &str, property: &Value, number: f64) -> Result<(), ToolError> {
    if let Some(min) = property["minimum"].as_f64() {
        if number < min {
            return Err(invalid(tool, format!("{key} is below {min}")));
        }
    }
    if let Some(max) = property["maximum"].as_f64() {
        if number > max {
            return Err(invalid(tool, format!("{key} is above {max}")));
        }
    }
    Ok(())
}

/// Metadata-only tool. Executors arrive with GS-04 and GS-05 and replace the
/// entry of the same name; until then `execute` reports `NotImplemented`.
struct StaticTool {
    name: &'static str,
    description: &'static str,
    schema: &'static str,
    risk: ToolRisk,
}

impl Tool for StaticTool {
    fn name(&self) -> &'static str {
        self.name
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn arguments_schema(&self) -> &'static str {
        self.schema
    }
    fn risk(&self) -> ToolRisk {
        self.risk
    }
}

const NO_ARGUMENTS: &str = r#"{"type":"object","properties":{},"additionalProperties":false}"#;

/// The October tool list from `docs/product/actions-policy.md`.
pub fn october_registry() -> ToolRegistry {
    use ToolRisk::{OneTap, Runs};
    let tools: [(&str, &str, &str, ToolRisk); 12] = [
        (
            "open_app",
            "Open an installed Mac app by its name.",
            r#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":200}},"required":["name"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "open_url",
            "Open an http or https page in the default browser.",
            r#"{"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":2048}},"required":["url"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "open_memory_source",
            "Reopen the page, file, or app a captured memory came from.",
            r#"{"type":"object","properties":{"memory_id":{"type":"string","minLength":1,"maxLength":128}},"required":["memory_id"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "reveal_file",
            "Show a file in Finder without opening it.",
            r#"{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096}},"required":["path"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "search",
            "Search the person's captured memories.",
            r#"{"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":500}},"required":["query"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "about_this_screen",
            "Answer a question about what is on the screen right now.",
            r#"{"type":"object","properties":{"question":{"type":"string","minLength":1,"maxLength":1000}},"required":["question"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "start_timer",
            "Start a timer that posts a local notification.",
            r#"{"type":"object","properties":{"minutes":{"type":"integer","minimum":1,"maximum":1440},"label":{"type":"string","maxLength":100}},"required":["minutes"],"additionalProperties":false}"#,
            Runs,
        ),
        (
            "pause_capture",
            "Pause FNDR's screen capture.",
            NO_ARGUMENTS,
            Runs,
        ),
        (
            "resume_capture",
            "Resume FNDR's screen capture.",
            NO_ARGUMENTS,
            OneTap,
        ),
        (
            "paste_text",
            "Paste text into the app that was focused when the command started.",
            r#"{"type":"object","properties":{"text":{"type":"string","minLength":1,"maxLength":20000,"x-sensitive":true}},"required":["text"],"additionalProperties":false}"#,
            OneTap,
        ),
        (
            "create_reminder",
            "Create a reminder in the Reminders app.",
            r#"{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":200},"due":{"type":"string","maxLength":64}},"required":["title"],"additionalProperties":false}"#,
            OneTap,
        ),
        (
            "run_shortcut",
            "Run one of the person's Apple Shortcuts by name.",
            r#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":200}},"required":["name"],"additionalProperties":false}"#,
            OneTap,
        ),
    ];
    let mut registry = ToolRegistry::new();
    for (name, description, schema, risk) in tools {
        registry
            .register(Box::new(StaticTool {
                name,
                description,
                schema,
                risk,
            }))
            .expect("built-in tool definitions are valid");
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registers_the_twelve_october_tools() {
        let names: Vec<_> = october_registry().specs().iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 12);
        for expected in [
            "open_app",
            "open_url",
            "open_memory_source",
            "reveal_file",
            "search",
            "about_this_screen",
            "start_timer",
            "pause_capture",
            "resume_capture",
            "paste_text",
            "create_reminder",
            "run_shortcut",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn tools_the_policy_forbids_are_not_registered() {
        let registry = october_registry();
        for forbidden in [
            "send_message",
            "send_email",
            "delete_file",
            "delete_memory",
            "purchase",
            "run_shell",
            "run_read_only_command",
        ] {
            assert_eq!(
                registry.risk_of(forbidden),
                Err(ToolError::UnknownTool(forbidden.to_string()))
            );
        }
    }

    #[test]
    fn risk_levels_match_the_actions_policy() {
        let registry = october_registry();
        for name in [
            "open_app",
            "open_url",
            "open_memory_source",
            "reveal_file",
            "search",
            "about_this_screen",
            "start_timer",
            "pause_capture",
        ] {
            assert_eq!(registry.risk_of(name), Ok(ToolRisk::Runs), "{name}");
        }
        for name in [
            "resume_capture",
            "paste_text",
            "create_reminder",
            "run_shortcut",
        ] {
            assert_eq!(registry.risk_of(name), Ok(ToolRisk::OneTap), "{name}");
        }
    }

    #[test]
    fn unknown_tool_is_refused_before_arguments_are_read() {
        let registry = october_registry();
        let result = registry.validate("send_message", &json!("not even an object"));
        assert_eq!(result, Err(ToolError::UnknownTool("send_message".into())));
    }

    #[test]
    fn valid_arguments_pass() {
        let registry = october_registry();
        assert_eq!(
            registry.validate("open_app", &json!({"name": "Slack"})),
            Ok(())
        );
        assert_eq!(registry.validate("pause_capture", &json!({})), Ok(()));
        assert_eq!(
            registry.validate("start_timer", &json!({"minutes": 25, "label": "focus"})),
            Ok(())
        );
        assert_eq!(
            registry.validate("create_reminder", &json!({"title": "email Dana"})),
            Ok(())
        );
    }

    #[test]
    fn invalid_arguments_are_refused() {
        let registry = october_registry();
        let cases = [
            ("open_app", json!({})),
            ("open_app", json!({"name": ""})),
            ("open_app", json!({"name": 7})),
            ("open_app", json!({"name": "Slack", "extra": "x"})),
            ("open_app", json!(["Slack"])),
            ("search", json!({"query": "x".repeat(501)})),
            ("start_timer", json!({"minutes": 0})),
            ("start_timer", json!({"minutes": 1441})),
            ("start_timer", json!({"minutes": 2.5})),
            ("start_timer", json!({"minutes": "10"})),
            ("pause_capture", json!({"force": true})),
        ];
        for (tool, arguments) in cases {
            assert!(
                matches!(
                    registry.validate(tool, &arguments),
                    Err(ToolError::InvalidArguments { .. })
                ),
                "{tool} {arguments} should be refused"
            );
        }
    }

    #[test]
    fn string_length_counts_characters_not_bytes() {
        let registry = october_registry();
        let name = "é".repeat(200);
        assert_eq!(
            registry.validate("open_app", &json!({ "name": name })),
            Ok(())
        );
    }

    #[test]
    fn sensitive_arguments_come_from_the_schema() {
        let registry = october_registry();
        assert_eq!(
            registry.sensitive_arguments("paste_text"),
            Ok(vec!["text".to_string()])
        );
        assert_eq!(registry.sensitive_arguments("open_app"), Ok(vec![]));
    }

    struct Probe(&'static str);
    impl Tool for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }
        fn description(&self) -> &'static str {
            "test"
        }
        fn arguments_schema(&self) -> &'static str {
            self.0
        }
        fn risk(&self) -> ToolRisk {
            ToolRisk::Runs
        }
    }

    #[test]
    fn registration_rejects_schemas_the_validator_cannot_enforce() {
        let rejected = [
            r#"{"type":"object","properties":{}}"#,
            r#"{"type":"object","properties":{},"additionalProperties":true}"#,
            r#"{"type":"object","properties":{"a":{"type":"string","pattern":"^x$"}},"additionalProperties":false}"#,
            r#"{"type":"object","properties":{"a":{"type":"array"}},"additionalProperties":false}"#,
            r#"{"type":"object","properties":{},"required":["a"],"additionalProperties":false}"#,
            r#"not json"#,
        ];
        for schema in rejected {
            let mut registry = ToolRegistry::new();
            assert!(
                matches!(
                    registry.register(Box::new(Probe(schema))),
                    Err(ToolError::InvalidDefinition { .. })
                ),
                "{schema} should be rejected"
            );
        }
    }

    #[test]
    fn registering_a_name_twice_is_refused() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(Probe(NO_ARGUMENTS))).unwrap();
        assert!(matches!(
            registry.register(Box::new(Probe(NO_ARGUMENTS))),
            Err(ToolError::InvalidDefinition { .. })
        ));
    }

    #[tokio::test]
    async fn execute_validates_before_running_the_executor() {
        let registry = october_registry();
        let invalid = registry.execute("open_app", json!({})).await;
        assert!(matches!(invalid, Err(ToolError::InvalidArguments { .. })));

        let valid = registry.execute("open_app", json!({"name": "Slack"})).await;
        assert_eq!(valid, Err(ToolError::NotImplemented("open_app".into())));

        let unknown = registry.execute("delete_file", json!({})).await;
        assert_eq!(unknown, Err(ToolError::UnknownTool("delete_file".into())));
    }
}

//! The worker's final message: what it made, what it claims and where each
//! claim comes from. Codex constrains the message with `output_schema`; FNDR
//! still parses it strictly, because the worker is untrusted (ADR 032 item 6).

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputKind {
    Docx,
    Xlsx,
    Pptx,
    Pdf,
    Md,
    Txt,
    Csv,
    Png,
}

impl OutputKind {
    pub const ALL: [OutputKind; 8] = [
        Self::Docx,
        Self::Xlsx,
        Self::Pptx,
        Self::Pdf,
        Self::Md,
        Self::Txt,
        Self::Csv,
        Self::Png,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Pdf => "pdf",
            Self::Md => "md",
            Self::Txt => "txt",
            Self::Csv => "csv",
            Self::Png => "png",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub summary: String,
    pub deliverables: Vec<Deliverable>,
    pub claims: Vec<Claim>,
    pub not_done: Vec<String>,
    pub assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deliverable {
    pub path: String,
    pub kind: OutputKind,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub text: String,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Memory,
    File,
    Web,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(rename = "type")]
    pub kind: SourceKind,
    /// A memory id, a path in the job folder, or a URL.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Where in the source: rows, a page, a section. May be empty.
    pub locator: String,
}

/// Parse the worker's final message. Anything else than one manifest object
/// is an error with a short reason for the result card.
pub fn parse(final_message: &str) -> Result<Manifest, String> {
    serde_json::from_str(final_message.trim())
        .map_err(|e| format!("The worker's final message is not a manifest: {e}"))
}

/// The JSON Schema sent as `outputSchema` on `turn/start`.
pub fn output_schema() -> Value {
    let strings = json!({ "type": "array", "items": { "type": "string" } });
    let kinds: Vec<&str> = OutputKind::ALL.iter().map(|k| k.extension()).collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "deliverables", "claims", "not_done", "assumptions"],
        "properties": {
            "summary": { "type": "string" },
            "deliverables": { "type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["path", "kind", "title"],
                "properties": {
                    "path": { "type": "string" },
                    "kind": { "type": "string", "enum": kinds },
                    "title": { "type": "string" }
                }
            }},
            "claims": { "type": "array", "items": {
                "type": "object", "additionalProperties": false,
                "required": ["text", "sources"],
                "properties": {
                    "text": { "type": "string" },
                    "sources": { "type": "array", "items": {
                        "type": "object", "additionalProperties": false,
                        "required": ["type", "ref", "locator"],
                        "properties": {
                            "type": { "type": "string", "enum": ["memory", "file", "web"] },
                            "ref": { "type": "string" },
                            "locator": { "type": "string" }
                        }
                    }}
                }
            }},
            "not_done": strings,
            "assumptions": strings
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{
        "summary": "Setup time drives churn.",
        "deliverables": [{"path": "out/readout.docx", "kind": "docx", "title": "Q4 readout"}],
        "claims": [{"text": "78 of 400 churned.", "sources": [
            {"type": "file", "ref": "inputs/survey.csv", "locator": "churned column"},
            {"type": "memory", "ref": "m_mail_7f3a", "locator": ""}]}],
        "not_done": [],
        "assumptions": ["Thursday is 2026-10-15."]
    }"#;

    #[test]
    fn a_manifest_parses_into_typed_parts() {
        let m = parse(GOOD).unwrap();
        assert_eq!(m.deliverables[0].kind, OutputKind::Docx);
        assert_eq!(m.claims[0].sources[0].kind, SourceKind::File);
        assert_eq!(m.claims[0].sources[1].reference, "m_mail_7f3a");
    }

    #[test]
    fn unknown_fields_kinds_and_prose_are_refused() {
        assert!(parse(&GOOD.replace("\"assumptions\"", "\"extra\": 1, \"assumptions\"")).is_err());
        assert!(parse(&GOOD.replace("\"docx\"", "\"exe\"")).is_err());
        assert!(parse(&GOOD.replace("\"memory\"", "\"rumor\"")).is_err());
        assert!(parse("Here is the readout you asked for.").is_err());
        assert!(parse(&format!("{GOOD} and more")).is_err());
    }

    #[test]
    fn the_schema_requires_every_field_and_names_every_kind() {
        let schema = output_schema();
        assert_eq!(schema["required"].as_array().unwrap().len(), 5);
        let kinds = &schema["properties"]["deliverables"]["items"]["properties"]["kind"]["enum"];
        assert_eq!(kinds.as_array().unwrap().len(), OutputKind::ALL.len());
        assert_eq!(
            schema["properties"]["claims"]["items"]["properties"]["sources"]["items"]["required"],
            json!(["type", "ref", "locator"])
        );
    }
}

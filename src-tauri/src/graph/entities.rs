//! Typed graph node entities.

use serde::{Deserialize, Serialize};
use specta::Type;

/// High-level entity kinds extracted from finalized memory / insight fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub enum GraphNodeType {
    Project,
    Memory,
    Concept,
    Decision,
    File,
    Error,
    Tool,
    Person,
    Url,
    Session,
    Task,
    Window,
    App,
    Command,
}

impl GraphNodeType {
    pub const ALL: [Self; 14] = [
        Self::Project,
        Self::Memory,
        Self::Concept,
        Self::Decision,
        Self::File,
        Self::Error,
        Self::Tool,
        Self::Person,
        Self::Url,
        Self::Session,
        Self::Task,
        Self::Window,
        Self::App,
        Self::Command,
    ];

    pub fn to_str(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Memory => "Memory",
            Self::Concept => "Concept",
            Self::Decision => "Decision",
            Self::File => "File",
            Self::Error => "Error",
            Self::Tool => "Tool",
            Self::Person => "Person",
            Self::Url => "Url",
            Self::Session => "Session",
            Self::Task => "Task",
            Self::Window => "Window",
            Self::App => "App",
            Self::Command => "Command",
        }
    }

    pub fn from_str(name: &str) -> Option<Self> {
        match name {
            "Project" => Some(Self::Project),
            "Memory" => Some(Self::Memory),
            "Concept" => Some(Self::Concept),
            "Decision" => Some(Self::Decision),
            "File" => Some(Self::File),
            "Error" => Some(Self::Error),
            "Tool" => Some(Self::Tool),
            "Person" => Some(Self::Person),
            "Url" => Some(Self::Url),
            "Session" => Some(Self::Session),
            "Task" => Some(Self::Task),
            "Window" => Some(Self::Window),
            "App" => Some(Self::App),
            "Command" => Some(Self::Command),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_type_literals_roundtrip() {
        for node_type in GraphNodeType::ALL {
            assert_eq!(GraphNodeType::from_str(node_type.to_str()), Some(node_type));
        }
    }
}

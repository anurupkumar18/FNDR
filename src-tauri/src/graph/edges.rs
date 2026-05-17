//! Typed graph edge relationships.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Semantic edge kinds for the insight graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub enum GraphEdgeType {
    DependsOn,
    Contains,
    Imports,
    Extends,
    Implements,
    PartOf,
    Supports,
    Contradicts,
    Supersedes,
    Refines,
    Questions,
    Resolves,
    Causes,
    Prevents,
    TriggeredBy,
    FixedBy,
    BrokeBy,
    PrecededBy,
    FollowedBy,
    SimilarTo,
    MentionedIn,
    UsedIn,
    CreatedBy,
    AppliesTo,
    OccurredInSession,
    BelongsToProject,
    UsedApp,
    SameTaskAs,
    EvidencedBy,
}

impl GraphEdgeType {
    pub const ALL: [Self; 29] = [
        Self::DependsOn,
        Self::Contains,
        Self::Imports,
        Self::Extends,
        Self::Implements,
        Self::PartOf,
        Self::Supports,
        Self::Contradicts,
        Self::Supersedes,
        Self::Refines,
        Self::Questions,
        Self::Resolves,
        Self::Causes,
        Self::Prevents,
        Self::TriggeredBy,
        Self::FixedBy,
        Self::BrokeBy,
        Self::PrecededBy,
        Self::FollowedBy,
        Self::SimilarTo,
        Self::MentionedIn,
        Self::UsedIn,
        Self::CreatedBy,
        Self::AppliesTo,
        Self::OccurredInSession,
        Self::BelongsToProject,
        Self::UsedApp,
        Self::SameTaskAs,
        Self::EvidencedBy,
    ];

    pub fn to_str(self) -> &'static str {
        match self {
            Self::DependsOn => "DependsOn",
            Self::Contains => "Contains",
            Self::Imports => "Imports",
            Self::Extends => "Extends",
            Self::Implements => "Implements",
            Self::PartOf => "PartOf",
            Self::Supports => "Supports",
            Self::Contradicts => "Contradicts",
            Self::Supersedes => "Supersedes",
            Self::Refines => "Refines",
            Self::Questions => "Questions",
            Self::Resolves => "Resolves",
            Self::Causes => "Causes",
            Self::Prevents => "Prevents",
            Self::TriggeredBy => "TriggeredBy",
            Self::FixedBy => "FixedBy",
            Self::BrokeBy => "BrokeBy",
            Self::PrecededBy => "PrecededBy",
            Self::FollowedBy => "FollowedBy",
            Self::SimilarTo => "SimilarTo",
            Self::MentionedIn => "MentionedIn",
            Self::UsedIn => "UsedIn",
            Self::CreatedBy => "CreatedBy",
            Self::AppliesTo => "AppliesTo",
            Self::OccurredInSession => "OccurredInSession",
            Self::BelongsToProject => "BelongsToProject",
            Self::UsedApp => "UsedApp",
            Self::SameTaskAs => "SameTaskAs",
            Self::EvidencedBy => "EvidencedBy",
        }
    }

    pub fn from_str(name: &str) -> Option<Self> {
        match name {
            "DependsOn" => Some(Self::DependsOn),
            "Contains" => Some(Self::Contains),
            "Imports" => Some(Self::Imports),
            "Extends" => Some(Self::Extends),
            "Implements" => Some(Self::Implements),
            "PartOf" => Some(Self::PartOf),
            "Supports" => Some(Self::Supports),
            "Contradicts" => Some(Self::Contradicts),
            "Supersedes" => Some(Self::Supersedes),
            "Refines" => Some(Self::Refines),
            "Questions" => Some(Self::Questions),
            "Resolves" => Some(Self::Resolves),
            "Causes" => Some(Self::Causes),
            "Prevents" => Some(Self::Prevents),
            "TriggeredBy" => Some(Self::TriggeredBy),
            "FixedBy" => Some(Self::FixedBy),
            "BrokeBy" => Some(Self::BrokeBy),
            "PrecededBy" => Some(Self::PrecededBy),
            "FollowedBy" => Some(Self::FollowedBy),
            "SimilarTo" => Some(Self::SimilarTo),
            "MentionedIn" => Some(Self::MentionedIn),
            "UsedIn" => Some(Self::UsedIn),
            "CreatedBy" => Some(Self::CreatedBy),
            "AppliesTo" => Some(Self::AppliesTo),
            "OccurredInSession" => Some(Self::OccurredInSession),
            "BelongsToProject" => Some(Self::BelongsToProject),
            "UsedApp" => Some(Self::UsedApp),
            "SameTaskAs" => Some(Self::SameTaskAs),
            "EvidencedBy" => Some(Self::EvidencedBy),
            _ => None,
        }
    }
}

pub mod edge_aliases {
    use super::GraphEdgeType;

    pub fn canonical(name: &str) -> Option<GraphEdgeType> {
        match name {
            "HasTopic" => Some(GraphEdgeType::MentionedIn),
            "CausedError" => Some(GraphEdgeType::Causes),
            "ResolvedError" => Some(GraphEdgeType::Resolves),
            "MadeDecision" => Some(GraphEdgeType::CreatedBy),
            "CreatedTodo" => Some(GraphEdgeType::CreatedBy),
            "RelatedTo" => Some(GraphEdgeType::SimilarTo),
            "Before" => Some(GraphEdgeType::PrecededBy),
            "After" => Some(GraphEdgeType::FollowedBy),
            "VisitedUrl" => Some(GraphEdgeType::MentionedIn),
            "UsesFile" => Some(GraphEdgeType::UsedIn),
            "MentionsEntity" => Some(GraphEdgeType::MentionedIn),
            other => GraphEdgeType::from_str(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{edge_aliases, GraphEdgeType};

    #[test]
    fn edge_type_literals_roundtrip() {
        for edge_type in GraphEdgeType::ALL {
            assert_eq!(GraphEdgeType::from_str(edge_type.to_str()), Some(edge_type));
        }
    }

    #[test]
    fn aliases_map_spec_names_to_canonical_variants() {
        let cases = [
            ("HasTopic", GraphEdgeType::MentionedIn),
            ("CausedError", GraphEdgeType::Causes),
            ("ResolvedError", GraphEdgeType::Resolves),
            ("MadeDecision", GraphEdgeType::CreatedBy),
            ("CreatedTodo", GraphEdgeType::CreatedBy),
            ("RelatedTo", GraphEdgeType::SimilarTo),
            ("Before", GraphEdgeType::PrecededBy),
            ("After", GraphEdgeType::FollowedBy),
            ("VisitedUrl", GraphEdgeType::MentionedIn),
            ("UsesFile", GraphEdgeType::UsedIn),
            ("MentionsEntity", GraphEdgeType::MentionedIn),
        ];

        for (alias, canonical) in cases {
            assert_eq!(edge_aliases::canonical(alias), Some(canonical), "{alias}");
        }
    }

    #[test]
    fn aliases_accept_identity_names() {
        for edge_type in GraphEdgeType::ALL {
            assert_eq!(
                edge_aliases::canonical(edge_type.to_str()),
                Some(edge_type),
                "{}",
                edge_type.to_str()
            );
        }
    }
}

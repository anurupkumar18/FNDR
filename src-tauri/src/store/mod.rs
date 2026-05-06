//! Storage module

mod lance_store;
pub mod schema;
mod state_store;

pub use lance_store::{
    Store, ACTIVITY_EVENTS_TABLE, CONTEXT_DELTAS_TABLE, CONTEXT_PACKS_TABLE, DECISION_LEDGER_TABLE,
    EDGES_TABLE, ENTITY_ALIASES_TABLE, MEETINGS_TABLE, MEMORIES_TABLE, NODES_TABLE,
    PROJECT_CONTEXTS_TABLE, SEGMENTS_TABLE, TASKS_TABLE,
};
pub use schema::{
    ActivityEvent, AppCount, CodeContext, CommandEvent, CommitRef, ContextDelta, ContextPack,
    ContextPackItemReason, ContextRuntimeStatus, ContextTask, DecisionLedgerEntry, DecisionSummary,
    EdgeType, EntityAliasRecord, EntityRef, ErrorEvent, EvidenceRef, ExcludedContextItem,
    FailureSummary, GraphEdge, GraphNode, HealthStatus, IssueSummary, MeetingBreakdown,
    MeetingSegment, MeetingSession, MemoryRecord, NodeType, PrivacyClass, ProjectContext,
    RelevantFile, SearchResult, Stats, Task, TaskType, WorkingState,
};
pub use state_store::StateStore;

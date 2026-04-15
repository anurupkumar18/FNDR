import { ActivityFacet, ClusterLens, ViewMode } from "./graphTypes";

export const VIEW_MODES: Array<{ key: ViewMode; label: string; description: string }> = [
    { key: "constellation", label: "Constellation", description: "How is everything connected?" },
    { key: "timeline", label: "Timeline", description: "How did the session flow?" },
    { key: "cluster", label: "Cluster", description: "What islands of work emerged?" },
    { key: "focus", label: "Focus", description: "Why does this memory matter?" },
    { key: "journey", label: "Journey", description: "How did thought move?" },
];

export const CLUSTER_LENSES: Array<{ key: ClusterLens; label: string }> = [
    { key: "app", label: "App" },
    { key: "memoryType", label: "Memory Type" },
    { key: "domain", label: "Domain" },
    { key: "session", label: "Session" },
];

export const ACTIVITY_FACETS: Array<{ key: ActivityFacet; label: string }> = [
    { key: "all", label: "All Activity" },
    { key: "productive", label: "Productive" },
    { key: "learning", label: "Learning" },
    { key: "research", label: "Research" },
    { key: "communication", label: "Communication" },
    { key: "unproductive", label: "Unproductive" },
    { key: "neutral", label: "Neutral" },
];

export const ACTIVITY_FACET_COLORS: Record<ActivityFacet, string> = {
    all: "#9ca3af",
    productive: "#4ade80",
    learning: "#60a5fa",
    research: "#22d3ee",
    communication: "#a78bfa",
    unproductive: "#fb7185",
    neutral: "#f59e0b",
};

export {
    EDGE_TYPE_LABELS,
    NODE_TYPE_META,
    activityFacetForNode,
    describeNode,
    formatDateTime,
    formatTime,
    memoryTypeForNode,
    nodeDomain,
    shorten,
    typeCountBadge,
} from "./graphInsights";

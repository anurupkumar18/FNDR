import { useCallback, useEffect, useMemo, useState } from "react";
import { getGraphData, GraphEdgeData, GraphNodeData } from "../api/tauri";
import "./GraphPanel.css";

interface GraphPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type GraphNodeType = "MemoryChunk" | "Entity" | "Task" | "Url";
type ViewMode = "timeline" | "cluster" | "focus" | "journey";
type ClusterLens = "app" | "memoryType" | "domain" | "session";
type ActivityFacet =
    | "all"
    | "productive"
    | "unproductive"
    | "learning"
    | "communication"
    | "research"
    | "neutral";

interface TypedGraphNode extends GraphNodeData {
    node_type: GraphNodeType;
}

interface SessionGroup {
    id: string;
    label: string;
    memoryIds: string[];
    firstTs: number;
    lastTs: number;
}

interface ClusterGroup {
    key: string;
    label: string;
    nodeIds: string[];
    typeCounts: Record<GraphNodeType, number>;
    edgeTouches: number;
}

interface FocusLayout {
    positions: Map<string, { x: number; y: number; ring: 0 | 1 | 2 }>;
    displayNodeIds: Set<string>;
    centerId: string | null;
}

const VIEW_MODES: Array<{ key: ViewMode; label: string; description: string }> = [
    { key: "timeline", label: "Timeline", description: "What happened?" },
    { key: "cluster", label: "Cluster", description: "What areas exist?" },
    { key: "focus", label: "Focus", description: "What is connected?" },
    { key: "journey", label: "Journey", description: "How did I move?" },
];

const CLUSTER_LENSES: Array<{ key: ClusterLens; label: string }> = [
    { key: "app", label: "App" },
    { key: "memoryType", label: "Memory Type" },
    { key: "domain", label: "Domain" },
    { key: "session", label: "Session" },
];

const NODE_TYPE_META: Record<
    GraphNodeType,
    { label: string; color: string; short: string }
> = {
    MemoryChunk: { label: "Memories", color: "#60a5fa", short: "M" },
    Entity: { label: "Entities", color: "#a78bfa", short: "E" },
    Task: { label: "Tasks", color: "#fb923c", short: "T" },
    Url: { label: "Links", color: "#34d399", short: "L" },
};

const NODE_TYPE_ORDER: GraphNodeType[] = ["MemoryChunk", "Entity", "Task", "Url"];

const EDGE_TYPE_LABELS: Record<string, string> = {
    PART_OF_SESSION: "Session link",
    REFERENCE_FOR_TASK: "Task reference",
    OCCURRED_AT: "Occurred at",
};

const ACTIVITY_FACETS: Array<{ key: ActivityFacet; label: string }> = [
    { key: "all", label: "All Activity" },
    { key: "productive", label: "Productive" },
    { key: "learning", label: "Learning" },
    { key: "research", label: "Research" },
    { key: "communication", label: "Communication" },
    { key: "unproductive", label: "Unproductive" },
    { key: "neutral", label: "Neutral" },
];

const ACTIVITY_FACET_COLORS: Record<ActivityFacet, string> = {
    all: "#9ca3af",
    productive: "#4ade80",
    learning: "#60a5fa",
    research: "#22d3ee",
    communication: "#a78bfa",
    unproductive: "#fb7185",
    neutral: "#f59e0b",
};

function safeString(value: unknown): string {
    return typeof value === "string" ? value.trim() : "";
}

function isGraphNodeType(value: string): value is GraphNodeType {
    return NODE_TYPE_ORDER.includes(value as GraphNodeType);
}

function parseHost(rawUrl: string): string {
    if (!rawUrl) {
        return "";
    }

    try {
        const normalized = rawUrl.startsWith("http") ? rawUrl : `https://${rawUrl}`;
        return new URL(normalized).host;
    } catch {
        return rawUrl
            .replace(/^https?:\/\//i, "")
            .split("/")[0]
            .trim();
    }
}

function formatDateTime(ts: number): string {
    return new Date(ts).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
    });
}

function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString(undefined, {
        hour: "numeric",
        minute: "2-digit",
    });
}

function shorten(value: string, limit = 90): string {
    if (value.length <= limit) {
        return value;
    }
    return `${value.slice(0, Math.max(0, limit - 1))}…`;
}

function classifyMemoryTypeFromApp(appName: string): string {
    const app = appName.toLowerCase();
    if (
        app.includes("safari") ||
        app.includes("chrome") ||
        app.includes("arc") ||
        app.includes("brave") ||
        app.includes("firefox") ||
        app.includes("edge")
    ) {
        return "web";
    }
    if (
        app.includes("code") ||
        app.includes("terminal") ||
        app.includes("xcode") ||
        app.includes("iterm")
    ) {
        return "development";
    }
    if (
        app.includes("meeting") ||
        app.includes("zoom") ||
        app.includes("teams")
    ) {
        return "meeting";
    }
    if (
        app.includes("mail") ||
        app.includes("slack") ||
        app.includes("messages") ||
        app.includes("discord")
    ) {
        return "communication";
    }
    if (
        app.includes("docs") ||
        app.includes("notion") ||
        app.includes("word") ||
        app.includes("pages") ||
        app.includes("preview") ||
        app.includes("pdf")
    ) {
        return "documents";
    }
    return "general";
}

function memoryTypeForNode(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};
    const declared = safeString(metadata.memory_type);
    if (declared) {
        return declared;
    }

    const app = safeString(metadata.app_name);
    return classifyMemoryTypeFromApp(app);
}

function includesAny(text: string, keywords: string[]): boolean {
    return keywords.some((keyword) => text.includes(keyword));
}

function nodeActivityText(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};
    const parts = [
        node.label,
        safeString(metadata.app_name),
        safeString(metadata.window_title),
        safeString(metadata.source_app),
        safeString(metadata.url),
        safeString(metadata.host),
        safeString(metadata.memory_type),
    ];

    return parts.join(" ").toLowerCase();
}

function nodeDomain(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};

    if (node.node_type === "Url") {
        return safeString(metadata.host) || parseHost(node.label);
    }

    const url = safeString(metadata.url);
    if (url) {
        return parseHost(url);
    }

    return "";
}

function activityFacetForNode(node: GraphNodeData): Exclude<ActivityFacet, "all"> {
    if (node.node_type === "Entity") {
        return "neutral";
    }
    if (node.node_type === "Task") {
        return "productive";
    }

    const text = nodeActivityText(node);

    if (
        includesAny(text, [
            "youtube",
            "netflix",
            "tiktok",
            "instagram",
            "facebook",
            "x.com",
            "twitter",
            "reddit",
            "twitch",
            "hulu",
            "steam",
            "game",
            "gaming",
        ])
    ) {
        return "unproductive";
    }

    if (
        includesAny(text, [
            "course",
            "tutorial",
            "udemy",
            "coursera",
            "khan",
            "lecture",
            "lesson",
            "learn",
            "readme",
            "docs",
            "documentation",
            "stack overflow",
            "leetcode",
            "arxiv",
        ])
    ) {
        return "learning";
    }

    if (
        includesAny(text, [
            "slack",
            "gmail",
            "outlook",
            "mail",
            "zoom",
            "teams",
            "discord",
            "messages",
            "whatsapp",
            "meet",
            "calendar invite",
        ])
    ) {
        return "communication";
    }

    if (
        includesAny(text, [
            "google search",
            "search",
            "perplexity",
            "wikipedia",
            "scholar",
            "pubmed",
            "investigate",
            "analysis",
            "research",
        ])
    ) {
        return "research";
    }

    if (
        includesAny(text, [
            "vscode",
            "code",
            "terminal",
            "xcode",
            "iterm",
            "github",
            "gitlab",
            "notion",
            "linear",
            "jira",
            "figma",
            "spreadsheet",
            "excel",
            "powerpoint",
            "doc",
        ])
    ) {
        return "productive";
    }

    const memoryType = memoryTypeForNode(node);
    if (memoryType === "development" || memoryType === "documents") {
        return "productive";
    }
    if (memoryType === "communication" || memoryType === "meeting") {
        return "communication";
    }
    if (memoryType === "web") {
        return "research";
    }

    return "neutral";
}

function nodeMatchesActivityFacet(node: GraphNodeData, facet: ActivityFacet): boolean {
    if (facet === "all") {
        return true;
    }
    return activityFacetForNode(node) === facet;
}

function sessionIdForMemory(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};
    const sessionId = safeString(metadata.session_id);
    if (sessionId) {
        return sessionId;
    }

    const bucket = new Date(node.created_at).toISOString().slice(0, 13);
    return `unknown-${bucket}`;
}

function buildAdjacency(
    edges: GraphEdgeData[],
    nodeMap: Map<string, TypedGraphNode>
): Map<string, Set<string>> {
    const adjacency = new Map<string, Set<string>>();

    for (const edge of edges) {
        if (!nodeMap.has(edge.source) || !nodeMap.has(edge.target)) {
            continue;
        }

        if (!adjacency.has(edge.source)) {
            adjacency.set(edge.source, new Set());
        }
        if (!adjacency.has(edge.target)) {
            adjacency.set(edge.target, new Set());
        }

        adjacency.get(edge.source)?.add(edge.target);
        adjacency.get(edge.target)?.add(edge.source);
    }

    return adjacency;
}

function shortestPath(startId: string, endId: string, adjacency: Map<string, Set<string>>): string[] {
    if (!startId || !endId) {
        return [];
    }
    if (startId === endId) {
        return [startId];
    }

    const visited = new Set<string>([startId]);
    const queue: string[] = [startId];
    const previous = new Map<string, string>();

    while (queue.length > 0) {
        const current = queue.shift();
        if (!current) {
            break;
        }

        const neighbors = adjacency.get(current);
        if (!neighbors) {
            continue;
        }

        for (const next of neighbors) {
            if (visited.has(next)) {
                continue;
            }
            visited.add(next);
            previous.set(next, current);

            if (next === endId) {
                const path = [endId];
                let cursor = endId;
                while (previous.has(cursor)) {
                    const prev = previous.get(cursor);
                    if (!prev) {
                        break;
                    }
                    path.push(prev);
                    cursor = prev;
                }
                return path.reverse();
            }

            queue.push(next);
        }
    }

    return [];
}

function relationSignals(a: GraphNodeData, b: GraphNodeData): string[] {
    const out: string[] = [];

    const aApp = safeString(a.metadata?.app_name);
    const bApp = safeString(b.metadata?.app_name);
    if (aApp && bApp && aApp === bApp) {
        out.push(`same app: ${aApp}`);
    }

    const aType = memoryTypeForNode(a);
    const bType = memoryTypeForNode(b);
    if (aType && bType && aType === bType) {
        out.push(`same type: ${aType}`);
    }

    const aDomain = nodeDomain(a);
    const bDomain = nodeDomain(b);
    if (aDomain && bDomain && aDomain === bDomain) {
        out.push(`revisited domain: ${aDomain}`);
    }

    const aSession = sessionIdForMemory(a);
    const bSession = sessionIdForMemory(b);
    if (aSession && bSession && aSession === bSession) {
        out.push("same session");
    }

    return out;
}

function describeNode(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};

    if (node.node_type === "MemoryChunk") {
        const app = safeString(metadata.app_name) || "Unknown app";
        const domain = nodeDomain(node);
        const memoryType = memoryTypeForNode(node);
        if (domain) {
            return `${app} • ${memoryType} • ${domain}`;
        }
        return `${app} • ${memoryType}`;
    }

    if (node.node_type === "Task") {
        const sourceApp = safeString(metadata.source_app);
        const taskType = safeString(metadata.task_type) || "Task";
        return sourceApp ? `${taskType} • ${sourceApp}` : taskType;
    }

    if (node.node_type === "Entity") {
        const entityType = safeString(metadata.entity_type) || "Entity";
        return entityType;
    }

    return nodeDomain(node) || "Link";
}

function clusterKeyForNode(node: GraphNodeData, lens: ClusterLens): string {
    const metadata = node.metadata ?? {};

    if (lens === "app") {
        if (node.node_type === "MemoryChunk") {
            return safeString(metadata.app_name) || "Unknown app";
        }
        if (node.node_type === "Task") {
            return safeString(metadata.source_app) || "Unknown app";
        }
        if (node.node_type === "Url") {
            return nodeDomain(node) || "Web";
        }
        return safeString(metadata.entity_type) || "Entity";
    }

    if (lens === "memoryType") {
        if (node.node_type === "MemoryChunk") {
            return memoryTypeForNode(node);
        }
        if (node.node_type === "Task") {
            return "task";
        }
        if (node.node_type === "Url") {
            return "web";
        }
        return safeString(metadata.entity_type) || "entity";
    }

    if (lens === "domain") {
        const domain = nodeDomain(node);
        if (domain) {
            return domain;
        }
        if (node.node_type === "MemoryChunk") {
            return safeString(metadata.app_name) || "no-domain";
        }
        return node.node_type.toLowerCase();
    }

    if (node.node_type === "MemoryChunk") {
        return sessionIdForMemory(node);
    }
    if (node.node_type === "Entity") {
        return safeString(metadata.session_id) || "session-entity";
    }
    return node.node_type.toLowerCase();
}

function radialPositions(keys: string[], width: number, height: number): Map<string, { x: number; y: number }> {
    const unique = [...new Set(keys)];
    const centerX = width / 2;
    const centerY = height / 2;
    const radius = Math.min(width, height) * 0.36;
    const map = new Map<string, { x: number; y: number }>();

    if (unique.length === 0) {
        return map;
    }

    if (unique.length === 1) {
        map.set(unique[0], { x: centerX, y: centerY });
        return map;
    }

    unique.forEach((key, index) => {
        const angle = (index / unique.length) * Math.PI * 2 - Math.PI / 2;
        map.set(key, {
            x: centerX + Math.cos(angle) * radius,
            y: centerY + Math.sin(angle) * radius,
        });
    });

    return map;
}

function typeCountBadge(counts: Record<GraphNodeType, number>): string {
    return NODE_TYPE_ORDER.map((type) => `${NODE_TYPE_META[type].short}:${counts[type] ?? 0}`).join(" • ");
}

export function GraphPanel({ isVisible, onClose }: GraphPanelProps) {
    const [rawNodes, setRawNodes] = useState<GraphNodeData[]>([]);
    const [rawEdges, setRawEdges] = useState<GraphEdgeData[]>([]);
    const [loading, setLoading] = useState(true);

    const [viewMode, setViewMode] = useState<ViewMode>("timeline");
    const [clusterLens, setClusterLens] = useState<ClusterLens>("app");
    const [activityFacet, setActivityFacet] = useState<ActivityFacet>("all");
    const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
    const [selectedClusterKey, setSelectedClusterKey] = useState<string | null>(null);
    const [journeyStartId, setJourneyStartId] = useState<string>("");
    const [journeyEndId, setJourneyEndId] = useState<string>("");

    useEffect(() => {
        if (!isVisible) {
            return;
        }

        setLoading(true);
        getGraphData()
            .then((data) => {
                setRawNodes(data.nodes);
                setRawEdges(data.edges);
            })
            .catch((err) => {
                console.error("Failed to load graph data:", err);
            })
            .finally(() => {
                setLoading(false);
            });
    }, [isVisible]);

    const activityFacetCounts = useMemo(() => {
        const counts: Record<ActivityFacet, number> = {
            all: 0,
            productive: 0,
            unproductive: 0,
            learning: 0,
            communication: 0,
            research: 0,
            neutral: 0,
        };
        for (const node of rawNodes) {
            if (!isGraphNodeType(node.node_type)) {
                continue;
            }
            counts.all += 1;
            counts[activityFacetForNode(node)] += 1;
        }
        return counts;
    }, [rawNodes]);

    const filteredNodes = useMemo<TypedGraphNode[]>(
        () =>
            rawNodes.filter(
                (node) => isGraphNodeType(node.node_type) && nodeMatchesActivityFacet(node, activityFacet)
            ) as TypedGraphNode[],
        [rawNodes, activityFacet]
    );

    const filteredNodeMap = useMemo(() => {
        const map = new Map<string, TypedGraphNode>();
        filteredNodes.forEach((node) => map.set(node.id, node));
        return map;
    }, [filteredNodes]);

    const filteredEdges = useMemo(
        () => rawEdges.filter((edge) => filteredNodeMap.has(edge.source) && filteredNodeMap.has(edge.target)),
        [rawEdges, filteredNodeMap]
    );

    const edgeByPair = useMemo(() => {
        const map = new Map<string, GraphEdgeData>();
        for (const edge of filteredEdges) {
            const keyA = `${edge.source}|${edge.target}`;
            const keyB = `${edge.target}|${edge.source}`;
            map.set(keyA, edge);
            map.set(keyB, edge);
        }
        return map;
    }, [filteredEdges]);

    const adjacency = useMemo(() => buildAdjacency(filteredEdges, filteredNodeMap), [filteredEdges, filteredNodeMap]);

    const neighborCounts = useMemo(() => {
        const map = new Map<string, number>();
        adjacency.forEach((neighbors, key) => map.set(key, neighbors.size));
        return map;
    }, [adjacency]);

    const sortedNodesByTime = useMemo(
        () => [...filteredNodes].sort((a, b) => b.created_at - a.created_at),
        [filteredNodes]
    );

    useEffect(() => {
        if (!isVisible || sortedNodesByTime.length === 0) {
            return;
        }

        if (!selectedNodeId || !filteredNodeMap.has(selectedNodeId)) {
            setSelectedNodeId(sortedNodesByTime[0].id);
        }

        if (!journeyStartId || !filteredNodeMap.has(journeyStartId)) {
            setJourneyStartId(sortedNodesByTime[Math.min(5, sortedNodesByTime.length - 1)].id);
        }

        if (!journeyEndId || !filteredNodeMap.has(journeyEndId)) {
            setJourneyEndId(sortedNodesByTime[0].id);
        }
    }, [
        isVisible,
        sortedNodesByTime,
        selectedNodeId,
        journeyStartId,
        journeyEndId,
        filteredNodeMap,
    ]);

    const memoryNodes = useMemo(
        () => sortedNodesByTime.filter((node) => node.node_type === "MemoryChunk"),
        [sortedNodesByTime]
    );

    const sessionLabels = useMemo(() => {
        const map = new Map<string, string>();
        for (const node of rawNodes) {
            if (node.node_type !== "Entity") {
                continue;
            }
            const sessionId = safeString(node.metadata?.session_id);
            if (sessionId) {
                map.set(sessionId, node.label || `Session ${sessionId.slice(0, 6)}`);
            }
        }
        return map;
    }, [rawNodes]);

    const relationSignalsByMemory = useMemo(() => {
        const map = new Map<string, string[]>();

        for (let i = 0; i < memoryNodes.length - 1; i++) {
            const current = memoryNodes[i];
            const previous = memoryNodes[i + 1];
            const signals = relationSignals(current, previous);
            if (signals.length > 0) {
                map.set(current.id, signals.slice(0, 2));
            }
        }

        return map;
    }, [memoryNodes]);

    const sessions = useMemo(() => {
        const grouping = new Map<string, SessionGroup>();

        for (const memory of memoryNodes) {
            const sessionId = sessionIdForMemory(memory);
            if (!grouping.has(sessionId)) {
                const sessionLabel = sessionLabels.get(sessionId) || `Session ${sessionId.slice(0, 8)}`;
                grouping.set(sessionId, {
                    id: sessionId,
                    label: sessionLabel,
                    memoryIds: [],
                    firstTs: memory.created_at,
                    lastTs: memory.created_at,
                });
            }

            const bucket = grouping.get(sessionId);
            if (!bucket) {
                continue;
            }

            bucket.memoryIds.push(memory.id);
            bucket.firstTs = Math.min(bucket.firstTs, memory.created_at);
            bucket.lastTs = Math.max(bucket.lastTs, memory.created_at);
        }

        return [...grouping.values()]
            .sort((a, b) => b.lastTs - a.lastTs)
            .map((session) => ({
                ...session,
                memoryIds: [...session.memoryIds].sort((a, b) => {
                    const left = filteredNodeMap.get(a)?.created_at ?? 0;
                    const right = filteredNodeMap.get(b)?.created_at ?? 0;
                    return right - left;
                }),
            }));
    }, [memoryNodes, sessionLabels, filteredNodeMap]);

    const clusters = useMemo(() => {
        const map = new Map<string, ClusterGroup>();

        for (const node of filteredNodes) {
            const key = clusterKeyForNode(node, clusterLens) || "unassigned";
            if (!map.has(key)) {
                map.set(key, {
                    key,
                    label: key,
                    nodeIds: [],
                    typeCounts: {
                        MemoryChunk: 0,
                        Entity: 0,
                        Task: 0,
                        Url: 0,
                    },
                    edgeTouches: 0,
                });
            }

            const cluster = map.get(key);
            if (!cluster) {
                continue;
            }

            cluster.nodeIds.push(node.id);
            cluster.typeCounts[node.node_type] += 1;
        }

        for (const edge of filteredEdges) {
            const source = filteredNodeMap.get(edge.source);
            const target = filteredNodeMap.get(edge.target);
            if (!source || !target) {
                continue;
            }

            const sourceKey = clusterKeyForNode(source, clusterLens) || "unassigned";
            const targetKey = clusterKeyForNode(target, clusterLens) || "unassigned";

            if (sourceKey === targetKey) {
                const cluster = map.get(sourceKey);
                if (cluster) {
                    cluster.edgeTouches += 1;
                }
                continue;
            }

            const left = map.get(sourceKey);
            const right = map.get(targetKey);
            if (left) {
                left.edgeTouches += 1;
            }
            if (right) {
                right.edgeTouches += 1;
            }
        }

        return [...map.values()].sort((a, b) => b.nodeIds.length - a.nodeIds.length);
    }, [filteredNodes, filteredEdges, filteredNodeMap, clusterLens]);

    const clusterConnections = useMemo(() => {
        const counts = new Map<string, number>();

        for (const edge of filteredEdges) {
            const source = filteredNodeMap.get(edge.source);
            const target = filteredNodeMap.get(edge.target);
            if (!source || !target) {
                continue;
            }

            const sourceKey = clusterKeyForNode(source, clusterLens) || "unassigned";
            const targetKey = clusterKeyForNode(target, clusterLens) || "unassigned";
            if (sourceKey === targetKey) {
                continue;
            }

            const pair = [sourceKey, targetKey].sort().join("|");
            counts.set(pair, (counts.get(pair) ?? 0) + 1);
        }

        return counts;
    }, [filteredEdges, filteredNodeMap, clusterLens]);

    const topClusters = useMemo(() => clusters.slice(0, 10), [clusters]);

    const clusterPositions = useMemo(() => {
        const keys = topClusters.map((cluster) => cluster.key);
        return radialPositions(keys, 980, 430);
    }, [topClusters]);

    useEffect(() => {
        if (topClusters.length === 0) {
            setSelectedClusterKey(null);
            return;
        }

        if (!selectedClusterKey || !topClusters.some((cluster) => cluster.key === selectedClusterKey)) {
            setSelectedClusterKey(topClusters[0].key);
        }
    }, [topClusters, selectedClusterKey]);

    const selectedCluster = useMemo(
        () => topClusters.find((cluster) => cluster.key === selectedClusterKey) ?? null,
        [topClusters, selectedClusterKey]
    );

    const focusLayout = useMemo<FocusLayout>(() => {
        if (filteredNodes.length === 0) {
            return {
                positions: new Map(),
                displayNodeIds: new Set(),
                centerId: null,
            };
        }

        const centerId =
            selectedNodeId && filteredNodeMap.has(selectedNodeId)
                ? selectedNodeId
                : filteredNodes[0].id;

        const firstDegree = [...(adjacency.get(centerId) ?? new Set())].slice(0, 10);
        const secondDegreeSet = new Set<string>();

        for (const first of firstDegree) {
            const neighbors = adjacency.get(first);
            if (!neighbors) {
                continue;
            }
            for (const candidate of neighbors) {
                if (candidate === centerId || firstDegree.includes(candidate)) {
                    continue;
                }
                secondDegreeSet.add(candidate);
                if (secondDegreeSet.size >= 14) {
                    break;
                }
            }
            if (secondDegreeSet.size >= 14) {
                break;
            }
        }

        const secondDegree = [...secondDegreeSet];
        const displayNodeIds = new Set<string>([centerId, ...firstDegree, ...secondDegree]);
        const positions = new Map<string, { x: number; y: number; ring: 0 | 1 | 2 }>();

        const centerX = 490;
        const centerY = 220;
        positions.set(centerId, { x: centerX, y: centerY, ring: 0 });

        const firstRadius = 132;
        firstDegree.forEach((nodeId, index) => {
            const angle = (index / Math.max(firstDegree.length, 1)) * Math.PI * 2 - Math.PI / 2;
            positions.set(nodeId, {
                x: centerX + Math.cos(angle) * firstRadius,
                y: centerY + Math.sin(angle) * firstRadius,
                ring: 1,
            });
        });

        const secondRadius = 212;
        secondDegree.forEach((nodeId, index) => {
            const angle = (index / Math.max(secondDegree.length, 1)) * Math.PI * 2 - Math.PI / 2;
            positions.set(nodeId, {
                x: centerX + Math.cos(angle) * secondRadius,
                y: centerY + Math.sin(angle) * secondRadius,
                ring: 2,
            });
        });

        return {
            positions,
            displayNodeIds,
            centerId,
        };
    }, [filteredNodes, selectedNodeId, filteredNodeMap, adjacency]);

    const focusEdges = useMemo(
        () =>
            filteredEdges.filter(
                (edge) =>
                    focusLayout.displayNodeIds.has(edge.source) &&
                    focusLayout.displayNodeIds.has(edge.target)
            ),
        [filteredEdges, focusLayout.displayNodeIds]
    );

    const focusNeighborNodes = useMemo(() => {
        if (!focusLayout.centerId) {
            return [];
        }

        const neighbors = [...(adjacency.get(focusLayout.centerId) ?? new Set())];
        return neighbors
            .map((id) => filteredNodeMap.get(id))
            .filter((node): node is TypedGraphNode => Boolean(node))
            .sort((a, b) => b.created_at - a.created_at)
            .slice(0, 12);
    }, [focusLayout.centerId, adjacency, filteredNodeMap]);

    const journeyOptions = useMemo(() => sortedNodesByTime.slice(0, 120), [sortedNodesByTime]);

    const journeyPath = useMemo(
        () => shortestPath(journeyStartId, journeyEndId, adjacency),
        [journeyStartId, journeyEndId, adjacency]
    );

    const timelineBridge = useMemo(() => {
        if (journeyPath.length > 0 || !journeyStartId || !journeyEndId) {
            return [] as GraphNodeData[];
        }

        const start = filteredNodeMap.get(journeyStartId);
        const end = filteredNodeMap.get(journeyEndId);
        if (!start || !end) {
            return [];
        }

        const minTs = Math.min(start.created_at, end.created_at);
        const maxTs = Math.max(start.created_at, end.created_at);

        return memoryNodes
            .filter((node) => node.created_at >= minTs && node.created_at <= maxTs)
            .slice(0, 8);
    }, [journeyPath.length, journeyStartId, journeyEndId, filteredNodeMap, memoryNodes]);

    const journeyWeb = useMemo(() => {
        const pathIds = journeyPath.slice(0, 12);
        if (pathIds.length === 0) {
            return {
                points: [] as Array<{ id: string; x: number; y: number; branch: boolean }>,
                edges: [] as Array<{ source: string; target: string; branch: boolean }>,
            };
        }

        const spacing = pathIds.length > 1 ? 860 / (pathIds.length - 1) : 0;
        const points: Array<{ id: string; x: number; y: number; branch: boolean }> = pathIds.map((id, index) => ({
            id,
            x: 60 + index * spacing,
            y: 120 + Math.sin(index * 0.9) * 30,
            branch: false,
        }));

        const pathSet = new Set(pathIds);
        const edges: Array<{ source: string; target: string; branch: boolean }> = [];

        for (let i = 0; i < pathIds.length - 1; i++) {
            edges.push({ source: pathIds[i], target: pathIds[i + 1], branch: false });
        }

        pathIds.forEach((id, index) => {
            const neighbors = [...(adjacency.get(id) ?? new Set())].filter((candidate) => !pathSet.has(candidate));
            const branchId = neighbors[0];
            if (!branchId) {
                return;
            }

            const source = points.find((point) => point.id === id);
            if (!source) {
                return;
            }

            const offsetY = index % 2 === 0 ? -60 : 62;
            points.push({
                id: branchId,
                x: source.x + (index % 2 === 0 ? -18 : 18),
                y: source.y + offsetY,
                branch: true,
            });
            edges.push({ source: id, target: branchId, branch: true });
        });

        return { points, edges };
    }, [journeyPath, adjacency]);

    const openFocus = useCallback((nodeId: string) => {
        setSelectedNodeId(nodeId);
        setViewMode("focus");
    }, []);

    const productivityScore = useMemo(() => {
        const positive =
            activityFacetCounts.productive +
            activityFacetCounts.learning +
            activityFacetCounts.research;
        const negative = activityFacetCounts.unproductive;
        const total = Math.max(activityFacetCounts.all, 1);

        return {
            positive,
            negative,
            positivePct: ((positive / total) * 100).toFixed(0),
            negativePct: ((negative / total) * 100).toFixed(0),
        };
    }, [activityFacetCounts]);

    if (!isVisible) {
        return null;
    }

    return (
        <div className="graph-panel">
            <div className="graph-header">
                <div className="graph-title-wrap">
                    <h2>Knowledge Graph</h2>
                    <p>Multi-view memory graph: timeline, clusters, focused ego graph, and journeys.</p>
                    <div className="graph-stats-row">
                        <span>◉ {filteredNodes.length}/{rawNodes.length} nodes</span>
                        <span>─ {filteredEdges.length}/{rawEdges.length} edges</span>
                        <span>◷ {sessions.length} sessions</span>
                        <span>▲ {productivityScore.positivePct}% productive-like</span>
                        <span>▼ {productivityScore.negativePct}% unproductive</span>
                    </div>
                </div>
                <button className="ui-action-btn graph-close-btn" onClick={onClose}>
                    ✕ Close
                </button>
            </div>

            <div className="graph-top-controls">
                <div className="graph-mode-tabs" role="tablist" aria-label="Graph Views">
                    {VIEW_MODES.map((mode) => (
                        <button
                            key={mode.key}
                            className={`ui-action-btn graph-mode-tab ${viewMode === mode.key ? "active" : ""}`}
                            onClick={() => setViewMode(mode.key)}
                            title={mode.description}
                        >
                            {mode.label}
                        </button>
                    ))}
                </div>

                <div className="graph-type-filters">
                    {ACTIVITY_FACETS.map((facet) => (
                        <button
                            key={facet.key}
                            className={`graph-type-filter ${activityFacet === facet.key ? "active" : "inactive"}`}
                            onClick={() => setActivityFacet(facet.key)}
                        >
                            <span
                                className="graph-type-dot"
                                style={{ background: ACTIVITY_FACET_COLORS[facet.key] }}
                            />
                            {facet.label}
                            <strong>{activityFacetCounts[facet.key]}</strong>
                        </button>
                    ))}
                </div>
            </div>

            <div className="graph-view-shell">
                {loading ? (
                    <div className="graph-loading">
                        <div className="spinner" />
                        Loading graph data...
                    </div>
                ) : filteredNodes.length === 0 ? (
                    <div className="graph-empty">
                        <p>No visible graph data for the selected activity filter.</p>
                    </div>
                ) : (
                    <>
                        {viewMode === "timeline" && (
                            <div className="graph-view timeline-view">
                                <div className="view-header">
                                    <h3>Session Timeline</h3>
                                    <p>Chronological sessions with lightweight relationship signals.</p>
                                </div>
                                <div className="timeline-list">
                                    {sessions.map((session) => {
                                        const miniIds = session.memoryIds.slice(0, 12);
                                        const miniPoints = miniIds.map((id, index) => {
                                            const step = miniIds.length > 1 ? 840 / (miniIds.length - 1) : 0;
                                            return {
                                                id,
                                                x: 44 + index * step,
                                                y: 48 + Math.sin(index * 0.8) * 18 + ((index % 3) - 1) * 8,
                                            };
                                        });
                                        const miniPointMap = new Map(miniPoints.map((point) => [point.id, point]));
                                        const miniEdges: Array<{ source: string; target: string; relation: boolean }> = [];

                                        for (let i = 0; i < miniIds.length - 1; i++) {
                                            miniEdges.push({ source: miniIds[i], target: miniIds[i + 1], relation: false });
                                        }

                                        for (let i = 0; i < miniIds.length; i++) {
                                            for (let j = i + 2; j < Math.min(miniIds.length, i + 6); j++) {
                                                const source = miniIds[i];
                                                const target = miniIds[j];
                                                if (adjacency.get(source)?.has(target)) {
                                                    miniEdges.push({ source, target, relation: true });
                                                }
                                            }
                                        }

                                        return (
                                            <article key={session.id} className="timeline-session">
                                            <header className="timeline-session-header">
                                                <div>
                                                    <h4>{session.label}</h4>
                                                    <p>
                                                        {formatDateTime(session.firstTs)} to {formatDateTime(session.lastTs)}
                                                    </p>
                                                </div>
                                                <span>{session.memoryIds.length} memories</span>
                                            </header>

                                            {miniPoints.length > 1 && (
                                                <div className="timeline-session-graph">
                                                    <svg viewBox="0 0 930 108" className="timeline-session-graph-svg">
                                                        {miniEdges.map((edge, index) => {
                                                            const source = miniPointMap.get(edge.source);
                                                            const target = miniPointMap.get(edge.target);
                                                            if (!source || !target) {
                                                                return null;
                                                            }
                                                            return (
                                                                <line
                                                                    key={`${session.id}-edge-${index}`}
                                                                    x1={source.x}
                                                                    y1={source.y}
                                                                    x2={target.x}
                                                                    y2={target.y}
                                                                    className={edge.relation ? "edge-relation" : "edge-chain"}
                                                                />
                                                            );
                                                        })}

                                                        {miniPoints.map((point, index) => {
                                                            const node = filteredNodeMap.get(point.id);
                                                            if (!node) {
                                                                return null;
                                                            }
                                                            return (
                                                                <g
                                                                    key={`${session.id}-node-${point.id}`}
                                                                    className="timeline-graph-node"
                                                                    onClick={() => openFocus(point.id)}
                                                                >
                                                                    <circle
                                                                        cx={point.x}
                                                                        cy={point.y}
                                                                        r={index === 0 ? 7.6 : 6}
                                                                    />
                                                                    <text x={point.x} y={point.y - 11} textAnchor="middle">
                                                                        {index + 1}
                                                                    </text>
                                                                    <title>{node.label}</title>
                                                                </g>
                                                            );
                                                        })}
                                                    </svg>
                                                </div>
                                            )}

                                            <div className="timeline-items">
                                                {session.memoryIds.map((memoryId) => {
                                                    const memory = filteredNodeMap.get(memoryId);
                                                    if (!memory) {
                                                        return null;
                                                    }

                                                    const neighbors = [...(adjacency.get(memory.id) ?? new Set())]
                                                        .map((id) => filteredNodeMap.get(id))
                                                        .filter((node): node is TypedGraphNode => Boolean(node));

                                                    const byType = {
                                                        tasks: neighbors.filter((node) => node.node_type === "Task").length,
                                                        entities: neighbors.filter((node) => node.node_type === "Entity").length,
                                                        links: neighbors.filter((node) => node.node_type === "Url").length,
                                                    };

                                                    const relationSignals = relationSignalsByMemory.get(memory.id) ?? [];
                                                    const domain = nodeDomain(memory);
                                                    const activity = activityFacetForNode(memory);

                                                    return (
                                                        <div key={memory.id} className="timeline-item">
                                                            <button
                                                                className="timeline-item-main"
                                                                onClick={() => openFocus(memory.id)}
                                                            >
                                                                <div className="timeline-item-top">
                                                                    <span>{formatTime(memory.created_at)}</span>
                                                                    <span>{describeNode(memory)}</span>
                                                                </div>
                                                                <p>{shorten(memory.label || "Untitled memory", 150)}</p>
                                                            </button>

                                                            <div className="timeline-chip-row">
                                                                <span
                                                                    className="timeline-chip"
                                                                    style={{
                                                                        borderColor: `${ACTIVITY_FACET_COLORS[activity]}66`,
                                                                        color: ACTIVITY_FACET_COLORS[activity],
                                                                        background: `${ACTIVITY_FACET_COLORS[activity]}1A`,
                                                                    }}
                                                                >
                                                                    {ACTIVITY_FACETS.find((entry) => entry.key === activity)?.label}
                                                                </span>
                                                                <span className="timeline-chip">{memoryTypeForNode(memory)}</span>
                                                                {domain && <span className="timeline-chip">{domain}</span>}
                                                                {byType.tasks > 0 && (
                                                                    <span className="timeline-chip">{byType.tasks} task links</span>
                                                                )}
                                                                {byType.entities > 0 && (
                                                                    <span className="timeline-chip">{byType.entities} entities</span>
                                                                )}
                                                                {byType.links > 0 && (
                                                                    <span className="timeline-chip">{byType.links} URLs</span>
                                                                )}
                                                                {relationSignals.map((signal) => (
                                                                    <span key={signal} className="timeline-chip relation">
                                                                        {signal}
                                                                    </span>
                                                                ))}
                                                            </div>
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                            </article>
                                        );
                                    })}
                                </div>
                            </div>
                        )}

                        {viewMode === "cluster" && (
                            <div className="graph-view cluster-view">
                                <div className="view-header row">
                                    <div>
                                        <h3>Cluster Map</h3>
                                        <p>Island view for exploration before drilling into details.</p>
                                    </div>
                                    <div className="cluster-lens-tabs">
                                        {CLUSTER_LENSES.map((lens) => (
                                            <button
                                                key={lens.key}
                                                className={`ui-action-btn ${clusterLens === lens.key ? "active" : ""}`}
                                                onClick={() => setClusterLens(lens.key)}
                                            >
                                                {lens.label}
                                            </button>
                                        ))}
                                    </div>
                                </div>

                                <div className="cluster-layout">
                                    <div className="cluster-map-card">
                                        <svg viewBox="0 0 980 430" className="cluster-map">
                                            {[...clusterConnections.entries()].map(([pair, count]) => {
                                                const [leftKey, rightKey] = pair.split("|");
                                                const left = clusterPositions.get(leftKey);
                                                const right = clusterPositions.get(rightKey);
                                                if (!left || !right) {
                                                    return null;
                                                }
                                                return (
                                                    <line
                                                        key={pair}
                                                        x1={left.x}
                                                        y1={left.y}
                                                        x2={right.x}
                                                        y2={right.y}
                                                        stroke="rgba(148, 163, 184, 0.35)"
                                                        strokeWidth={Math.min(1 + count * 0.8, 5)}
                                                    />
                                                );
                                            })}

                                            {topClusters.map((cluster) => {
                                                const pos = clusterPositions.get(cluster.key);
                                                if (!pos) {
                                                    return null;
                                                }

                                                const radius = 18 + Math.min(cluster.nodeIds.length, 24);
                                                const isActive = cluster.key === selectedClusterKey;

                                                return (
                                                    <g
                                                        key={cluster.key}
                                                        className="cluster-node"
                                                        onClick={() => setSelectedClusterKey(cluster.key)}
                                                    >
                                                        <circle
                                                            cx={pos.x}
                                                            cy={pos.y}
                                                            r={radius}
                                                            fill={isActive ? "rgba(96, 165, 250, 0.35)" : "rgba(17, 24, 39, 0.84)"}
                                                            stroke={isActive ? "#60a5fa" : "rgba(148, 163, 184, 0.45)"}
                                                            strokeWidth={isActive ? 2 : 1}
                                                        />
                                                        <text x={pos.x} y={pos.y + 4} textAnchor="middle">
                                                            {cluster.nodeIds.length}
                                                        </text>
                                                        <text x={pos.x} y={pos.y + radius + 16} textAnchor="middle" className="cluster-label">
                                                            {shorten(cluster.label, 18)}
                                                        </text>
                                                    </g>
                                                );
                                            })}
                                        </svg>
                                    </div>

                                    <div className="cluster-side-panel">
                                        <div className="cluster-list">
                                            {topClusters.map((cluster) => (
                                                <button
                                                    key={cluster.key}
                                                    className={`cluster-list-item ${cluster.key === selectedClusterKey ? "active" : ""}`}
                                                    onClick={() => setSelectedClusterKey(cluster.key)}
                                                >
                                                    <div>
                                                        <strong>{cluster.label}</strong>
                                                        <p>{typeCountBadge(cluster.typeCounts)}</p>
                                                    </div>
                                                    <span>{cluster.nodeIds.length}</span>
                                                </button>
                                            ))}
                                        </div>

                                        {selectedCluster && (
                                            <div className="cluster-drilldown">
                                                <h4>{selectedCluster.label}</h4>
                                                <p>{selectedCluster.nodeIds.length} nodes in this cluster</p>
                                                <div className="cluster-members">
                                                    {selectedCluster.nodeIds.slice(0, 8).map((nodeId) => {
                                                        const node = filteredNodeMap.get(nodeId);
                                                        if (!node) {
                                                            return null;
                                                        }
                                                        return (
                                                            <button
                                                                key={node.id}
                                                                className="cluster-member"
                                                                onClick={() => openFocus(node.id)}
                                                            >
                                                                <span
                                                                    className="pill"
                                                                    style={{ background: `${NODE_TYPE_META[node.node_type].color}33` }}
                                                                >
                                                                    {NODE_TYPE_META[node.node_type].label}
                                                                </span>
                                                                <span>{shorten(node.label, 74)}</span>
                                                            </button>
                                                        );
                                                    })}
                                                </div>
                                            </div>
                                        )}
                                    </div>
                                </div>
                            </div>
                        )}

                        {viewMode === "focus" && (
                            <div className="graph-view focus-view">
                                <div className="view-header row">
                                    <div>
                                        <h3>Focus Graph</h3>
                                        <p>Ego graph for one memory/entity/task with first and second degree context.</p>
                                    </div>
                                    <select
                                        className="focus-node-select"
                                        value={focusLayout.centerId ?? ""}
                                        onChange={(event) => setSelectedNodeId(event.target.value)}
                                    >
                                        {sortedNodesByTime.slice(0, 150).map((node) => (
                                            <option key={node.id} value={node.id}>
                                                {NODE_TYPE_META[node.node_type].label} • {shorten(node.label, 70)}
                                            </option>
                                        ))}
                                    </select>
                                </div>

                                {focusLayout.centerId && filteredNodeMap.get(focusLayout.centerId) && (
                                    <>
                                        <div className="focus-summary">
                                            <span
                                                className="pill"
                                                style={{
                                                    background: `${NODE_TYPE_META[filteredNodeMap.get(focusLayout.centerId)?.node_type ?? "MemoryChunk"].color}33`,
                                                }}
                                            >
                                                {NODE_TYPE_META[filteredNodeMap.get(focusLayout.centerId)?.node_type ?? "MemoryChunk"].label}
                                            </span>
                                            <strong>{filteredNodeMap.get(focusLayout.centerId)?.label}</strong>
                                            <span>{neighborCounts.get(focusLayout.centerId) ?? 0} direct connections</span>
                                        </div>

                                        <div className="focus-layout">
                                            <div className="focus-map-card">
                                                <svg viewBox="0 0 980 430" className="focus-map">
                                                    {focusEdges.map((edge) => {
                                                        const source = focusLayout.positions.get(edge.source);
                                                        const target = focusLayout.positions.get(edge.target);
                                                        if (!source || !target) {
                                                            return null;
                                                        }
                                                        return (
                                                            <line
                                                                key={edge.id}
                                                                x1={source.x}
                                                                y1={source.y}
                                                                x2={target.x}
                                                                y2={target.y}
                                                                stroke="rgba(148, 163, 184, 0.45)"
                                                                strokeWidth={1.2}
                                                            />
                                                        );
                                                    })}

                                                    {[...focusLayout.displayNodeIds].map((nodeId) => {
                                                        const node = filteredNodeMap.get(nodeId);
                                                        const pos = focusLayout.positions.get(nodeId);
                                                        if (!node || !pos) {
                                                            return null;
                                                        }

                                                        const meta = NODE_TYPE_META[node.node_type];
                                                        const radius = pos.ring === 0 ? 18 : pos.ring === 1 ? 12 : 9;

                                                        return (
                                                            <g key={node.id} className="focus-node" onClick={() => setSelectedNodeId(node.id)}>
                                                                <circle
                                                                    cx={pos.x}
                                                                    cy={pos.y}
                                                                    r={radius}
                                                                    fill={node.id === focusLayout.centerId ? `${meta.color}66` : `${meta.color}2a`}
                                                                    stroke={meta.color}
                                                                    strokeWidth={node.id === focusLayout.centerId ? 2 : 1}
                                                                />
                                                                <text x={pos.x} y={pos.y + 4} textAnchor="middle" className="focus-short">
                                                                    {meta.short}
                                                                </text>
                                                                <text
                                                                    x={pos.x}
                                                                    y={pos.y + radius + 14}
                                                                    textAnchor="middle"
                                                                    className="focus-label"
                                                                >
                                                                    {shorten(node.label, 22)}
                                                                </text>
                                                            </g>
                                                        );
                                                    })}
                                                </svg>
                                            </div>

                                            <aside className="focus-side-panel">
                                                <h4>Direct Neighbors</h4>
                                                <div className="focus-neighbor-list">
                                                    {focusNeighborNodes.map((node) => (
                                                        <button
                                                            key={node.id}
                                                            className="focus-neighbor-item"
                                                            onClick={() => setSelectedNodeId(node.id)}
                                                        >
                                                            <span
                                                                className="pill"
                                                                style={{ background: `${NODE_TYPE_META[node.node_type].color}33` }}
                                                            >
                                                                {NODE_TYPE_META[node.node_type].label}
                                                            </span>
                                                            <span>{shorten(node.label, 80)}</span>
                                                        </button>
                                                    ))}
                                                    {focusNeighborNodes.length === 0 && (
                                                        <p className="inline-empty">No direct neighbors in current filters.</p>
                                                    )}
                                                </div>
                                            </aside>
                                        </div>
                                    </>
                                )}
                            </div>
                        )}

                        {viewMode === "journey" && (
                            <div className="graph-view journey-view">
                                <div className="view-header row">
                                    <div>
                                        <h3>Journey Path</h3>
                                        <p>Route between two memories or graph nodes.</p>
                                    </div>
                                </div>

                                {journeyWeb.points.length > 0 && (
                                    <div className="journey-web-card">
                                        <svg viewBox="0 0 980 240" className="journey-web-svg">
                                            {journeyWeb.edges.map((edge, index) => {
                                                const source = journeyWeb.points.find((point) => point.id === edge.source);
                                                const target = journeyWeb.points.find((point) => point.id === edge.target);
                                                if (!source || !target) {
                                                    return null;
                                                }
                                                return (
                                                    <line
                                                        key={`journey-edge-${index}`}
                                                        x1={source.x}
                                                        y1={source.y}
                                                        x2={target.x}
                                                        y2={target.y}
                                                        className={edge.branch ? "branch-edge" : "path-edge"}
                                                    />
                                                );
                                            })}

                                            {journeyWeb.points.map((point) => {
                                                const node = filteredNodeMap.get(point.id);
                                                if (!node) {
                                                    return null;
                                                }
                                                return (
                                                    <g
                                                        key={`journey-point-${point.id}-${point.branch ? "branch" : "path"}`}
                                                        className={point.branch ? "journey-branch-node" : "journey-path-node"}
                                                        onClick={() => openFocus(point.id)}
                                                    >
                                                        <circle cx={point.x} cy={point.y} r={point.branch ? 5 : 7} />
                                                        <title>{node.label}</title>
                                                    </g>
                                                );
                                            })}
                                        </svg>
                                    </div>
                                )}

                                <div className="journey-controls">
                                    <label>
                                        Start
                                        <select
                                            value={journeyStartId}
                                            onChange={(event) => setJourneyStartId(event.target.value)}
                                        >
                                            {journeyOptions.map((node) => (
                                                <option key={node.id} value={node.id}>
                                                    {formatDateTime(node.created_at)} • {shorten(node.label, 72)}
                                                </option>
                                            ))}
                                        </select>
                                    </label>

                                    <button
                                        className="ui-action-btn"
                                        onClick={() => {
                                            setJourneyStartId(journeyEndId);
                                            setJourneyEndId(journeyStartId);
                                        }}
                                    >
                                        Swap
                                    </button>

                                    <label>
                                        End
                                        <select
                                            value={journeyEndId}
                                            onChange={(event) => setJourneyEndId(event.target.value)}
                                        >
                                            {journeyOptions.map((node) => (
                                                <option key={node.id} value={node.id}>
                                                    {formatDateTime(node.created_at)} • {shorten(node.label, 72)}
                                                </option>
                                            ))}
                                        </select>
                                    </label>
                                </div>

                                {journeyPath.length > 0 ? (
                                    <div className="journey-chain">
                                        {journeyPath.map((nodeId, index) => {
                                            const node = filteredNodeMap.get(nodeId);
                                            if (!node) {
                                                return null;
                                            }

                                            const next = journeyPath[index + 1];
                                            const edge = next ? edgeByPair.get(`${nodeId}|${next}`) : null;

                                            return (
                                                <div key={node.id} className="journey-step-wrap">
                                                    <button
                                                        className="journey-step"
                                                        onClick={() => openFocus(node.id)}
                                                    >
                                                        <span
                                                            className="pill"
                                                            style={{ background: `${NODE_TYPE_META[node.node_type].color}33` }}
                                                        >
                                                            {NODE_TYPE_META[node.node_type].label}
                                                        </span>
                                                        <strong>{shorten(node.label, 82)}</strong>
                                                        <small>{describeNode(node)}</small>
                                                    </button>
                                                    {edge && (
                                                        <div className="journey-arrow">
                                                            <span>→</span>
                                                            <small>{EDGE_TYPE_LABELS[edge.edge_type] ?? edge.edge_type}</small>
                                                        </div>
                                                    )}
                                                </div>
                                            );
                                        })}
                                    </div>
                                ) : (
                                    <div className="journey-fallback">
                                        <p>
                                            No direct graph path found between those two nodes in current filters.
                                        </p>
                                        {timelineBridge.length > 0 && (
                                            <div className="journey-bridge">
                                                <h4>Temporal bridge</h4>
                                                {timelineBridge.map((node) => (
                                                    <button
                                                        key={node.id}
                                                        className="journey-bridge-item"
                                                        onClick={() => openFocus(node.id)}
                                                    >
                                                        <span>{formatDateTime(node.created_at)}</span>
                                                        <span>{shorten(node.label, 110)}</span>
                                                    </button>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        )}
                    </>
                )}
            </div>
        </div>
    );
}

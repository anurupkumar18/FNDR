import { GraphEdgeData, GraphNodeData } from "../../api/tauri";
import {
    ActivityFacet,
    ClusterConnection,
    ClusterInsight,
    ClusterLens,
    EdgePairIndex,
    FocusMode,
    FocusNeighborhood,
    GraphNodeType,
    JourneySemantics,
    NodeInsight,
    NodeTypeMeta,
    TimelineInsight,
    TypedGraphNode,
} from "./graphTypes";

export const NODE_TYPE_ORDER: GraphNodeType[] = ["MemoryChunk", "Entity", "Task", "Url"];

export const NODE_TYPE_META: Record<GraphNodeType, NodeTypeMeta> = {
    MemoryChunk: {
        label: "Memories",
        color: "#68b5ff",
        short: "M",
        softColor: "rgba(104, 181, 255, 0.18)",
    },
    Entity: {
        label: "Entities",
        color: "#b28bff",
        short: "E",
        softColor: "rgba(178, 139, 255, 0.18)",
    },
    Task: {
        label: "Tasks",
        color: "#ff9a5f",
        short: "T",
        softColor: "rgba(255, 154, 95, 0.18)",
    },
    Url: {
        label: "Links",
        color: "#3edba8",
        short: "L",
        softColor: "rgba(62, 219, 168, 0.18)",
    },
};

export const EDGE_TYPE_LABELS: Record<string, string> = {
    PART_OF_SESSION: "Same session",
    REFERENCE_FOR_TASK: "Task linkage",
    OCCURRED_AT: "Context/URL continuity",
};

const ACTIVITY_KEYWORDS = {
    unproductive: [
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
        "gaming",
    ],
    learning: [
        "course",
        "tutorial",
        "udemy",
        "coursera",
        "lecture",
        "lesson",
        "learn",
        "readme",
        "docs",
        "documentation",
        "stack overflow",
        "leetcode",
        "arxiv",
    ],
    communication: [
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
        "calendar",
    ],
    research: [
        "google search",
        "search",
        "perplexity",
        "wikipedia",
        "scholar",
        "pubmed",
        "investigate",
        "analysis",
        "research",
    ],
    productive: [
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
    ],
};

export function safeString(value: unknown): string {
    return typeof value === "string" ? value.trim() : "";
}

export function shorten(value: string, limit = 90): string {
    if (value.length <= limit) {
        return value;
    }
    return `${value.slice(0, Math.max(0, limit - 1))}…`;
}

export function formatDateTime(ts: number): string {
    return new Date(ts).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
    });
}

export function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString(undefined, {
        hour: "numeric",
        minute: "2-digit",
    });
}

export function isGraphNodeType(value: string): value is GraphNodeType {
    return NODE_TYPE_ORDER.includes(value as GraphNodeType);
}

export function asTypedGraphNodes(nodes: GraphNodeData[]): TypedGraphNode[] {
    return nodes.filter((node) => isGraphNodeType(node.node_type)) as TypedGraphNode[];
}

export function parseHost(rawUrl: string): string {
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

export function classifyMemoryTypeFromApp(appName: string): string {
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

    if (app.includes("meeting") || app.includes("zoom") || app.includes("teams")) {
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

export function memoryTypeForNode(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};
    const declared = safeString(metadata.memory_type);
    if (declared) {
        return declared;
    }

    const app = safeString(metadata.app_name);
    return classifyMemoryTypeFromApp(app);
}

export function nodeDomain(node: GraphNodeData): string {
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

function includesAny(text: string, keywords: string[]): boolean {
    return keywords.some((keyword) => text.includes(keyword));
}

function nodeActivityText(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};
    return [
        node.label,
        safeString(metadata.app_name),
        safeString(metadata.window_title),
        safeString(metadata.source_app),
        safeString(metadata.url),
        safeString(metadata.host),
        safeString(metadata.memory_type),
    ]
        .join(" ")
        .toLowerCase();
}

export function activityFacetForNode(node: GraphNodeData): Exclude<ActivityFacet, "all"> {
    if (node.node_type === "Entity") {
        return "neutral";
    }

    if (node.node_type === "Task") {
        return "productive";
    }

    const text = nodeActivityText(node);

    if (includesAny(text, ACTIVITY_KEYWORDS.unproductive)) {
        return "unproductive";
    }

    if (includesAny(text, ACTIVITY_KEYWORDS.learning)) {
        return "learning";
    }

    if (includesAny(text, ACTIVITY_KEYWORDS.communication)) {
        return "communication";
    }

    if (includesAny(text, ACTIVITY_KEYWORDS.research)) {
        return "research";
    }

    if (includesAny(text, ACTIVITY_KEYWORDS.productive)) {
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

export function nodeMatchesActivityFacet(node: GraphNodeData, facet: ActivityFacet): boolean {
    if (facet === "all") {
        return true;
    }
    return activityFacetForNode(node) === facet;
}

export function sessionIdForMemory(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};
    const sessionId = safeString(metadata.session_id);
    if (sessionId) {
        return sessionId;
    }

    const bucket = new Date(node.created_at).toISOString().slice(0, 13);
    return `unknown-${bucket}`;
}

export function relationSignals(a: GraphNodeData, b: GraphNodeData): string[] {
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

export function describeNode(node: GraphNodeData): string {
    const metadata = node.metadata ?? {};

    if (node.node_type === "MemoryChunk") {
        const app = safeString(metadata.app_name) || "Unknown app";
        const domain = nodeDomain(node);
        const memoryType = memoryTypeForNode(node);
        return domain ? `${app} • ${memoryType} • ${domain}` : `${app} • ${memoryType}`;
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

export function clusterKeyForNode(node: GraphNodeData, lens: ClusterLens): string {
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

export function typeCountBadge(counts: Record<GraphNodeType, number>): string {
    return NODE_TYPE_ORDER.map((type) => `${NODE_TYPE_META[type].short}:${counts[type] ?? 0}`).join(" • ");
}

export function buildAdjacency(
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

export function buildEdgePairMap(edges: GraphEdgeData[]): EdgePairIndex {
    const byPair: EdgePairIndex = {};
    for (const edge of edges) {
        byPair[`${edge.source}|${edge.target}`] = edge;
        byPair[`${edge.target}|${edge.source}`] = edge;
    }
    return byPair;
}

export function shortestPath(startId: string, endId: string, adjacency: Map<string, Set<string>>): string[] {
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

function edgeTypeWeight(edgeType: string): number {
    switch (edgeType) {
        case "REFERENCE_FOR_TASK":
            return 1.15;
        case "OCCURRED_AT":
            return 0.95;
        case "PART_OF_SESSION":
            return 0.75;
        default:
            return 0.7;
    }
}

function nodeTypeBias(nodeType: GraphNodeType): number {
    switch (nodeType) {
        case "Task":
            return 1.1;
        case "Entity":
            return 1.05;
        case "MemoryChunk":
            return 1;
        case "Url":
            return 0.88;
        default:
            return 1;
    }
}

function clamp01(value: number): number {
    return Math.max(0, Math.min(1, value));
}

function normalizeValues(values: number[]): number[] {
    if (values.length === 0) {
        return [];
    }
    const min = Math.min(...values);
    const max = Math.max(...values);
    const span = max - min;
    if (span < 1e-6) {
        return values.map(() => 0.5);
    }
    return values.map((value) => (value - min) / span);
}

export function scoreNodeImportance(
    nodes: TypedGraphNode[],
    edges: GraphEdgeData[],
    adjacency: Map<string, Set<string>>
): Map<string, NodeInsight> {
    const weightedDegree = new Map<string, number>();
    const rawDegree = new Map<string, number>();

    for (const node of nodes) {
        weightedDegree.set(node.id, 0);
        rawDegree.set(node.id, adjacency.get(node.id)?.size ?? 0);
    }

    for (const edge of edges) {
        const weight = edgeTypeWeight(edge.edge_type);
        weightedDegree.set(edge.source, (weightedDegree.get(edge.source) ?? 0) + weight);
        weightedDegree.set(edge.target, (weightedDegree.get(edge.target) ?? 0) + weight);
    }

    const created = nodes.map((node) => node.created_at);
    const recencyScores = normalizeValues(created);
    const weightedValues = nodes.map((node) => weightedDegree.get(node.id) ?? 0);
    const weightedNorm = normalizeValues(weightedValues);
    const degreeValues = nodes.map((node) => rawDegree.get(node.id) ?? 0);
    const degreeNorm = normalizeValues(degreeValues);

    const out = new Map<string, NodeInsight>();

    nodes.forEach((node, index) => {
        const typeBias = nodeTypeBias(node.node_type);
        const importance = clamp01(
            weightedNorm[index] * 0.48 +
                recencyScores[index] * 0.22 +
                degreeNorm[index] * 0.2 +
                (typeBias - 0.8) * 0.18
        );

        out.set(node.id, {
            id: node.id,
            importance,
            recency: recencyScores[index],
            weightedDegree: weightedDegree.get(node.id) ?? 0,
            bridgeStrength: 0,
            labelPriority: clamp01(importance * 0.78 + recencyScores[index] * 0.22),
            isBridge: false,
        });
    });

    return out;
}

export function scoreBridgeStrength(
    nodes: TypedGraphNode[],
    adjacency: Map<string, Set<string>>,
    clusterKeyByNode: Map<string, string>
): Map<string, number> {
    const out = new Map<string, number>();

    for (const node of nodes) {
        const neighbors = [...(adjacency.get(node.id) ?? new Set<string>())];
        if (neighbors.length === 0) {
            out.set(node.id, 0);
            continue;
        }

        const selfCluster = clusterKeyByNode.get(node.id);
        const uniqueClusters = new Set<string>();
        let interConnections = 0;

        for (const neighborId of neighbors) {
            const neighborCluster = clusterKeyByNode.get(neighborId);
            if (neighborCluster) {
                uniqueClusters.add(neighborCluster);
            }
            if (neighborCluster && selfCluster && neighborCluster !== selfCluster) {
                interConnections += 1;
            }
        }

        const uniqueRatio = (uniqueClusters.size - (selfCluster ? 1 : 0)) / Math.max(neighbors.length, 1);
        const interRatio = interConnections / Math.max(neighbors.length, 1);
        const degreeBoost = Math.min(neighbors.length / 10, 1) * 0.12;
        out.set(node.id, clamp01(uniqueRatio * 0.55 + interRatio * 0.45 + degreeBoost));
    }

    return out;
}

export function scoreEdgeProminence(
    edge: GraphEdgeData,
    nodeInsights: Map<string, NodeInsight>,
    bridgeScores: Map<string, number>,
    interaction: { selectedNodeId?: string | null; hoveredNodeId?: string | null; highlightedIds?: Set<string> }
): number {
    const source = nodeInsights.get(edge.source);
    const target = nodeInsights.get(edge.target);
    const sourceImportance = source?.importance ?? 0.2;
    const targetImportance = target?.importance ?? 0.2;
    const sourceBridge = bridgeScores.get(edge.source) ?? 0;
    const targetBridge = bridgeScores.get(edge.target) ?? 0;

    const edgeStrength = edgeTypeWeight(edge.edge_type) / 1.2;
    let prominence = 0.05 + edgeStrength * 0.18 + (sourceImportance + targetImportance) * 0.3 + (sourceBridge + targetBridge) * 0.25;

    const selected = interaction.selectedNodeId;
    const hovered = interaction.hoveredNodeId;

    if (selected && (edge.source === selected || edge.target === selected)) {
        prominence += 0.32;
    }

    if (hovered && (edge.source === hovered || edge.target === hovered)) {
        prominence += 0.26;
    }

    if (interaction.highlightedIds && interaction.highlightedIds.has(edge.source) && interaction.highlightedIds.has(edge.target)) {
        prominence += 0.18;
    }

    return clamp01(prominence);
}

function edgeReasonFromNodes(edge: GraphEdgeData, source: TypedGraphNode, target: TypedGraphNode): string {
    if (edge.edge_type === "PART_OF_SESSION") {
        return "shared session";
    }
    if (edge.edge_type === "REFERENCE_FOR_TASK") {
        return "shared task";
    }

    const sourceType = memoryTypeForNode(source);
    const targetType = memoryTypeForNode(target);
    if (sourceType && targetType && sourceType === targetType) {
        return "shared memory type";
    }

    if (Math.abs(source.created_at - target.created_at) <= 5 * 60 * 1000) {
        return "shared transition";
    }

    return "shared transition";
}

export function labelClusterRelationship(reasonCounts: Map<string, number>): string[] {
    return [...reasonCounts.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 2)
        .map(([reason]) => reason);
}

function assignClusterRole(
    rank: number,
    cluster: { strengthScore: number; bridgeScore: number; nodeCount: number }
): "dominant" | "secondary" | "bridge" | "peripheral" {
    if (rank === 0) {
        return "dominant";
    }

    if (cluster.bridgeScore > 0.6 && cluster.nodeCount >= 3) {
        return "bridge";
    }

    if (rank <= 2 || cluster.strengthScore >= 0.52) {
        return "secondary";
    }

    return "peripheral";
}

function keywordBucket(memory: TypedGraphNode): string {
    const app = safeString(memory.metadata?.app_name);
    if (app) {
        return app;
    }
    const domain = nodeDomain(memory);
    if (domain) {
        return domain;
    }
    return memoryTypeForNode(memory);
}

function buildSparklineFromTimestamps(timestamps: number[], size = 16): number[] {
    if (timestamps.length === 0) {
        return new Array(size).fill(0);
    }

    const min = Math.min(...timestamps);
    const max = Math.max(...timestamps);
    const span = Math.max(max - min, 1);
    const buckets = new Array(size).fill(0);

    for (const ts of timestamps) {
        const idx = Math.min(size - 1, Math.floor(((ts - min) / span) * size));
        buckets[idx] += 1;
    }

    const maxBucket = Math.max(...buckets, 1);
    return buckets.map((value) => value / maxBucket);
}

export function rankRepresentativeMemories(
    memoryIds: string[],
    nodeMap: Map<string, TypedGraphNode>,
    nodeInsights: Map<string, NodeInsight>,
    limit = 5
): string[] {
    const candidates = memoryIds
        .map((id) => nodeMap.get(id))
        .filter((node): node is TypedGraphNode => Boolean(node));

    const scored = candidates
        .map((node) => {
            const insight = nodeInsights.get(node.id);
            const labelInfo = new Set(
                node.label
                    .toLowerCase()
                    .split(/\W+/)
                    .filter((token) => token.length > 2)
            ).size;
            const richness = clamp01(labelInfo / 18);
            const score = (insight?.importance ?? 0.2) * 0.55 + (insight?.recency ?? 0.2) * 0.25 + richness * 0.2;
            return { node, score };
        })
        .sort((a, b) => b.score - a.score);

    const selected: string[] = [];
    const diversityKeys = new Set<string>();

    for (const item of scored) {
        const key = `${safeString(item.node.metadata?.app_name)}|${memoryTypeForNode(item.node)}|${nodeDomain(item.node)}`;
        if (diversityKeys.has(key) && selected.length >= Math.ceil(limit / 2)) {
            continue;
        }

        diversityKeys.add(key);
        selected.push(item.node.id);

        if (selected.length >= limit) {
            break;
        }
    }

    if (selected.length < limit) {
        for (const item of scored) {
            if (!selected.includes(item.node.id)) {
                selected.push(item.node.id);
            }
            if (selected.length >= limit) {
                break;
            }
        }
    }

    return selected;
}

function buildClusterSummary(cluster: {
    memoryCount: number;
    typeCounts: Record<GraphNodeType, number>;
    topTag: string;
}): string {
    const dominantType = NODE_TYPE_ORDER
        .map((type) => ({ type, count: cluster.typeCounts[type] ?? 0 }))
        .sort((a, b) => b.count - a.count)[0];

    const dominantLabel = dominantType.count > 0 ? NODE_TYPE_META[dominantType.type].label.toLowerCase() : "activity";
    return `${cluster.memoryCount} memories, mostly ${cluster.topTag || dominantLabel}`;
}

export function buildClusterInsights(
    nodes: TypedGraphNode[],
    edges: GraphEdgeData[],
    lens: ClusterLens,
    nodeInsights: Map<string, NodeInsight>
): {
    clusters: ClusterInsight[];
    clusterKeyByNode: Map<string, string>;
    connections: ClusterConnection[];
} {
    const nodeMap = new Map(nodes.map((node) => [node.id, node]));
    const clusterMap = new Map<
        string,
        {
            key: string;
            label: string;
            nodeIds: string[];
            memoryIds: string[];
            typeCounts: Record<GraphNodeType, number>;
            internalEdges: number;
            crossEdges: number;
            topTagCounts: Map<string, number>;
            timestamps: number[];
        }
    >();
    const clusterKeyByNode = new Map<string, string>();

    for (const node of nodes) {
        const key = clusterKeyForNode(node, lens) || "unassigned";
        clusterKeyByNode.set(node.id, key);
        if (!clusterMap.has(key)) {
            clusterMap.set(key, {
                key,
                label: key,
                nodeIds: [],
                memoryIds: [],
                typeCounts: {
                    MemoryChunk: 0,
                    Entity: 0,
                    Task: 0,
                    Url: 0,
                },
                internalEdges: 0,
                crossEdges: 0,
                topTagCounts: new Map<string, number>(),
                timestamps: [],
            });
        }

        const cluster = clusterMap.get(key);
        if (!cluster) {
            continue;
        }

        cluster.nodeIds.push(node.id);
        cluster.typeCounts[node.node_type] += 1;
        if (node.node_type === "MemoryChunk") {
            cluster.memoryIds.push(node.id);
            cluster.timestamps.push(node.created_at);
            const tag = keywordBucket(node);
            cluster.topTagCounts.set(tag, (cluster.topTagCounts.get(tag) ?? 0) + 1);
        }
    }

    const pairReasonCounts = new Map<string, Map<string, number>>();
    const pairCount = new Map<string, number>();

    for (const edge of edges) {
        const source = nodeMap.get(edge.source);
        const target = nodeMap.get(edge.target);
        if (!source || !target) {
            continue;
        }

        const leftKey = clusterKeyByNode.get(source.id) ?? "unassigned";
        const rightKey = clusterKeyByNode.get(target.id) ?? "unassigned";

        if (leftKey === rightKey) {
            const cluster = clusterMap.get(leftKey);
            if (cluster) {
                cluster.internalEdges += 1;
            }
            continue;
        }

        const left = clusterMap.get(leftKey);
        const right = clusterMap.get(rightKey);
        if (left) {
            left.crossEdges += 1;
        }
        if (right) {
            right.crossEdges += 1;
        }

        const pair = [leftKey, rightKey].sort().join("|");
        pairCount.set(pair, (pairCount.get(pair) ?? 0) + 1);

        if (!pairReasonCounts.has(pair)) {
            pairReasonCounts.set(pair, new Map<string, number>());
        }

        const reason = edgeReasonFromNodes(edge, source, target);
        const reasonMap = pairReasonCounts.get(pair);
        if (reasonMap) {
            reasonMap.set(reason, (reasonMap.get(reason) ?? 0) + 1);
        }
    }

    const clustersRaw = [...clusterMap.values()].map((cluster) => {
        const nodeCount = cluster.nodeIds.length;
        const possibleInternal = Math.max((nodeCount * (nodeCount - 1)) / 2, 1);
        const internalDensity = clamp01(cluster.internalEdges / possibleInternal);
        const crossConnectivity = clamp01(cluster.crossEdges / Math.max(cluster.internalEdges + cluster.crossEdges, 1));

        const avgImportance =
            cluster.nodeIds.reduce((sum, id) => sum + (nodeInsights.get(id)?.importance ?? 0), 0) /
            Math.max(nodeCount, 1);

        const avgBridge =
            cluster.nodeIds.reduce((sum, id) => sum + (nodeInsights.get(id)?.bridgeStrength ?? 0), 0) /
            Math.max(nodeCount, 1);

        const strengthScore = clamp01(
            Math.log2(nodeCount + 1) / 4 + internalDensity * 0.32 + avgImportance * 0.28 + (1 - crossConnectivity) * 0.1
        );

        const sortedTags = [...cluster.topTagCounts.entries()].sort((a, b) => b[1] - a[1]);
        const topTags = sortedTags.slice(0, 3).map(([tag]) => tag);
        const topTag = topTags[0] ?? "mixed";

        const subthemes = sortedTags.slice(0, 4).map(([key, count]) => ({ key, count }));

        const summary = buildClusterSummary({
            memoryCount: cluster.memoryIds.length,
            typeCounts: cluster.typeCounts,
            topTag,
        });

        return {
            ...cluster,
            nodeCount,
            internalDensity,
            crossConnectivity,
            bridgeScore: avgBridge,
            strengthScore,
            topTags,
            summary,
            subthemes,
            sparkline: buildSparklineFromTimestamps(cluster.timestamps),
        };
    });

    clustersRaw.sort((a, b) => b.strengthScore - a.strengthScore);

    const clusters = clustersRaw.map((cluster, index) => {
        const role = assignClusterRole(index, {
            strengthScore: cluster.strengthScore,
            bridgeScore: cluster.bridgeScore,
            nodeCount: cluster.nodeCount,
        });

        const exemplarMemoryIds = rankRepresentativeMemories(
            cluster.memoryIds,
            nodeMap,
            nodeInsights,
            5
        );

        return {
            key: cluster.key,
            label: cluster.label,
            nodeIds: cluster.nodeIds,
            memoryIds: cluster.memoryIds,
            typeCounts: cluster.typeCounts,
            role,
            strengthScore: cluster.strengthScore,
            bridgeScore: cluster.bridgeScore,
            internalDensity: cluster.internalDensity,
            crossConnectivity: cluster.crossConnectivity,
            topTags: cluster.topTags,
            summary: cluster.summary,
            exemplarMemoryIds,
            subthemes: cluster.subthemes,
            sparkline: cluster.sparkline,
        } satisfies ClusterInsight;
    });

    const maxPairCount = Math.max(...pairCount.values(), 1);
    const connections: ClusterConnection[] = [...pairCount.entries()].map(([pair, count]) => {
        const [leftKey, rightKey] = pair.split("|");
        const reasons = labelClusterRelationship(pairReasonCounts.get(pair) ?? new Map<string, number>());

        return {
            pair,
            leftKey,
            rightKey,
            count,
            reasons,
            primaryReason: reasons[0] ?? "shared transition",
            strength: clamp01(count / maxPairCount),
        };
    });

    connections.sort((a, b) => b.count - a.count);

    return { clusters, clusterKeyByNode, connections };
}

function lexicalSimilarity(a: string, b: string): number {
    const left = a
        .toLowerCase()
        .replace(/[^a-z0-9\s]/g, " ")
        .split(/\s+/)
        .filter((token) => token.length > 2);
    const right = b
        .toLowerCase()
        .replace(/[^a-z0-9\s]/g, " ")
        .split(/\s+/)
        .filter((token) => token.length > 2);

    if (left.length === 0 || right.length === 0) {
        return 0;
    }

    const leftSet = new Set(left);
    const rightSet = new Set(right);
    let overlap = 0;
    for (const token of leftSet) {
        if (rightSet.has(token)) {
            overlap += 1;
        }
    }

    return overlap / Math.max(leftSet.size, rightSet.size, 1);
}

function continuityScore(current: TypedGraphNode, previous: TypedGraphNode): number {
    const app = safeString(current.metadata?.app_name) === safeString(previous.metadata?.app_name) ? 0.35 : 0;
    const memoryType = memoryTypeForNode(current) === memoryTypeForNode(previous) ? 0.25 : 0;
    const domain = nodeDomain(current) && nodeDomain(current) === nodeDomain(previous) ? 0.2 : 0;
    const lexical = lexicalSimilarity(current.label, previous.label) * 0.2;
    return app + memoryType + domain + lexical;
}

function segmentLabel(memories: TypedGraphNode[], seenPrimaryApps: Set<string>): TimelineInsight["segments"][number]["label"] {
    const appCounts = new Map<string, number>();
    const memoryTypeCounts = new Map<string, number>();
    let appSwitches = 0;

    memories.forEach((memory, index) => {
        const app = safeString(memory.metadata?.app_name) || "Unknown";
        appCounts.set(app, (appCounts.get(app) ?? 0) + 1);

        const memoryType = memoryTypeForNode(memory);
        memoryTypeCounts.set(memoryType, (memoryTypeCounts.get(memoryType) ?? 0) + 1);

        if (index > 0) {
            const prevApp = safeString(memories[index - 1]?.metadata?.app_name);
            if (prevApp && prevApp !== app) {
                appSwitches += 1;
            }
        }
    });

    const topType = [...memoryTypeCounts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "general";
    const topApp = [...appCounts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "Unknown";

    if (seenPrimaryApps.has(topApp) && memories.length >= 3) {
        return "return";
    }

    seenPrimaryApps.add(topApp);

    if (topType === "development" || topType === "task") {
        return appSwitches <= 1 ? "implementation" : "refinement";
    }
    if (topType === "documents" || topType === "meeting") {
        return "validation";
    }
    if (topType === "web") {
        return "exploration";
    }
    if (appSwitches >= Math.max(2, Math.floor(memories.length / 2))) {
        return "drift";
    }

    return "refinement";
}

export function deriveTimelineSegments(
    memoryNodesDesc: TypedGraphNode[],
    adjacency: Map<string, Set<string>>
): TimelineInsight {
    const memoryNodes = [...memoryNodesDesc]
        .filter((node) => node.node_type === "MemoryChunk")
        .sort((a, b) => a.created_at - b.created_at);

    if (memoryNodes.length === 0) {
        return {
            segments: [],
            pivots: [],
            groupedMemories: [],
            densityByBucket: [],
        };
    }

    const groupedMemories: TimelineInsight["groupedMemories"] = [];
    let currentGroup = [memoryNodes[0]];

    for (let i = 1; i < memoryNodes.length; i++) {
        const current = memoryNodes[i];
        const prev = memoryNodes[i - 1];
        const closeInTime = Math.abs(current.created_at - prev.created_at) <= 90_000;
        const similar =
            safeString(current.metadata?.app_name) === safeString(prev.metadata?.app_name) &&
            lexicalSimilarity(current.label, prev.label) > 0.72;

        if (closeInTime && similar) {
            currentGroup.push(current);
            continue;
        }

        groupedMemories.push({
            id: `group-${groupedMemories.length}`,
            memoryIds: currentGroup.map((item) => item.id),
            representativeId: currentGroup[0].id,
            isCollapsedByDefault: currentGroup.length >= 3,
            similarityReason: currentGroup.length >= 3 ? `${currentGroup.length} similar iterations` : "",
        });
        currentGroup = [current];
    }

    groupedMemories.push({
        id: `group-${groupedMemories.length}`,
        memoryIds: currentGroup.map((item) => item.id),
        representativeId: currentGroup[0].id,
        isCollapsedByDefault: currentGroup.length >= 3,
        similarityReason: currentGroup.length >= 3 ? `${currentGroup.length} similar iterations` : "",
    });

    const pivotList: TimelineInsight["pivots"] = [];
    const segmentBuckets: TypedGraphNode[][] = [];
    let currentSegment: TypedGraphNode[] = [memoryNodes[0]];

    for (let i = 1; i < memoryNodes.length; i++) {
        const current = memoryNodes[i];
        const prev = memoryNodes[i - 1];
        const gapMs = current.created_at - prev.created_at;
        const continuity = continuityScore(current, prev);

        let pivotReason = "";
        if (gapMs >= 18 * 60 * 1000) {
            pivotReason = "quiet gap";
        } else if (safeString(current.metadata?.app_name) !== safeString(prev.metadata?.app_name)) {
            pivotReason = "app switch";
        } else if (continuity < 0.25) {
            pivotReason = "topic shift";
        } else if (!(adjacency.get(prev.id)?.has(current.id) ?? false) && continuity < 0.38) {
            pivotReason = "task pivot";
        }

        if (pivotReason) {
            pivotList.push({ memoryId: current.id, reason: pivotReason });
            segmentBuckets.push(currentSegment);
            currentSegment = [current];
            continue;
        }

        currentSegment.push(current);
    }
    segmentBuckets.push(currentSegment);

    const seenPrimaryApps = new Set<string>();
    const segments: TimelineInsight["segments"] = segmentBuckets.map((memories, index) => {
        const intensities = memories.map((memory, i) => {
            if (i === 0) {
                return 0.4;
            }
            const prev = memories[i - 1];
            return clamp01(continuityScore(memory, prev));
        });

        const pivotCount = pivotList.filter((pivot) => memories.some((memory) => memory.id === pivot.memoryId)).length;
        const intensity = intensities.reduce((sum, value) => sum + value, 0) / Math.max(intensities.length, 1);
        const label = segmentLabel(memories, seenPrimaryApps);
        const confidence = clamp01(0.45 + intensity * 0.4 + (memories.length >= 4 ? 0.12 : 0));

        return {
            id: `segment-${index}`,
            label,
            memoryIds: memories.map((memory) => memory.id),
            startTs: memories[0]?.created_at ?? 0,
            endTs: memories[memories.length - 1]?.created_at ?? 0,
            intensity,
            confidence,
            pivotCount,
        };
    });

    const densityByBucket = buildSparklineFromTimestamps(memoryNodes.map((memory) => memory.created_at), 24);

    return {
        segments,
        pivots: pivotList,
        groupedMemories,
        densityByBucket,
    };
}

function signalFromEdgeType(edgeType: string): string {
    if (edgeType === "PART_OF_SESSION") {
        return "session continuity";
    }
    if (edgeType === "REFERENCE_FOR_TASK") {
        return "task continuity";
    }
    if (edgeType === "OCCURRED_AT") {
        return "reference link";
    }
    return "semantic follow-up";
}

function hopQualityFromSignals(reason: string, appContinuity: boolean, sessionContinuity: boolean): "direct" | "weak" | "cross-context" | "semantic" {
    if (reason === "task continuity" || reason === "reference link") {
        return "direct";
    }
    if (!appContinuity && !sessionContinuity) {
        return "cross-context";
    }
    if (reason === "semantic follow-up") {
        return "semantic";
    }
    return "weak";
}

export function deriveJourneyHopSemantics(
    pathIds: string[],
    nodeMap: Map<string, TypedGraphNode>,
    edgeByPair: EdgePairIndex,
    bridgeScores: Map<string, number>
): JourneySemantics {
    if (pathIds.length === 0) {
        return {
            hops: [],
            summary: "No route available in current graph slice.",
            overallQuality: "weak",
        };
    }

    const hops = pathIds.map((nodeId, index) => {
        const node = nodeMap.get(nodeId);
        const prevNode = index > 0 ? nodeMap.get(pathIds[index - 1]) : null;
        const nextNode = index < pathIds.length - 1 ? nodeMap.get(pathIds[index + 1]) : null;

        let role: "anchor" | "bridge" | "pivot" | "endpoint" = "anchor";
        if (index === 0) {
            role = "anchor";
        } else if (index === pathIds.length - 1) {
            role = "endpoint";
        } else {
            const bridgeStrength = bridgeScores.get(nodeId) ?? 0;
            const prevApp = safeString(prevNode?.metadata?.app_name);
            const nextApp = safeString(nextNode?.metadata?.app_name);
            if (bridgeStrength >= 0.55 || node?.node_type === "Entity") {
                role = "bridge";
            } else if (prevApp && nextApp && prevApp !== nextApp) {
                role = "pivot";
            }
        }

        const edge = nextNode ? edgeByPair[`${nodeId}|${nextNode.id}`] : prevNode ? edgeByPair[`${prevNode.id}|${nodeId}`] : undefined;
        const reason = signalFromEdgeType(edge?.edge_type ?? "");

        const appContinuity =
            Boolean(node) &&
            Boolean(nextNode) &&
            safeString(node?.metadata?.app_name) === safeString(nextNode?.metadata?.app_name);
        const sessionContinuity =
            Boolean(node) &&
            Boolean(nextNode) &&
            sessionIdForMemory(node as GraphNodeData) === sessionIdForMemory(nextNode as GraphNodeData);

        const quality = hopQualityFromSignals(reason, appContinuity, sessionContinuity);

        return {
            nodeId,
            index,
            role,
            reason,
            quality,
            isEssential: role === "anchor" || role === "endpoint" || role === "bridge" || role === "pivot",
            phaseGroup: 0,
        };
    });

    let phase = 0;
    for (let i = 0; i < hops.length; i++) {
        if (i > 0 && (hops[i].quality !== hops[i - 1].quality || hops[i].reason !== hops[i - 1].reason)) {
            phase += 1;
        }
        hops[i].phaseGroup = phase;
    }

    const reasonCounts = new Map<string, number>();
    hops.forEach((hop) => reasonCounts.set(hop.reason, (reasonCounts.get(hop.reason) ?? 0) + 1));

    const dominantReasons = [...reasonCounts.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 2)
        .map(([reason]) => reason);

    const overallQuality =
        hops.some((hop) => hop.quality === "cross-context")
            ? "cross-context"
            : hops.some((hop) => hop.quality === "weak")
                ? "weak"
                : hops.some((hop) => hop.quality === "semantic")
                    ? "semantic"
                    : "direct";

    return {
        hops,
        summary: `Connected through ${dominantReasons.join(" and ") || "graph continuity"}.`,
        overallQuality,
    };
}

function focusSemanticScore(center: TypedGraphNode, candidate: TypedGraphNode): number {
    const app = safeString(center.metadata?.app_name) === safeString(candidate.metadata?.app_name) ? 0.3 : 0;
    const memoryType = memoryTypeForNode(center) === memoryTypeForNode(candidate) ? 0.25 : 0;
    const domain = nodeDomain(center) && nodeDomain(center) === nodeDomain(candidate) ? 0.18 : 0;
    const session = sessionIdForMemory(center) === sessionIdForMemory(candidate) ? 0.2 : 0;
    const lexical = lexicalSimilarity(center.label, candidate.label) * 0.07;
    return app + memoryType + domain + session + lexical;
}

function edgeReasonLabel(edgeType?: string | null): string {
    if (!edgeType) {
        return "";
    }
    if (edgeType === "PART_OF_SESSION") {
        return "same session edge";
    }
    if (edgeType === "REFERENCE_FOR_TASK") {
        return "task reference edge";
    }
    if (edgeType === "OCCURRED_AT") {
        return "url/context edge";
    }
    return "graph edge";
}

function focusConnectionReasons(
    center: TypedGraphNode,
    candidate: TypedGraphNode,
    edge?: GraphEdgeData,
    viaNodeId?: string | null
): string[] {
    const reasons: string[] = [];
    const edgeReason = edgeReasonLabel(edge?.edge_type);
    if (edgeReason) {
        reasons.push(edgeReason);
    }

    const centerApp = safeString(center.metadata?.app_name);
    const candidateApp = safeString(candidate.metadata?.app_name);
    if (centerApp && candidateApp && centerApp === candidateApp) {
        reasons.push(`same app (${centerApp})`);
    }

    const centerSession = sessionIdForMemory(center);
    const candidateSession = sessionIdForMemory(candidate);
    if (centerSession && candidateSession && centerSession === candidateSession) {
        reasons.push("same session id");
    }

    const centerDomain = nodeDomain(center);
    const candidateDomain = nodeDomain(candidate);
    if (centerDomain && candidateDomain && centerDomain === candidateDomain) {
        reasons.push(`same domain (${centerDomain})`);
    }

    const minutesApart = Math.round(Math.abs(center.created_at - candidate.created_at) / 60_000);
    if (minutesApart <= 60) {
        reasons.push(`${Math.max(minutesApart, 1)}m apart`);
    }

    if (viaNodeId) {
        reasons.push("connected via direct neighbor");
    }

    if (reasons.length === 0) {
        reasons.push("connected in graph");
    }

    return reasons.slice(0, 3);
}

function focusScoreByMode(
    center: TypedGraphNode,
    candidate: TypedGraphNode,
    mode: FocusMode,
    nodeInsights: Map<string, NodeInsight>,
    edge?: GraphEdgeData,
    viaNodeId?: string | null
): { score: number; reasons: string[] } {
    const baseImportance = nodeInsights.get(candidate.id)?.importance ?? 0.15;
    const reasons = focusConnectionReasons(center, candidate, edge, viaNodeId);

    if (mode === "structural") {
        const edgeWeight = edge ? edgeTypeWeight(edge.edge_type) / 1.2 : 0.2;
        return {
            score: baseImportance * 0.7 + edgeWeight * 0.3,
            reasons: reasons.length > 0 ? reasons : ["structural connection"],
        };
    }

    if (mode === "semantic") {
        const semantic = focusSemanticScore(center, candidate);
        return {
            score: baseImportance * 0.35 + semantic * 0.65,
            reasons: reasons.length > 0 ? reasons : ["semantic similarity"],
        };
    }

    const centerTs = center.created_at;
    const candidateTs = candidate.created_at;
    const temporalDirection = candidateTs >= centerTs ? 0.32 : 0.18;
    const edgeBonus = edge?.edge_type === "REFERENCE_FOR_TASK" ? 0.35 : edge?.edge_type === "PART_OF_SESSION" ? 0.2 : 0.15;

    const causalReasons = [...reasons];
    causalReasons.push(candidateTs >= centerTs ? "follow-up action" : "upstream context");

    return {
        score: baseImportance * 0.3 + temporalDirection + edgeBonus,
        reasons: causalReasons,
    };
}

export function buildFocusNeighborhood(
    centerId: string,
    nodeMap: Map<string, TypedGraphNode>,
    adjacency: Map<string, Set<string>>,
    edgeByPair: EdgePairIndex,
    mode: FocusMode,
    nodeInsights: Map<string, NodeInsight>,
    directLimit = 10,
    secondaryLimit = 14
): FocusNeighborhood {
    const center = nodeMap.get(centerId);
    if (!center) {
        return {
            centerId,
            direct: [],
            secondary: [],
            displayNodeIds: new Set([centerId]),
        };
    }

    const directCandidates = [...(adjacency.get(centerId) ?? new Set())]
        .map((nodeId) => nodeMap.get(nodeId))
        .filter((node): node is TypedGraphNode => Boolean(node));

    const direct = directCandidates
        .map((candidate) => {
            const edge = edgeByPair[`${centerId}|${candidate.id}`];
            const scored = focusScoreByMode(center, candidate, mode, nodeInsights, edge, null);
            return {
                nodeId: candidate.id,
                score: scored.score,
                degree: 1 as const,
                reasons: scored.reasons.slice(0, 2),
                edgeType: edge?.edge_type ?? null,
                viaNodeId: null,
            };
        })
        .sort((a, b) => b.score - a.score)
        .slice(0, directLimit);

    const directIdSet = new Set(direct.map((item) => item.nodeId));
    const secondaryViaMap = new Map<string, string>();

    direct.forEach((item) => {
        const neighbors = adjacency.get(item.nodeId);
        if (!neighbors) {
            return;
        }
        for (const candidateId of neighbors) {
            if (candidateId === centerId || directIdSet.has(candidateId)) {
                continue;
            }
            if (!secondaryViaMap.has(candidateId)) {
                secondaryViaMap.set(candidateId, item.nodeId);
            }
        }
    });

    const secondary = [...secondaryViaMap.keys()]
        .map((nodeId) => nodeMap.get(nodeId))
        .filter((node): node is TypedGraphNode => Boolean(node))
        .map((candidate) => {
            const viaNodeId = secondaryViaMap.get(candidate.id) ?? null;
            const viaEdge = viaNodeId ? edgeByPair[`${viaNodeId}|${candidate.id}`] : undefined;
            const scored = focusScoreByMode(center, candidate, mode, nodeInsights, viaEdge, viaNodeId);
            return {
                nodeId: candidate.id,
                score: scored.score,
                degree: 2 as const,
                reasons: scored.reasons.slice(0, 2),
                edgeType: viaEdge?.edge_type ?? null,
                viaNodeId,
            };
        })
        .sort((a, b) => b.score - a.score)
        .slice(0, secondaryLimit);

    return {
        centerId,
        direct,
        secondary,
        displayNodeIds: new Set([centerId, ...direct.map((item) => item.nodeId), ...secondary.map((item) => item.nodeId)]),
    };
}

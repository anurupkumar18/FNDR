import type { GraphView } from "./types";

export interface FilterOptions {
    nodeTypes: string[];
    projects: string[];
    topics: string[];
    edgeKinds: string[];
    confidenceRange: [number, number];
}

function metaString(raw: { metadata: unknown }, key: string): string | null {
    const md = raw.metadata;
    if (md && typeof md === "object" && key in md) {
        const v = (md as Record<string, unknown>)[key];
        if (typeof v === "string" && v.trim()) return v;
    }
    return null;
}

export function deriveFilterOptions(view: GraphView): FilterOptions {
    const nodeTypes = new Set<string>();
    const projects = new Set<string>();
    const topics = new Set<string>();
    for (const n of view.nodes) {
        nodeTypes.add(n.nodeType);
        const p = metaString(n.raw, "project");
        if (p) projects.add(p);
        const t = metaString(n.raw, "topic");
        if (t) topics.add(t);
    }

    const edgeKinds = new Set<string>();
    let minConf = 1;
    let maxConf = 0;
    let sawEdges = false;
    for (const e of view.edges) {
        edgeKinds.add(e.kind);
        sawEdges = true;
        if (e.confidence < minConf) minConf = e.confidence;
        if (e.confidence > maxConf) maxConf = e.confidence;
    }
    const confidenceRange: [number, number] = sawEdges ? [minConf, maxConf] : [0, 1];

    return {
        nodeTypes: Array.from(nodeTypes),
        projects: Array.from(projects),
        topics: Array.from(topics),
        edgeKinds: Array.from(edgeKinds),
        confidenceRange,
    };
}

import type { InsightGraphEdge, InsightGraphNode } from "@/shared/ipc/tauri";
import type { EdgeKind, RelationshipReason } from "./types";

const STRUCTURAL = new Set([
    "PartOf",
    "Contains",
    "DependsOn",
    "Imports",
    "Extends",
    "Implements",
    "UsedIn",
    "CreatedBy",
    "Refines",
    "Supports",
    "FixedBy",
    "BrokeBy",
    "Prevents",
]);
const SEMANTIC = new Set(["SimilarTo"]);
const TEMPORAL = new Set(["PrecededBy", "FollowedBy", "Causes", "TriggeredBy"]);
const CONFLICT = new Set(["Contradicts", "Supersedes", "Resolves", "Questions"]);

export function edgeKindFor(edgeType: string): EdgeKind {
    if (STRUCTURAL.has(edgeType)) return "structural";
    if (SEMANTIC.has(edgeType)) return "semantic";
    if (TEMPORAL.has(edgeType)) return "temporal";
    if (CONFLICT.has(edgeType)) return "conflict";
    return "reference"; // includes MentionedIn / AppliesTo plus any future unknowns
}

const STRUCTURAL_VERB: Record<string, string> = {
    PartOf: "part of",
    Contains: "contains",
    DependsOn: "depends on",
    Imports: "imports",
    Extends: "extends",
    Implements: "implements",
    UsedIn: "used in",
    CreatedBy: "created by",
    Refines: "refines",
    Supports: "supports",
    FixedBy: "fixed by",
    BrokeBy: "broke by",
    Prevents: "prevents",
};

const CONFLICT_VERB: Record<string, string> = {
    Contradicts: "contradicts",
    Supersedes: "supersedes",
    Resolves: "resolves",
    Questions: "questions",
};

const REFERENCE_VERB: Record<string, string> = {
    MentionedIn: "mentioned in",
    AppliesTo: "applies to",
};

function humanize(edgeType: string): string {
    // "MentionsObliquely" -> "mentions obliquely"
    return edgeType.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
}

function metadataField(node: InsightGraphNode, key: string): string | null {
    const md = node.metadata;
    if (md && typeof md === "object" && key in md) {
        const v = (md as Record<string, unknown>)[key];
        return typeof v === "string" && v.trim() ? v : null;
    }
    return null;
}

export function explainEdge(
    edge: InsightGraphEdge,
    source: InsightGraphNode,
    target: InsightGraphNode,
): RelationshipReason[] {
    const reasons: RelationshipReason[] = [];
    const kind = edgeKindFor(edge.edge_type);
    let primary: RelationshipReason;

    switch (kind) {
        case "structural": {
            const verb = STRUCTURAL_VERB[edge.edge_type] ?? humanize(edge.edge_type);
            primary = { text: `${verb} ${target.label}`, tone: "neutral" };
            break;
        }
        case "semantic": {
            const conf = edge.confidence.toFixed(2);
            primary = { text: `semantic similarity · confidence ${conf}`, tone: "amber" };
            break;
        }
        case "temporal": {
            const direction = edge.edge_type === "PrecededBy" ? "precedes" : "follows";
            const verb =
                edge.edge_type === "Causes"
                    ? "causes"
                    : edge.edge_type === "TriggeredBy"
                      ? "triggered by"
                      : direction;
            primary = { text: `temporal · ${verb} ${target.label}`, tone: "neutral" };
            break;
        }
        case "conflict": {
            const verb = CONFLICT_VERB[edge.edge_type] ?? humanize(edge.edge_type);
            primary = { text: `${verb} ${target.label}`, tone: "alarm" };
            break;
        }
        case "reference":
        default: {
            const verb = REFERENCE_VERB[edge.edge_type] ?? humanize(edge.edge_type);
            primary = { text: `${verb} ${target.label}`, tone: "neutral" };
            break;
        }
    }

    if (edge.confidence < 0.7 && kind !== "semantic") {
        primary = { ...primary, text: `${primary.text} · low confidence` };
    }
    reasons.push(primary);

    const sourceProject = metadataField(source, "project");
    const targetProject = metadataField(target, "project");
    if (sourceProject && sourceProject === targetProject) {
        reasons.push({ text: `shared project · ${sourceProject}`, tone: "neutral" });
    }

    const sourceTopic = metadataField(source, "topic");
    const targetTopic = metadataField(target, "topic");
    if (sourceTopic && sourceTopic === targetTopic) {
        reasons.push({ text: `shared topic · ${sourceTopic}`, tone: "neutral" });
    }

    return reasons;
}

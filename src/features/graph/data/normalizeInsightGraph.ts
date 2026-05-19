import type { InsightGraphSubgraph, InsightGraphNode, InsightGraphEdge } from "@/shared/ipc/tauri"
import type { GraphData, GraphNode, GraphEdge, GraphCommunity, Anchor3D } from "../types"
import { NodeType, EdgeType, FocusType } from "../types"
import { getDisplayLabel } from "../utils/displayTitle"

// 12 fixed spherical positions on a radius-150 sphere (mirrors src-tauri/src/graph/projection.rs:94-107)
// Communities are sorted by id then assigned to positions in order, so the same community always
// lands at the same spot across runs.
const ANCHOR_POSITIONS: ReadonlyArray<readonly [number, number]> = [
  [45, 0],
  [45, 60],
  [45, 120],
  [45, 180],
  [45, 240],
  [45, 300],
  [-45, 0],
  [-45, 60],
  [-45, 120],
  [-45, 180],
  [-45, 240],
  [-45, 300],
]
const ANCHOR_RADIUS = 150

function computeAnchor(index: number): Anchor3D {
  const [lat, lon] = ANCHOR_POSITIONS[index % ANCHOR_POSITIONS.length]
  const latRad = (lat * Math.PI) / 180
  const lonRad = (lon * Math.PI) / 180
  return {
    x: ANCHOR_RADIUS * Math.cos(latRad) * Math.cos(lonRad),
    y: ANCHOR_RADIUS * Math.sin(latRad),
    z: ANCHOR_RADIUS * Math.cos(latRad) * Math.sin(lonRad),
  }
}

function mapNodeType(raw: string | null | undefined): NodeType {
  const t = (raw ?? "").toLowerCase()
  if (t.includes("entity")) return NodeType.Entity
  if (t.includes("community") || t.includes("cluster")) return NodeType.Community
  if (t.includes("evidence")) return NodeType.Evidence
  if (t.includes("agent")) return NodeType.AgentContext
  return NodeType.Memory
}

function mapEdgeType(raw: string | null | undefined): EdgeType {
  const t = (raw ?? "").toLowerCase()
  if (t.includes("semantic")) return EdgeType.SemanticSimilarity
  if (t.includes("explicit") || t.includes("reference")) return EdgeType.ExplicitReference
  if (t.includes("temporal") || t.includes("time") || t.includes("adjacent")) return EdgeType.TemporalAdjacency
  if (t.includes("project")) return EdgeType.SameProject
  if (t.includes("session")) return EdgeType.SameSession
  if (t.includes("inferred") || t.includes("agent")) return EdgeType.AgentInferred
  if (t.includes("provenance") || t.includes("source")) return EdgeType.Provenance
  return EdgeType.SemanticSimilarity
}

function clamp01(value: number): number {
  if (Number.isNaN(value)) return 0
  return Math.max(0, Math.min(1, value))
}

function buildConnectionCounts(edges: InsightGraphEdge[], nodeIds: Set<string>): Map<string, number> {
  const counts = new Map<string, number>()
  for (const e of edges) {
    if (!nodeIds.has(e.source_id) || !nodeIds.has(e.target_id)) continue
    counts.set(e.source_id, (counts.get(e.source_id) ?? 0) + 1)
    counts.set(e.target_id, (counts.get(e.target_id) ?? 0) + 1)
  }
  return counts
}

export function normalizeInsightGraph(
  subgraph: InsightGraphSubgraph,
  louvain?: Record<string, number> | null
): GraphData {
  const nodeIdSet = new Set(subgraph.nodes.map((n) => n.id))
  const connectionCounts = buildConnectionCounts(subgraph.edges, nodeIdSet)
  const louvainMap: Record<string, number> = louvain ?? subgraph.louvain ?? {}

  // ---- Nodes ----
  const nodes: GraphNode[] = subgraph.nodes.map((raw: InsightGraphNode) => {
    const connections = connectionCounts.get(raw.id) ?? 0
    const confidence = clamp01(raw.confidence ?? 0.5)
    // Importance: confidence weighted by log of connectivity (mirrors graphDataBuilder.ts:40-44)
    const importance = clamp01(confidence * (Math.log2(connections + 1) / 4 + 0.25))
    const communityId =
      raw.id in louvainMap ? String(louvainMap[raw.id]) : undefined

    return {
      id: raw.id,
      node_type: mapNodeType(raw.node_type),
      title: getDisplayLabel(raw),
      community_id: communityId,
      timestamp_start: raw.created_at,
      timestamp_end: raw.updated_at,
      importance_score: importance,
      confidence_score: confidence,
      reuse_count: connections,
      source_ids: raw.source_memory_ids ?? [],
      metadata: (raw.metadata && typeof raw.metadata === "object")
        ? (raw.metadata as Record<string, unknown>)
        : undefined,
    }
  })

  // ---- Edges ----
  const edges: GraphEdge[] = subgraph.edges
    .filter((e) => nodeIdSet.has(e.source_id) && nodeIdSet.has(e.target_id))
    .map((e: InsightGraphEdge) => {
      const confidence = clamp01(e.confidence ?? 0.5)
      return {
        id: e.id,
        source: e.source_id,
        target: e.target_id,
        edge_type: mapEdgeType(e.edge_type),
        weight: confidence,
        confidence,
        metadata: (e.metadata && typeof e.metadata === "object")
          ? (e.metadata as Record<string, unknown>)
          : undefined,
      }
    })

  // ---- Communities (from louvain map) ----
  // Group node ids by community number, then sort communities by id for stable anchor assignment.
  const communityGroups = new Map<number, string[]>()
  for (const nodeId of Object.keys(louvainMap)) {
    if (!nodeIdSet.has(nodeId)) continue
    const cid = louvainMap[nodeId]
    const list = communityGroups.get(cid) ?? []
    list.push(nodeId)
    communityGroups.set(cid, list)
  }

  const sortedCommunityIds = Array.from(communityGroups.keys()).sort((a, b) => a - b)
  const communities: GraphCommunity[] = sortedCommunityIds.map((cid, index) => {
    const memberIds = communityGroups.get(cid) ?? []
    const memberImportance =
      memberIds
        .map((id) => nodes.find((n) => n.id === id)?.importance_score ?? 0)
        .reduce((a, b) => a + b, 0) / Math.max(memberIds.length, 1)

    const label =
      cid === 0 && subgraph.cluster_0_name
        ? subgraph.cluster_0_name
        : `Cluster ${cid}`

    return {
      id: String(cid),
      label,
      anchor: computeAnchor(index),
      node_count: memberIds.length,
      importance_score: clamp01(memberImportance),
    }
  })

  if (typeof console !== "undefined") {
    console.debug(
      `[normalizeInsightGraph] mapped ${nodes.length} nodes, ${edges.length} edges, ${communities.length} communities`
    )
  }

  return {
    nodes,
    edges,
    communities,
    active_focus: { focus_type: FocusType.Atlas, label: "Full Memory Atlas" },
  }
}

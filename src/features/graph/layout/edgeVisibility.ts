import type { GraphEdge, EdgeType } from "../types"
import { EDGE_CONFIG } from "../constants"

export interface VisibleEdge extends GraphEdge {
  isVisible: boolean
  displayOpacity: number
}

export function selectVisibleEdges(
  edges: GraphEdge[],
  selectedNodeIds: Set<string>,
  enabledEdgeTypes: Set<EdgeType>,
  maxVisible: number = EDGE_CONFIG.maxVisibleEdges
): VisibleEdge[] {
  // Filter by edge type
  const typeFiltered = edges.filter((e) => enabledEdgeTypes.has(e.edge_type))

  // If specific nodes are selected, prioritize their edges
  let prioritized: GraphEdge[] = []
  const selectedEdges: GraphEdge[] = []
  const unselectedEdges: GraphEdge[] = []

  for (const edge of typeFiltered) {
    if (selectedNodeIds.has(edge.source) || selectedNodeIds.has(edge.target)) {
      selectedEdges.push(edge)
    } else {
      unselectedEdges.push(edge)
    }
  }

  // Sort by weight (higher weight = stronger connection)
  selectedEdges.sort((a, b) => b.weight - a.weight)
  unselectedEdges.sort((a, b) => b.weight - a.weight)

  // Take top-K edges, prioritizing selected node connections
  prioritized = [
    ...selectedEdges.slice(0, Math.ceil(maxVisible * 0.5)), // 50% for selected node edges
    ...unselectedEdges.slice(0, Math.floor(maxVisible * 0.5)), // 50% for other edges
  ].slice(0, maxVisible)

  // Convert to VisibleEdge with opacity based on weight
  return prioritized.map((edge) => ({
    ...edge,
    isVisible: true,
    displayOpacity: EDGE_CONFIG.defaultOpacity * edge.weight,
  }))
}

export function getEdgeColor(edgeType: EdgeType): string {
  // Cosmic palette — keeps the 3D graph reading as deep space.
  const colorMap: Record<EdgeType, string> = {
    semantic_similarity: "#7c5cff", // violet
    explicit_reference: "#5ce0ff", // cyan
    temporal_adjacency: "#ff6aa1", // rose
    same_project: "#ffc36b", // amber
    same_session: "#a8efff", // pale cyan
    agent_inferred: "#b6a4ff", // pale violet
    provenance: "#e8e4fb", // paper
  }
  return colorMap[edgeType] || "#aea7d4"
}

export function getEdgeWidth(edgeType: EdgeType, weight: number): number {
  const baseWidth = (EDGE_CONFIG.edgeWidths as any)[edgeType] || 1
  return baseWidth * weight
}

export function shouldShowEdgeLabel(edge: GraphEdge, hoveredEdgeId: string | null): boolean {
  // Only show edge label when hovered
  return edge.id === hoveredEdgeId
}

export function filterEdgesByDistance(
  edges: VisibleEdge[],
  nodePositions: Map<string, { x: number; y: number; z: number }>,
  maxDistance: number = 300
): VisibleEdge[] {
  return edges.filter((edge) => {
    const sourcePos = nodePositions.get(edge.source)
    const targetPos = nodePositions.get(edge.target)

    if (!sourcePos || !targetPos) return false

    const dx = targetPos.x - sourcePos.x
    const dy = targetPos.y - sourcePos.y
    const dz = targetPos.z - sourcePos.z
    const distance = Math.sqrt(dx * dx + dy * dy + dz * dz)

    return distance < maxDistance
  })
}

export function groupEdgesByType(edges: VisibleEdge[]): Map<EdgeType, VisibleEdge[]> {
  const grouped = new Map<EdgeType, VisibleEdge[]>()

  for (const edge of edges) {
    if (!grouped.has(edge.edge_type)) {
      grouped.set(edge.edge_type, [])
    }
    grouped.get(edge.edge_type)!.push(edge)
  }

  return grouped
}

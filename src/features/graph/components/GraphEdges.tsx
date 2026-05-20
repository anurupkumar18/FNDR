import React, { useMemo } from "react"
import * as THREE from "three"
import type { GraphData, EdgeType } from "../types"
import { useGraphStore } from "../state/graphStore"
import { selectVisibleEdges, getEdgeColor } from "../layout/edgeVisibility"

interface NodeLayout {
  nodeId: string
  position: { x: number; y: number; z: number }
  x: number
  y: number
  z: number
}

interface GraphEdgesProps {
  graphData: GraphData
  nodePositions: NodeLayout[]
}

/** Per-edge-type, per-state lineSegments grouped into one geometry. Drawing
 *  all edges with a single BufferGeometry per (type, state) keeps the draw
 *  call count tiny — even at 500+ edges. */
interface EdgeGroupInput {
  edgeType: EdgeType
  positions: number[]
  isFocused: boolean
}

function EdgeGroupLines({ edgeType, positions, isFocused }: EdgeGroupInput) {
  const geometry = useMemo(() => {
    const g = new THREE.BufferGeometry()
    g.setAttribute("position", new THREE.BufferAttribute(new Float32Array(positions), 3))
    return g
  }, [positions])

  const material = useMemo(() => {
    return new THREE.LineBasicMaterial({
      color: getEdgeColor(edgeType),
      transparent: true,
      opacity: isFocused ? 0.75 : 0.22,
      blending: isFocused ? THREE.AdditiveBlending : THREE.NormalBlending,
      depthWrite: false,
      fog: true,
    })
  }, [edgeType, isFocused])

  if (positions.length === 0) return null
  return <lineSegments geometry={geometry} material={material} />
}

export const GraphEdges: React.FC<GraphEdgesProps> = ({ graphData, nodePositions }) => {
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId)
  const hoveredNodeId = useGraphStore((s) => s.hoveredNodeId)
  const enabledEdgeTypes = useGraphStore((s) => s.enabledEdgeTypes)

  const nodePositionMap = useMemo(
    () => new Map(nodePositions.map((p) => [p.nodeId, p.position])),
    [nodePositions],
  )

  // Show all enabled-type edges, capped at maxVisibleEdges. Bias selection
  // toward edges incident to the currently selected/hovered node so the
  // user can always see context.
  const visibleEdges = useMemo(() => {
    const focusSet = new Set<string>()
    if (selectedNodeId) focusSet.add(selectedNodeId)
    if (hoveredNodeId) focusSet.add(hoveredNodeId)
    return selectVisibleEdges(graphData.edges, focusSet, enabledEdgeTypes, 500)
  }, [graphData.edges, selectedNodeId, hoveredNodeId, enabledEdgeTypes])

  // Bucket edges by (edgeType, focused?) — focused = either endpoint matches
  // the selected/hovered node.
  const groups = useMemo(() => {
    const focusSet = new Set<string>()
    if (selectedNodeId) focusSet.add(selectedNodeId)
    if (hoveredNodeId) focusSet.add(hoveredNodeId)
    const map = new Map<string, EdgeGroupInput>()
    for (const edge of visibleEdges) {
      const s = nodePositionMap.get(edge.source)
      const t = nodePositionMap.get(edge.target)
      if (!s || !t) continue
      const focused = focusSet.has(edge.source) || focusSet.has(edge.target)
      const key = `${edge.edge_type}::${focused ? "f" : "n"}`
      let group = map.get(key)
      if (!group) {
        group = { edgeType: edge.edge_type, positions: [], isFocused: focused }
        map.set(key, group)
      }
      group.positions.push(s.x, s.y, s.z, t.x, t.y, t.z)
    }
    return Array.from(map.entries()).map(([key, group]) => ({ key, ...group }))
  }, [visibleEdges, nodePositionMap, selectedNodeId, hoveredNodeId])

  return (
    <group>
      {groups.map((g) => (
        <EdgeGroupLines
          key={g.key}
          edgeType={g.edgeType}
          positions={g.positions}
          isFocused={g.isFocused}
        />
      ))}
    </group>
  )
}

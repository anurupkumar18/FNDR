import React, { useMemo } from "react"
import * as THREE from "three"
import type { GraphNode, GraphCommunity } from "../types"
import { useGraphStore } from "../state/graphStore"
import { useViewportStore } from "../state/viewportStore"
import { getNodeDisplayTitle } from "../utils/displayTitle"

interface NodeLayout {
  nodeId: string
  position: { x: number; y: number; z: number }
  x: number
  y: number
  z: number
}

type LabelKind = "community" | "selected" | "hovered" | "node"

interface Label {
  id: string
  text: string
  position: { x: number; y: number; z: number }
  kind: LabelKind
}

interface GraphLabelsProps {
  graphData: { nodes: GraphNode[] }
  nodePositions: NodeLayout[]
  communities: GraphCommunity[]
}

const PROJECT_VEC = new THREE.Vector3()

export const GraphLabels: React.FC<GraphLabelsProps> = ({
  graphData,
  nodePositions,
  communities,
}) => {
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId)
  const hoveredNodeId = useGraphStore((s) => s.hoveredNodeId)
  const showLabels = useGraphStore((s) => s.showLabels)

  // Subscribe to the per-frame tick so we re-render at ~60fps while the
  // camera moves. The matrix itself lives on a stable instance, mutated in
  // place by ViewportSync — reading it doesn't need to trigger React.
  const tick = useViewportStore((s) => s.tick)
  const width = useViewportStore((s) => s.width)
  const height = useViewportStore((s) => s.height)

  const labels = useMemo<Label[]>(() => {
    const result: Label[] = []
    const nodeMap = new Map(graphData.nodes.map((n) => [n.id, n]))
    const maxLabels = 14

    // 1. Community headlines (always shown, up to 6)
    communities.slice(0, 6).forEach((community) => {
      result.push({
        id: `community-${community.id}`,
        text: community.label,
        position: community.anchor,
        kind: "community",
      })
    })

    // 2. Selected node label
    if (selectedNodeId && result.length < maxLabels) {
      const selectedNode = nodeMap.get(selectedNodeId)
      const selectedPos = nodePositions.find((p) => p.nodeId === selectedNodeId)
      if (selectedNode && selectedPos) {
        const title = getNodeDisplayTitle(selectedNode)
        result.push({
          id: `node-${selectedNodeId}`,
          text: title.length > 32 ? title.slice(0, 32) + "…" : title,
          position: selectedPos.position,
          kind: "selected",
        })
      }
    }

    // 3. Hovered node label (when different from selected)
    if (hoveredNodeId && hoveredNodeId !== selectedNodeId && result.length < maxLabels) {
      const hoveredNode = nodeMap.get(hoveredNodeId)
      const hoveredPos = nodePositions.find((p) => p.nodeId === hoveredNodeId)
      if (hoveredNode && hoveredPos) {
        const title = getNodeDisplayTitle(hoveredNode)
        result.push({
          id: `node-${hoveredNodeId}`,
          text: title.length > 32 ? title.slice(0, 32) + "…" : title,
          position: hoveredPos.position,
          kind: "hovered",
        })
      }
    }

    // 4. Top important nodes
    const importantNodes = graphData.nodes
      .filter(
        (n) =>
          n.id !== selectedNodeId &&
          n.id !== hoveredNodeId &&
          (n.importance_score ?? 0) > 0.75,
      )
      .sort((a, b) => (b.importance_score ?? 0) - (a.importance_score ?? 0))
      .slice(0, Math.max(0, maxLabels - result.length))

    importantNodes.forEach((node) => {
      const pos = nodePositions.find((p) => p.nodeId === node.id)
      if (pos && result.length < maxLabels) {
        const title = getNodeDisplayTitle(node)
        result.push({
          id: `node-${node.id}`,
          text: title.length > 24 ? title.slice(0, 24) + "…" : title,
          position: pos.position,
          kind: "node",
        })
      }
    })

    return result.slice(0, maxLabels)
  }, [selectedNodeId, hoveredNodeId, nodePositions, communities, graphData.nodes])

  if (!showLabels) return null
  // Hide labels until ViewportSync has reported a real size — projecting
  // against a 0×0 viewport would put everything in the corner.
  if (width === 0 || height === 0) return null

  // Pull a fresh reference to the matrix on every render (the underlying
  // matrix is mutated in place by ViewportSync). `tick` is the dep that
  // re-runs us each frame.
  const matrix = useViewportStore.getState().matrix
  void tick // ensure render is bound to the per-frame tick

  // Project each label.
  const projected = labels.map((label) => {
    PROJECT_VEC.set(label.position.x, label.position.y, label.position.z)
    PROJECT_VEC.applyMatrix4(matrix)
    // After projection: clip space. If w<=0, the point is behind the camera.
    // applyMatrix4 already does the perspective divide, so we have NDC in x/y.
    const behindCamera = PROJECT_VEC.z < -1 || PROJECT_VEC.z > 1
    const screenX = (PROJECT_VEC.x * 0.5 + 0.5) * width
    const screenY = (1 - (PROJECT_VEC.y * 0.5 + 0.5)) * height
    return {
      ...label,
      screenX,
      screenY,
      visible:
        !behindCamera &&
        screenX >= -80 &&
        screenY >= -40 &&
        screenX <= width + 80 &&
        screenY <= height + 40,
    }
  })

  return (
    <div className="absolute inset-0 pointer-events-none">
      {projected.map((label) => {
        if (!label.visible) return null
        return (
          <div
            key={label.id}
            className="g3d-label"
            data-kind={label.kind}
            style={{
              position: "absolute",
              left: `${label.screenX}px`,
              top: `${label.screenY}px`,
              zIndex:
                label.kind === "selected" ? 100 : label.kind === "community" ? 60 : 50,
            }}
          >
            {label.text}
          </div>
        )
      })}
    </div>
  )
}

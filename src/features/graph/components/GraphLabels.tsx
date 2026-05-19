import React, { useMemo } from "react"
import type { GraphNode, GraphCommunity } from "../types"
import { useGraphStore } from "../state/graphStore"
import { LABEL_CONFIG, COMMUNITY_COLORS } from "../constants"

interface NodeLayout {
  nodeId: string
  position: { x: number; y: number; z: number }
  x: number
  y: number
  z: number
}

interface Label {
  id: string
  text: string
  position: { x: number; y: number; z: number }
  color: string
  isSelected: boolean
  type: "community" | "node"
}

interface GraphLabelsProps {
  graphData: { nodes: GraphNode[]; edges: any[] }
  nodePositions: NodeLayout[]
  communities: GraphCommunity[]
}

function LabelElement({
  label,
  screenPosition,
}: {
  label: Label
  screenPosition: [number, number] | null
}) {
  if (!screenPosition) return null

  const [x, y] = screenPosition

  return (
    <div
      key={label.id}
      style={{
        position: "absolute",
        left: `${x}px`,
        top: `${y}px`,
        pointerEvents: "none",
        transform: "translate(-50%, -50%)",
        zIndex: label.isSelected ? 100 : 50,
      }}
      className="whitespace-nowrap"
    >
      <div
        className={`px-2 py-1 rounded text-xs font-medium ${
          label.isSelected ? "opacity-100 scale-105" : "opacity-75"
        } transition-all`}
        style={{
          backgroundColor: "rgba(10, 14, 39, 0.9)",
          color: label.color,
          border: `1px solid ${label.color}40`,
          maxWidth: "120px",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {label.text}
      </div>
    </div>
  )
}

export const GraphLabels: React.FC<GraphLabelsProps> = ({
  graphData,
  nodePositions,
  communities,
}) => {
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId)
  const hoveredNodeId = useGraphStore((s) => s.hoveredNodeId)
  const showLabels = useGraphStore((s) => s.showLabels)

  if (!showLabels) return null

  // Compute labels with strict discipline
  const labels = useMemo(() => {
    const result: Label[] = []
    const nodeMap = new Map(graphData.nodes.map((n) => [n.id, n]))

    // 1. Community labels (always shown)
    communities.forEach((community) => {
      result.push({
        id: `community-${community.id}`,
        text: community.label,
        position: community.anchor,
        color: COMMUNITY_COLORS[community.label] || "#CCCCCC",
        isSelected: false,
        type: "community",
      })
    })

    // 2. Selected node label
    if (selectedNodeId) {
      const selectedNode = nodeMap.get(selectedNodeId)
      const selectedPos = nodePositions.find((p) => p.nodeId === selectedNodeId)
      if (selectedNode && selectedPos) {
        result.push({
          id: `node-${selectedNodeId}`,
          text:
            selectedNode.title.length > LABEL_CONFIG.truncateLength
              ? selectedNode.title.substring(0, LABEL_CONFIG.truncateLength) + "…"
              : selectedNode.title,
          position: selectedPos.position,
          color: "#FFFFFF",
          isSelected: true,
          type: "node",
        })
      }
    }

    // 3. Hovered node label (only if different from selected)
    if (hoveredNodeId && hoveredNodeId !== selectedNodeId) {
      const hoveredNode = nodeMap.get(hoveredNodeId)
      const hoveredPos = nodePositions.find((p) => p.nodeId === hoveredNodeId)
      if (hoveredNode && hoveredPos) {
        result.push({
          id: `node-${hoveredNodeId}`,
          text:
            hoveredNode.title.length > LABEL_CONFIG.truncateLength
              ? hoveredNode.title.substring(0, LABEL_CONFIG.truncateLength) + "…"
              : hoveredNode.title,
          position: hoveredPos.position,
          color: "#FFFF99",
          isSelected: false,
          type: "node",
        })
      }
    }

    // 4. Top important nodes (up to limit, excluding selected/hovered)
    const importantNodes = graphData.nodes
      .filter(
        (n) =>
          n.id !== selectedNodeId &&
          n.id !== hoveredNodeId &&
          n.importance_score &&
          n.importance_score > 0.7
      )
      .sort((a, b) => (b.importance_score ?? 0) - (a.importance_score ?? 0))
      .slice(0, LABEL_CONFIG.topImportanceShown)

    importantNodes.forEach((node) => {
      const pos = nodePositions.find((p) => p.nodeId === node.id)
      if (pos && result.length < LABEL_CONFIG.maxLabelsVisible) {
        result.push({
          id: `node-${node.id}`,
          text:
            node.title.length > LABEL_CONFIG.truncateLength
              ? node.title.substring(0, LABEL_CONFIG.truncateLength) + "…"
              : node.title,
          position: pos.position,
          color: "#AAAAFF",
          isSelected: false,
          type: "node",
        })
      }
    })

    return result.slice(0, LABEL_CONFIG.maxLabelsVisible)
  }, [
    selectedNodeId,
    hoveredNodeId,
    nodePositions,
    communities,
    graphData.nodes,
    showLabels,
  ])

  return (
    <div className="absolute inset-0 pointer-events-none">
      {labels.map((label) => (
        <LabelElement key={label.id} label={label} screenPosition={null} />
      ))}
    </div>
  )
}

import React, { useMemo, useCallback, useRef } from "react"
import { useFrame } from "@react-three/fiber"
import * as THREE from "three"
import type { GraphData } from "../types"
import { useGraphStore } from "../state/graphStore"
import { getNodeGeometry } from "../rendering/geometries"
import { createNodeMaterial, createGlowMaterial } from "../rendering/materials"
import {
  getNodeSize,
  getNodeGlowIntensity,
  getNodeOpacity,
  computeNodeDepths,
} from "../layout/depthComputation"
import { COMMUNITY_COLORS } from "../constants"

interface NodeLayout {
  nodeId: string
  position: { x: number; y: number; z: number }
  x: number
  y: number
  z: number
}

interface GraphNodesProps {
  graphData: GraphData
  nodePositions: NodeLayout[]
}

interface NodeMeshProps {
  node: any
  position: { x: number; y: number; z: number }
  depthOffset: number
  isSelected: boolean
  isHovered: boolean
  onClick: () => void
  onPointerEnter: () => void
  onPointerLeave: () => void
}

const TARGET_SCALE = new THREE.Vector3()

function NodeMesh({
  node,
  position,
  depthOffset,
  onClick,
  onPointerEnter,
  onPointerLeave,
  isSelected,
  isHovered,
}: NodeMeshProps) {
  const groupRef = useRef<THREE.Group>(null)
  const meshRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)

  const size = useMemo(() => getNodeSize(node), [node])
  const glowIntensity = useMemo(() => getNodeGlowIntensity(node), [node])
  const opacity = useMemo(() => getNodeOpacity(node), [node])

  const color = useMemo(() => {
    if (node.community_id && COMMUNITY_COLORS[node.community_id]) {
      return COMMUNITY_COLORS[node.community_id]
    }
    const typeColors: Record<string, string> = {
      memory: "#7c5cff",
      entity: "#5ce0ff",
      community: "#ffc36b",
      evidence: "#ff6aa1",
      agent_context: "#b6a4ff",
    }
    return typeColors[node.node_type] || "#aea7d4"
  }, [node.community_id, node.node_type])

  const geometry = useMemo(() => getNodeGeometry(size), [size])

  const material = useMemo(() => {
    const baseMaterial = createNodeMaterial(color, color)
    baseMaterial.opacity = opacity
    baseMaterial.transparent = opacity < 1
    return baseMaterial
  }, [color, opacity])

  const glowMaterial = useMemo(
    () => createGlowMaterial(color, Math.max(0.6, glowIntensity)),
    [color, glowIntensity],
  )

  // Animate scale + emissive intensity each frame so hover/select feel responsive
  // without re-rendering the React tree on every transition.
  useFrame((_state, delta) => {
    const group = groupRef.current
    const mesh = meshRef.current
    const halo = haloRef.current
    if (!group || !mesh || !halo) return

    const targetScale = isSelected ? 1.18 : isHovered ? 1.1 : 1.0
    TARGET_SCALE.setScalar(targetScale)
    group.scale.lerp(TARGET_SCALE, Math.min(1, delta * 9))

    const m = mesh.material as THREE.MeshPhysicalMaterial
    const targetEmissive = isSelected ? 1.7 : isHovered ? 1.2 : 0.7 * glowIntensity
    m.emissiveIntensity += (targetEmissive - m.emissiveIntensity) * Math.min(1, delta * 9)

    const g = halo.material as THREE.MeshBasicMaterial
    const targetHaloOpacity = isSelected
      ? 0.85
      : isHovered
      ? 0.7
      : Math.min(0.55 * glowIntensity, 0.55)
    g.opacity += (targetHaloOpacity - g.opacity) * Math.min(1, delta * 9)
  })

  const adjustedZ = position.z + depthOffset
  const haloScale = isSelected || isHovered ? 2.6 : 2.2

  return (
    <group ref={groupRef} position={[position.x, position.y, adjustedZ]}>
      <mesh
        ref={meshRef}
        geometry={geometry}
        material={material}
        onClick={(e) => {
          e.stopPropagation()
          onClick()
        }}
        onPointerEnter={(e) => {
          e.stopPropagation()
          onPointerEnter()
          document.body.style.cursor = "pointer"
        }}
        onPointerLeave={(e) => {
          e.stopPropagation()
          onPointerLeave()
          document.body.style.cursor = ""
        }}
      />

      {/* Always-on additive halo — picked up by Bloom for the nebula glow. */}
      <mesh
        ref={haloRef}
        geometry={geometry}
        material={glowMaterial}
        scale={haloScale}
        raycast={() => null}
      />
    </group>
  )
}

export const GraphNodes: React.FC<GraphNodesProps> = ({ graphData, nodePositions }) => {
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId)
  const hoveredNodeId = useGraphStore((s) => s.hoveredNodeId)
  const setSelectedNodeId = useGraphStore((s) => s.setSelectedNodeId)
  const setHoveredNodeId = useGraphStore((s) => s.setHoveredNodeId)
  const enabledNodeTypes = useGraphStore((s) => s.enabledNodeTypes)
  const showEvidence = useGraphStore((s) => s.showEvidence)

  const depths = useMemo(() => computeNodeDepths(graphData), [graphData])
  const depthMap = useMemo(() => new Map(depths.map((d) => [d.nodeId, d])), [depths])

  const visibleNodes = useMemo(() => {
    return graphData.nodes.filter((node) => {
      if (!enabledNodeTypes.has(node.node_type)) return false
      if (node.node_type === "evidence" && !showEvidence) return false
      return true
    })
  }, [graphData.nodes, enabledNodeTypes, showEvidence])

  const handleNodeClick = useCallback(
    (nodeId: string) => {
      setSelectedNodeId(nodeId === selectedNodeId ? null : nodeId)
    },
    [selectedNodeId, setSelectedNodeId],
  )

  const handleNodeHover = useCallback(
    (nodeId: string) => {
      setHoveredNodeId(nodeId)
    },
    [setHoveredNodeId],
  )

  const handleNodeHoverOut = useCallback(() => {
    setHoveredNodeId(null)
  }, [setHoveredNodeId])

  return (
    <>
      {visibleNodes.map((node) => {
        const nodePos = nodePositions.find((p) => p.nodeId === node.id)
        if (!nodePos) return null

        const depth = depthMap.get(node.id)
        const depthOffset = depth?.depthOffset ?? 0
        const isSelected = node.id === selectedNodeId
        const isHovered = node.id === hoveredNodeId

        return (
          <NodeMesh
            key={node.id}
            node={node}
            position={nodePos.position}
            depthOffset={depthOffset}
            isSelected={isSelected}
            isHovered={isHovered}
            onClick={() => handleNodeClick(node.id)}
            onPointerEnter={() => handleNodeHover(node.id)}
            onPointerLeave={handleNodeHoverOut}
          />
        )
      })}
    </>
  )
}

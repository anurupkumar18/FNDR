import React, { useRef, useEffect, useMemo } from "react"
import { Canvas, useFrame, useThree } from "@react-three/fiber"
import { Stars } from "@react-three/drei"
import { EffectComposer, Bloom, Vignette } from "@react-three/postprocessing"
import { BlendFunction, KernelSize } from "postprocessing"
import * as THREE from "three"
import type { GraphData } from "../types"
import { GraphNodes } from "./GraphNodes"
import { GraphEdges } from "./GraphEdges"
import { computeCommunityAnchors, computeLocalNodePositions } from "../layout/communityLayout"
import { getNodeSize } from "../layout/depthComputation"
import { useGraphStore } from "../state/graphStore"
import { useViewportStore } from "../state/viewportStore"
import { CameraRig, type CameraRigHandle } from "./CameraRig"

interface GraphSceneProps {
  graphData: GraphData
}

function SceneContent({ graphData }: GraphSceneProps) {
  const rigRef = useRef<CameraRigHandle | null>(null)
  const hasFramedRef = useRef(false)
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId)

  const layout = useMemo(() => {
    const communities = computeCommunityAnchors(graphData.communities)
    const nodePositions = computeLocalNodePositions(graphData.nodes, communities)
    return { communities, nodePositions }
  }, [graphData.nodes, graphData.communities])

  // Frame the scene on first mount.
  useEffect(() => {
    if (hasFramedRef.current || !rigRef.current) return
    hasFramedRef.current = true

    const bounds = new THREE.Box3()
    layout.communities.forEach((c) => {
      bounds.expandByPoint(new THREE.Vector3(c.anchor.x, c.anchor.y, c.anchor.z))
    })
    const size = bounds.getSize(new THREE.Vector3())
    const maxDim = Math.max(size.x, size.y, size.z, 1)
    const distance = maxDim * 2.1
    rigRef.current.frame(distance)
  }, [layout.communities])

  // When a node is selected, smoothly fly the camera so it frames that node.
  const selectedNodeRef = useRef<string | null>(null)
  useEffect(() => {
    if (!rigRef.current) return
    if (selectedNodeId && selectedNodeId !== selectedNodeRef.current) {
      selectedNodeRef.current = selectedNodeId
      const nodeLayout = layout.nodePositions.find((n) => n.nodeId === selectedNodeId)
      const node = graphData.nodes.find((n) => n.id === selectedNodeId)
      if (nodeLayout && node) {
        rigRef.current.focusOn(
          new THREE.Vector3(nodeLayout.x, nodeLayout.y, nodeLayout.z),
          getNodeSize(node),
        )
      }
    } else if (!selectedNodeId) {
      selectedNodeRef.current = null
    }
  }, [selectedNodeId, layout.nodePositions, graphData.nodes])

  return (
    <>
      {/* Background gradient via fog + scene color */}
      <color attach="background" args={["#050714"]} />
      <fog attach="fog" args={["#050714", 120, 1400]} />

      {/* Ambient + chromatic rim lights */}
      <ambientLight intensity={0.35} color={"#8a8ad8"} />
      <pointLight position={[400, 300, 400]} intensity={1.2} color={"#7c5cff"} distance={2400} decay={1.4} />
      <pointLight position={[-400, -200, -300]} intensity={0.9} color={"#5ce0ff"} distance={2400} decay={1.4} />
      <pointLight position={[0, -400, 200]} intensity={0.5} color={"#ff6aa1"} distance={1800} decay={1.5} />

      {/* Starfield backdrop */}
      <Stars radius={900} depth={400} count={5000} factor={4} saturation={0.4} fade speed={0.3} />

      <GraphNodes graphData={graphData} nodePositions={layout.nodePositions} />
      <GraphEdges graphData={graphData} nodePositions={layout.nodePositions} />

      <CameraRig ref={rigRef} />

      <ViewportSync />

      <EffectComposer multisampling={4}>
        <Bloom
          intensity={1.1}
          luminanceThreshold={0.18}
          luminanceSmoothing={0.5}
          kernelSize={KernelSize.LARGE}
          mipmapBlur
        />
        <Vignette eskil={false} offset={0.25} darkness={0.55} blendFunction={BlendFunction.NORMAL} />
      </EffectComposer>
    </>
  )
}

/** Pushes the live camera projection + canvas size into the viewport store
 *  every frame so the DOM label overlay (outside the Canvas) can project
 *  node world positions to screen pixels. */
function ViewportSync() {
  const { camera, size } = useThree()
  const set = useViewportStore((s) => s.set)
  useFrame(() => {
    set(camera, size.width, size.height)
  })
  return null
}

export const GraphScene: React.FC<GraphSceneProps> = ({ graphData }) => {
  return (
    <Canvas
      camera={{ position: [0, 0, 500], fov: 60, near: 0.1, far: 5000 }}
      dpr={typeof window !== "undefined" ? Math.min(window.devicePixelRatio, 2) : 1}
      gl={{ antialias: true, alpha: false, powerPreference: "high-performance" }}
      style={{ width: "100%", height: "100%", display: "block" }}
    >
      <SceneContent graphData={graphData} />
    </Canvas>
  )
}

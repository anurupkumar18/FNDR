import React, { useEffect, useState, useCallback, useMemo, useRef } from "react"
import type { InsightGraphSubgraph } from "@/shared/ipc/tauri"
import { graphDataAdapter } from "../data/adapter"
import { normalizeInsightGraph } from "../data/normalizeInsightGraph"
import { computeCommunityAnchors, computeLocalNodePositions } from "../layout/communityLayout"
import { useGraphStore } from "../state/graphStore"
import type { GraphData } from "../types"
import { FocusType } from "../types"
import { GraphScene } from "./GraphScene"
import { GraphControls } from "./GraphControls"
import { GraphSidePanel } from "./GraphSidePanel"
import { GraphHoverCard } from "./GraphHoverCard"
import { GraphLabels } from "./GraphLabels"
import "./graph3d.css"

interface KnowledgeGraph3DProps {
  onClose?: () => void
  /** Optional bridged data from the 2D graph. When provided with nodes, used in place of the
   *  (currently stubbed) backend atlas command. */
  subgraph?: InsightGraphSubgraph | null
  louvain?: Record<string, number> | null
}

export const KnowledgeGraph3D: React.FC<KnowledgeGraph3DProps> = ({
  onClose,
  subgraph,
  louvain,
}) => {
  const [graphData, setGraphData] = useState<GraphData | null>(null)
  const [error, setError] = useState<string | null>(null)

  const mode = useGraphStore((s) => s.mode)
  const setMode = useGraphStore((s) => s.setMode)
  const setLoading = useGraphStore((s) => s.setLoading)
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId)
  const hoveredNodeId = useGraphStore((s) => s.hoveredNodeId)
  const enabledNodeTypes = useGraphStore((s) => s.enabledNodeTypes)
  const enabledEdgeTypes = useGraphStore((s) => s.enabledEdgeTypes)

  // Stable reference to the subgraph identity so we don't re-normalize on every render.
  // We only care when the size of the underlying data actually changes.
  const subgraphKey = useMemo(() => {
    if (!subgraph) return null
    return `${subgraph.nodes.length}:${subgraph.edges.length}:${subgraph.cluster_0_name ?? ""}`
  }, [subgraph])

  // Keep refs in sync with unstable object props so we can read them inside effects
  // without triggering re-runs when the parent re-renders.
  const subgraphRef = useRef<InsightGraphSubgraph | null>(null)
  const louvainRef = useRef<Record<string, number> | null>(null)

  useEffect(() => {
    subgraphRef.current = subgraph ?? null
  }, [subgraph])

  useEffect(() => {
    louvainRef.current = louvain ?? null
  }, [louvain])

  // Load graph data — priority: bridged subgraph > backend atlas command > error
  useEffect(() => {
    let cancelled = false

    const load = async () => {
      setLoading(true)
      setError(null)

      // Priority 1: bridged 2D subgraph
      if (subgraphRef.current && subgraphRef.current.nodes.length > 0) {
        const data = normalizeInsightGraph(subgraphRef.current, louvainRef.current)
        if (cancelled) return
        setGraphData(data)
        setLoading(false)
        return
      }

      // Priority 2: backend atlas command (currently stubbed; will work when implemented)
      try {
        const data =
          mode === "context"
            ? await graphDataAdapter.loadContextGraph({
                focus_type: FocusType.Atlas,
                label: "Full Memory Atlas",
              })
            : await graphDataAdapter.loadAtlasGraph()
        if (cancelled) return
        setGraphData(data)
      } catch (err) {
        if (cancelled) return
        const message = err instanceof Error ? err.message : "Failed to load graph"
        setError(message)
        console.error("[KnowledgeGraph3D] graph load error:", err)
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    void load()
    return () => {
      cancelled = true
    }
  }, [subgraphKey, mode, setLoading])

  const handleModeChange = useCallback(
    (newMode: "atlas" | "context") => {
      setMode(newMode)
      graphDataAdapter.clearCache()
    },
    [setMode]
  )

  // Compute layout for labels BEFORE any early returns (Rules of Hooks)
  // This must be called unconditionally on every render
  const { communities, nodePositions } = useMemo(() => {
    if (!graphData) return { communities: [], nodePositions: [] }
    const communities = computeCommunityAnchors(graphData.communities)
    const nodePositions = computeLocalNodePositions(graphData.nodes, communities)
    return { communities, nodePositions }
  }, [graphData])

  if (error) {
    return (
      <div className="knowledge-graph-3d-shell graph3d-root relative w-full h-full overflow-hidden">
        <div className="g3d-state">
          <h2 className="g3d-state-title">The graph couldn't load.</h2>
          <p className="g3d-state-text">{error}</p>
          {onClose && (
            <button type="button" className="g3d-pill" onClick={onClose}>
              Back to 2D
            </button>
          )}
        </div>
      </div>
    )
  }

  if (!graphData) {
    return (
      <div className="knowledge-graph-3d-shell graph3d-root relative w-full h-full overflow-hidden">
        <div className="g3d-state">
          <div className="g3d-spinner" />
          <p className="g3d-state-text">Developing the constellation…</p>
        </div>
      </div>
    )
  }

  if (!graphData.nodes || graphData.nodes.length === 0) {
    const looksFilteredOut = enabledNodeTypes.size < 3 || enabledEdgeTypes.size < 7
    return (
      <div className="knowledge-graph-3d-shell graph3d-root relative w-full h-full overflow-hidden">
        <div className="g3d-state">
          <h2 className="g3d-state-title">
            {looksFilteredOut ? "No nodes match the active filters." : "Nothing to map yet."}
          </h2>
          <p className="g3d-state-text">
            {looksFilteredOut
              ? "Loosen the filters or reset them to see the full graph."
              : "Capture some memories and the graph will start to form."}
          </p>
          {onClose && (
            <button type="button" className="g3d-pill" onClick={onClose}>
              Back to 2D
            </button>
          )}
        </div>
      </div>
    )
  }

  const selectedNode = graphData.nodes.find((n) => n.id === selectedNodeId)
  const hoveredNode = graphData.nodes.find((n) => n.id === hoveredNodeId)

  return (
    <div className="knowledge-graph-3d-shell graph3d-root relative w-full h-full overflow-hidden flex flex-col">
      {/* Main graph canvas */}
      <div className="flex-1 relative">
        <GraphScene graphData={graphData} />

        {/* Labels layer — positioned absolutely over canvas (DOM, NOT inside Canvas) */}
        <div className="absolute inset-0 pointer-events-none">
          <GraphLabels
            graphData={graphData}
            nodePositions={nodePositions}
            communities={communities}
          />
        </div>

        {/* Hover card */}
        {hoveredNode && hoveredNodeId !== selectedNodeId && (
          <GraphHoverCard node={hoveredNode} />
        )}

        {/* Controls */}
        <GraphControls onModeChange={handleModeChange} graphData={graphData} />
      </div>

      {/* Side panel */}
      {selectedNode && <GraphSidePanel node={selectedNode} graphData={graphData} />}

      {/* Close button */}
      {onClose && (
        <button
          type="button"
          className="g3d-icon-btn"
          data-variant="close"
          onClick={onClose}
          aria-label="Close 3D graph"
          title="Close graph"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </div>
  )
}

import React from "react"
import type { GraphData } from "../types"
import { useGraphStore } from "../state/graphStore"
import { NodeType, EdgeType } from "../types"

interface GraphControlsProps {
  onModeChange: (mode: "atlas" | "context") => void
  graphData: GraphData
}

export const GraphControls: React.FC<GraphControlsProps> = ({ onModeChange, graphData }) => {
  const mode = useGraphStore((s) => s.mode)
  const showLabels = useGraphStore((s) => s.showLabels)
  const showEvidence = useGraphStore((s) => s.showEvidence)
  const enabledNodeTypes = useGraphStore((s) => s.enabledNodeTypes)
  const enabledEdgeTypes = useGraphStore((s) => s.enabledEdgeTypes)

  const toggleShowLabels = useGraphStore((s) => s.toggleShowLabels)
  const toggleShowEvidence = useGraphStore((s) => s.toggleShowEvidence)
  const toggleNodeType = useGraphStore((s) => s.toggleNodeType)
  const toggleEdgeType = useGraphStore((s) => s.toggleEdgeType)
  const resetFilters = useGraphStore((s) => s.resetFilters)

  const [showFilters, setShowFilters] = React.useState(false)

  const nodeTypes: NodeType[] = [NodeType.Memory, NodeType.Entity, NodeType.Community]
  const edgeTypes: EdgeType[] = [
    EdgeType.SemanticSimilarity,
    EdgeType.ExplicitReference,
    EdgeType.SameProject,
    EdgeType.TemporalAdjacency,
  ]

  return (
    <div className="absolute top-4 left-4 flex gap-2 z-30">
      {/* Mode toggle */}
      <div className="flex gap-2 bg-slate-800 bg-opacity-80 rounded-lg p-1">
        <button
          onClick={() => onModeChange("atlas")}
          className={`px-3 py-2 rounded text-xs font-medium transition-colors ${
            mode === "atlas"
              ? "bg-blue-600 text-white"
              : "text-slate-300 hover:text-slate-100"
          }`}
        >
          Atlas
        </button>
        <button
          onClick={() => onModeChange("context")}
          className={`px-3 py-2 rounded text-xs font-medium transition-colors ${
            mode === "context"
              ? "bg-green-600 text-white"
              : "text-slate-300 hover:text-slate-100"
          }`}
        >
          Context
        </button>
      </div>

      {/* Labels toggle */}
      <button
        onClick={toggleShowLabels}
        className={`px-3 py-2 rounded text-xs font-medium transition-colors ${
          showLabels
            ? "bg-blue-600 text-white"
            : "bg-slate-800 bg-opacity-80 text-slate-300 hover:text-slate-100"
        }`}
        title="Toggle labels"
      >
        Labels
      </button>

      {/* Filters button */}
      <button
        onClick={() => setShowFilters(!showFilters)}
        className="px-3 py-2 rounded text-xs font-medium bg-slate-800 bg-opacity-80 text-slate-300 hover:text-slate-100 transition-colors"
      >
        Filters {showFilters ? "▼" : "▶"}
      </button>

      {/* Filters panel */}
      {showFilters && (
        <div className="absolute top-12 left-0 bg-slate-900 border border-slate-700 rounded-lg p-3 w-72 shadow-xl">
          <div className="space-y-4">
            {/* Node type filters */}
            <div>
              <p className="text-xs font-semibold text-slate-400 uppercase mb-2">Node Types</p>
              <div className="space-y-1">
                {nodeTypes.map((nodeType) => (
                  <label
                    key={nodeType}
                    className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer hover:text-slate-100"
                  >
                    <input
                      type="checkbox"
                      checked={enabledNodeTypes.has(nodeType)}
                      onChange={() => toggleNodeType(nodeType)}
                      className="w-4 h-4 rounded bg-slate-700 border-slate-600"
                    />
                    <span className="capitalize">{nodeType}</span>
                  </label>
                ))}
              </div>
            </div>

            {/* Edge type filters */}
            <div>
              <p className="text-xs font-semibold text-slate-400 uppercase mb-2">Edge Types</p>
              <div className="space-y-1">
                {edgeTypes.map((edgeType) => (
                  <label
                    key={edgeType}
                    className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer hover:text-slate-100"
                  >
                    <input
                      type="checkbox"
                      checked={enabledEdgeTypes.has(edgeType)}
                      onChange={() => toggleEdgeType(edgeType)}
                      className="w-4 h-4 rounded bg-slate-700 border-slate-600"
                    />
                    <span className="capitalize">{edgeType.replace(/_/g, " ")}</span>
                  </label>
                ))}
              </div>
            </div>

            {/* Evidence toggle */}
            <div>
              <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer hover:text-slate-100">
                <input
                  type="checkbox"
                  checked={showEvidence}
                  onChange={toggleShowEvidence}
                  className="w-4 h-4 rounded bg-slate-700 border-slate-600"
                />
                <span>Show evidence nodes</span>
              </label>
            </div>

            {/* Reset button */}
            <button
              onClick={resetFilters}
              className="w-full px-3 py-2 mt-2 bg-slate-700 hover:bg-slate-600 rounded text-xs text-slate-300 hover:text-slate-100 transition-colors"
            >
              Reset filters
            </button>

            {/* Info */}
            <div className="text-xs text-slate-500 border-t border-slate-700 pt-2">
              <div>Nodes: {graphData.nodes.length}</div>
              <div>Edges: {graphData.edges.length}</div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

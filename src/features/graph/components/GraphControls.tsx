import React from "react"
import type { GraphData } from "../types"
import { useGraphStore } from "../state/graphStore"
import { NodeType, EdgeType } from "../types"
import { EDGE_COLORS } from "../constants"

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
    <div className="g3d-toolbar">
      <div className="g3d-segmented" role="tablist" aria-label="Graph mode">
        <button
          type="button"
          className="g3d-pill"
          data-active={mode === "atlas"}
          role="tab"
          aria-selected={mode === "atlas"}
          onClick={() => onModeChange("atlas")}
        >
          Atlas
        </button>
        <button
          type="button"
          className="g3d-pill"
          data-active={mode === "context"}
          role="tab"
          aria-selected={mode === "context"}
          onClick={() => onModeChange("context")}
        >
          Context
        </button>
      </div>

      <button
        type="button"
        className="g3d-pill"
        data-active={showLabels}
        onClick={toggleShowLabels}
        title="Toggle labels"
      >
        Labels
      </button>

      <div style={{ position: "relative" }}>
        <button
          type="button"
          className="g3d-pill"
          data-active={showFilters}
          onClick={() => setShowFilters((v) => !v)}
          aria-expanded={showFilters}
        >
          Filters {showFilters ? "▾" : "▸"}
        </button>

        {showFilters && (
          <div className="g3d-popover" role="dialog" aria-label="Graph filters">
            <div>
              <p className="g3d-section-title">Node types</p>
              {nodeTypes.map((nodeType) => (
                <label key={nodeType} className="g3d-checkbox-row">
                  <input
                    type="checkbox"
                    className="g3d-checkbox"
                    checked={enabledNodeTypes.has(nodeType)}
                    onChange={() => toggleNodeType(nodeType)}
                  />
                  <span>{nodeType}</span>
                </label>
              ))}
            </div>

            <div>
              <p className="g3d-section-title">Edge types</p>
              {edgeTypes.map((edgeType) => (
                <label key={edgeType} className="g3d-checkbox-row">
                  <input
                    type="checkbox"
                    className="g3d-checkbox"
                    checked={enabledEdgeTypes.has(edgeType)}
                    onChange={() => toggleEdgeType(edgeType)}
                  />
                  <span
                    className="g3d-edge-swatch"
                    style={{
                      background: EDGE_COLORS[edgeType] ?? "var(--g3d-violet)",
                      color: EDGE_COLORS[edgeType] ?? "var(--g3d-violet)",
                    }}
                  />
                  <span>{edgeType.replace(/_/g, " ")}</span>
                </label>
              ))}
            </div>

            <label className="g3d-checkbox-row">
              <input
                type="checkbox"
                className="g3d-checkbox"
                checked={showEvidence}
                onChange={toggleShowEvidence}
              />
              <span>Show evidence nodes</span>
            </label>

            <button type="button" className="g3d-popover-reset" onClick={resetFilters}>
              Reset filters
            </button>

            <div className="g3d-popover-footer">
              <span>
                {graphData.nodes.length} nodes · {graphData.edges.length} edges
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

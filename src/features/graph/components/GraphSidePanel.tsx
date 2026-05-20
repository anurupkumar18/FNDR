import React, { useMemo } from "react"
import { format } from "date-fns"
import type { GraphNode, GraphData } from "../types"
import { useGraphStore } from "../state/graphStore"
import { getDisplayLabel } from "../utils/displayTitle"

interface GraphSidePanelProps {
  node: GraphNode
  graphData: GraphData
}

export const GraphSidePanel: React.FC<GraphSidePanelProps> = ({ node, graphData }) => {
  const setSelectedNodeId = useGraphStore((s) => s.setSelectedNodeId)

  const formattedTime = useMemo(() => {
    if (!node.timestamp_start) return null
    try {
      return format(new Date(node.timestamp_start), "PPpp")
    } catch {
      return null
    }
  }, [node.timestamp_start])

  const connectedNodeIds = useMemo(() => {
    const edges = graphData.edges.filter(
      (e) => e.source === node.id || e.target === node.id
    )
    return edges.map((e) => (e.source === node.id ? e.target : e.source))
  }, [node.id, graphData.edges])

  const connectedNodes = useMemo(() => {
    return graphData.nodes.filter((n) => connectedNodeIds.includes(n.id)).slice(0, 6)
  }, [graphData.nodes, connectedNodeIds])

  const [showEvidence, setShowEvidence] = React.useState(false)

  const metaRows: Array<[string, string]> = [
    node.app_name ? ["App", node.app_name] : null,
    node.project ? ["Project", node.project] : null,
    node.topic ? ["Topic", node.topic] : null,
    node.window_title ? ["Window", node.window_title] : null,
    node.url ? ["URL", node.url] : null,
    formattedTime ? ["Timestamp", formattedTime] : null,
  ].filter((row): row is [string, string] => row !== null)

  return (
    <aside className="g3d-side-panel" aria-label="Selected node detail">
      <header className="g3d-side-header">
        <h2 className="g3d-side-title">{getDisplayLabel(node)}</h2>
        <button
          type="button"
          className="g3d-icon-btn"
          onClick={() => setSelectedNodeId(null)}
          aria-label="Close detail panel"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </header>

      <div className="g3d-side-body">
        {node.summary && <p className="g3d-side-summary">{node.summary}</p>}

        {metaRows.length > 0 && (
          <section>
            <p className="g3d-section-title">Metadata</p>
            <div className="g3d-side-meta">
              {metaRows.map(([key, val]) => (
                <div key={key} className="g3d-side-meta-row">
                  <span className="g3d-side-meta-key">{key}</span>
                  <span className="g3d-side-meta-val" title={val}>
                    {val}
                  </span>
                </div>
              ))}
            </div>
          </section>
        )}

        {(node.importance_score !== undefined ||
          node.relevance_score !== undefined ||
          node.confidence_score !== undefined) && (
          <section>
            <p className="g3d-section-title">Scores</p>
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              {node.importance_score !== undefined && (
                <div className="g3d-score-row">
                  <div className="g3d-score-head">
                    <span>Importance</span>
                    <span>{(node.importance_score * 100).toFixed(0)}%</span>
                  </div>
                  <div className="g3d-score-bar">
                    <div
                      className="g3d-score-fill"
                      style={{ width: `${Math.max(0, Math.min(1, node.importance_score)) * 100}%` }}
                    />
                  </div>
                </div>
              )}
              {node.relevance_score !== undefined && (
                <div className="g3d-score-row">
                  <div className="g3d-score-head">
                    <span>Relevance</span>
                    <span>{(node.relevance_score * 100).toFixed(0)}%</span>
                  </div>
                  <div className="g3d-score-bar">
                    <div
                      className="g3d-score-fill"
                      data-tone="cyan"
                      style={{ width: `${Math.max(0, Math.min(1, node.relevance_score)) * 100}%` }}
                    />
                  </div>
                </div>
              )}
              {node.confidence_score !== undefined && (
                <div className="g3d-score-row">
                  <div className="g3d-score-head">
                    <span>Confidence</span>
                    <span>{(node.confidence_score * 100).toFixed(0)}%</span>
                  </div>
                  <div className="g3d-score-bar">
                    <div
                      className="g3d-score-fill"
                      data-tone="rose"
                      style={{ width: `${Math.max(0, Math.min(1, node.confidence_score)) * 100}%` }}
                    />
                  </div>
                </div>
              )}
            </div>
          </section>
        )}

        {connectedNodes.length > 0 && (
          <section>
            <p className="g3d-section-title">Related memories</p>
            <div className="g3d-related-list">
              {connectedNodes.map((connected) => (
                <button
                  key={connected.id}
                  type="button"
                  className="g3d-related-item"
                  onClick={() => setSelectedNodeId(connected.id)}
                >
                  <div>{getDisplayLabel(connected)}</div>
                  {connected.project && (
                    <div className="g3d-related-sub">{connected.project}</div>
                  )}
                </button>
              ))}
            </div>
          </section>
        )}

        {node.metadata && Object.keys(node.metadata).length > 0 && (
          <section>
            <button
              type="button"
              className="g3d-disclosure"
              onClick={() => setShowEvidence((v) => !v)}
              aria-expanded={showEvidence}
            >
              <svg
                width="10"
                height="10"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                style={{
                  transform: showEvidence ? "rotate(90deg)" : "rotate(0)",
                  transition: "transform 200ms ease-out",
                }}
              >
                <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
              </svg>
              Provenance
            </button>
            {showEvidence && (
              <pre className="g3d-evidence-block">
                {JSON.stringify(node.metadata, null, 2)}
              </pre>
            )}
          </section>
        )}

        <div className="g3d-side-actions">
          <button type="button" className="g3d-side-action">
            Search around this
          </button>
          <button type="button" className="g3d-side-action">
            Focus graph here
          </button>
        </div>
      </div>
    </aside>
  )
}

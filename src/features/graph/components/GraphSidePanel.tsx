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

  // Find connected nodes
  const connectedNodeIds = useMemo(() => {
    const edges = graphData.edges.filter(
      (e) => e.source === node.id || e.target === node.id
    )
    return edges.map((e) => (e.source === node.id ? e.target : e.source))
  }, [node.id, graphData.edges])

  const connectedNodes = useMemo(() => {
    return graphData.nodes.filter((n) => connectedNodeIds.includes(n.id)).slice(0, 5)
  }, [graphData.nodes, connectedNodeIds])

  const [showEvidence, setShowEvidence] = React.useState(false)

  return (
    <div className="absolute right-0 top-0 bottom-0 w-80 bg-slate-900 border-l border-slate-700 overflow-y-auto z-40">
      {/* Header */}
      <div className="sticky top-0 bg-slate-900 border-b border-slate-700 p-4 flex justify-between items-start">
        <h2 className="text-lg font-bold text-white flex-1 pr-2 line-clamp-2">
          {getDisplayLabel(node)}
        </h2>
        <button
          onClick={() => setSelectedNodeId(null)}
          className="flex-shrink-0 text-slate-400 hover:text-slate-200"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div className="p-4 space-y-6">
        {/* Summary */}
        {node.summary && (
          <div>
            <p className="text-xs font-semibold text-slate-400 uppercase tracking-wide mb-2">
              Summary
            </p>
            <p className="text-sm text-slate-300">{node.summary}</p>
          </div>
        )}

        {/* Metadata */}
        <div>
          <p className="text-xs font-semibold text-slate-400 uppercase tracking-wide mb-3">
            Metadata
          </p>
          <div className="space-y-2 text-sm">
            {node.app_name && (
              <div>
                <span className="text-slate-500">App:</span>
                <span className="ml-2 text-slate-300">{node.app_name}</span>
              </div>
            )}

            {node.project && (
              <div>
                <span className="text-slate-500">Project:</span>
                <span className="ml-2 text-slate-300">{node.project}</span>
              </div>
            )}

            {node.topic && (
              <div>
                <span className="text-slate-500">Topic:</span>
                <span className="ml-2 text-slate-300">{node.topic}</span>
              </div>
            )}

            {node.window_title && (
              <div>
                <span className="text-slate-500">Window:</span>
                <span className="ml-2 text-slate-300 truncate">{node.window_title}</span>
              </div>
            )}

            {node.url && (
              <div>
                <span className="text-slate-500">URL:</span>
                <span className="ml-2 text-slate-300 text-xs truncate">{node.url}</span>
              </div>
            )}

            {formattedTime && (
              <div>
                <span className="text-slate-500">Timestamp:</span>
                <span className="ml-2 text-slate-300">{formattedTime}</span>
              </div>
            )}
          </div>
        </div>

        {/* Scores */}
        <div>
          <p className="text-xs font-semibold text-slate-400 uppercase tracking-wide mb-3">
            Scores
          </p>
          <div className="space-y-3">
            {node.importance_score !== undefined && (
              <div>
                <div className="flex justify-between mb-1">
                  <span className="text-xs text-slate-400">Importance</span>
                  <span className="text-xs text-slate-400">
                    {(node.importance_score * 100).toFixed(0)}%
                  </span>
                </div>
                <div className="h-2 bg-slate-800 rounded overflow-hidden">
                  <div
                    className="h-full bg-blue-500"
                    style={{ width: `${node.importance_score * 100}%` }}
                  />
                </div>
              </div>
            )}

            {node.relevance_score !== undefined && (
              <div>
                <div className="flex justify-between mb-1">
                  <span className="text-xs text-slate-400">Relevance</span>
                  <span className="text-xs text-slate-400">
                    {(node.relevance_score * 100).toFixed(0)}%
                  </span>
                </div>
                <div className="h-2 bg-slate-800 rounded overflow-hidden">
                  <div
                    className="h-full bg-green-500"
                    style={{ width: `${node.relevance_score * 100}%` }}
                  />
                </div>
              </div>
            )}

            {node.confidence_score !== undefined && (
              <div>
                <div className="flex justify-between mb-1">
                  <span className="text-xs text-slate-400">Confidence</span>
                  <span className="text-xs text-slate-400">
                    {(node.confidence_score * 100).toFixed(0)}%
                  </span>
                </div>
                <div className="h-2 bg-slate-800 rounded overflow-hidden">
                  <div
                    className="h-full bg-purple-500"
                    style={{ width: `${node.confidence_score * 100}%` }}
                  />
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Connected nodes */}
        {connectedNodes.length > 0 && (
          <div>
            <p className="text-xs font-semibold text-slate-400 uppercase tracking-wide mb-2">
              Related Memories
            </p>
            <div className="space-y-2">
              {connectedNodes.map((connected) => (
                <button
                  key={connected.id}
                  onClick={() => setSelectedNodeId(connected.id)}
                  className="w-full text-left p-2 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-300 hover:text-slate-100 transition-colors"
                >
                  <div className="line-clamp-2">{getDisplayLabel(connected)}</div>
                  {connected.project && (
                    <div className="text-slate-500 text-xs mt-1">{connected.project}</div>
                  )}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Evidence section (collapsed by default) */}
        {node.metadata && Object.keys(node.metadata).length > 0 && (
          <div>
            <button
              onClick={() => setShowEvidence(!showEvidence)}
              className="text-xs font-semibold text-slate-400 uppercase tracking-wide hover:text-slate-200 flex items-center gap-2"
            >
              <svg
                className={`w-3 h-3 transition-transform ${showEvidence ? "rotate-90" : ""}`}
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
              </svg>
              Provenance
            </button>

            {showEvidence && (
              <div className="mt-2 bg-slate-950 border border-slate-800 rounded p-2 text-xs text-slate-400">
                <pre className="overflow-auto max-h-48 whitespace-pre-wrap break-words font-mono text-xs">
                  {JSON.stringify(node.metadata, null, 2)}
                </pre>
              </div>
            )}
          </div>
        )}

        {/* Actions */}
        <div className="border-t border-slate-700 pt-4 space-y-2">
          <button className="w-full px-3 py-2 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-300 hover:text-slate-100 transition-colors">
            Search around this
          </button>
          <button className="w-full px-3 py-2 bg-slate-800 hover:bg-slate-700 rounded text-xs text-slate-300 hover:text-slate-100 transition-colors">
            Focus graph here
          </button>
        </div>
      </div>
    </div>
  )
}

import React, { useMemo } from "react"
import type { GraphNode } from "../types"
import { format } from "date-fns"

interface GraphHoverCardProps {
  node: GraphNode
}

export const GraphHoverCard: React.FC<GraphHoverCardProps> = ({ node }) => {
  const formattedTime = useMemo(() => {
    if (!node.timestamp_start) return null
    try {
      return format(new Date(node.timestamp_start), "MMM d, h:mm a")
    } catch {
      return null
    }
  }, [node.timestamp_start])

  return (
    <div
      className="fixed pointer-events-none z-40 bg-slate-800 border border-slate-700 rounded-lg shadow-xl"
      style={{
        left: "50%",
        top: "50%",
        transform: "translate(-50%, -120%)",
        maxWidth: "280px",
        padding: "12px",
      }}
    >
      {/* Title */}
      <div className="font-semibold text-sm text-white mb-2 line-clamp-2">
        {node.title}
      </div>

      {/* Metadata row */}
      <div className="text-xs text-slate-400 space-y-1 mb-2">
        {node.app_name && (
          <div className="flex items-center gap-2">
            <span className="text-slate-500">App:</span>
            <span className="text-slate-300">{node.app_name}</span>
          </div>
        )}

        {formattedTime && (
          <div className="flex items-center gap-2">
            <span className="text-slate-500">Time:</span>
            <span className="text-slate-300">{formattedTime}</span>
          </div>
        )}

        {node.project && (
          <div className="flex items-center gap-2">
            <span className="text-slate-500">Project:</span>
            <span className="text-slate-300">{node.project}</span>
          </div>
        )}
      </div>

      {/* Summary */}
      {node.summary && (
        <div className="text-xs text-slate-300 bg-slate-900 bg-opacity-50 p-2 rounded mb-2 line-clamp-3">
          {node.summary}
        </div>
      )}

      {/* Scores */}
      <div className="text-xs text-slate-400 space-y-1">
        {node.importance_score !== undefined && (
          <div className="flex justify-between">
            <span>Importance</span>
            <div className="w-20 h-1 bg-slate-700 rounded">
              <div
                className="h-full bg-blue-500 rounded"
                style={{ width: `${(node.importance_score ?? 0) * 100}%` }}
              />
            </div>
          </div>
        )}

        {node.confidence_score !== undefined && (
          <div className="flex justify-between">
            <span>Confidence</span>
            <div className="w-20 h-1 bg-slate-700 rounded">
              <div
                className="h-full bg-green-500 rounded"
                style={{ width: `${(node.confidence_score ?? 0) * 100}%` }}
              />
            </div>
          </div>
        )}
      </div>

      {/* Arrow */}
      <div
        className="absolute left-1/2 -translate-x-1/2 transform"
        style={{
          bottom: "-6px",
          width: 0,
          height: 0,
          borderLeft: "6px solid transparent",
          borderRight: "6px solid transparent",
          borderTop: "6px solid rgb(30, 41, 59)",
        }}
      />
    </div>
  )
}

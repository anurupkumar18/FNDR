import React, { useMemo } from "react"
import type { GraphNode } from "../types"
import { format } from "date-fns"
import { getDisplayLabel } from "../utils/displayTitle"

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
    <div className="g3d-hover-card" role="status" aria-live="polite">
      <h3 className="g3d-hover-title">{getDisplayLabel(node)}</h3>

      <div className="g3d-hover-meta">
        {node.app_name && <span>{node.app_name}</span>}
        {node.project && <span>{node.project}</span>}
        {formattedTime && <span>{formattedTime}</span>}
      </div>

      {node.summary && <p className="g3d-hover-summary">{node.summary}</p>}
    </div>
  )
}

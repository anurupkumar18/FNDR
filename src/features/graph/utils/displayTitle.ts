import type { GraphNode } from "../types"
import { NodeType } from "../types"

type AnyNodeShape = {
  id?: string
  title?: string
  label?: string
  summary?: string
  project?: string
  topic?: string
  activity_type?: string
  app_name?: string
  window_title?: string
  url?: string
  node_type?: string | NodeType
  metadata?: unknown
}

export function isIdLikeTitle(title: string, nodeId?: string): boolean {
  if (!title) return true
  const trimmed = title.trim()
  if (!trimmed) return true

  // Equals node id
  if (nodeId && trimmed === nodeId) return true

  // Common backend ID prefixes
  if (/^memory\s/i.test(trimmed)) return true
  if (/^mem_/i.test(trimmed)) return true
  if (/^entity\s/i.test(trimmed)) return true
  if (/^ent_/i.test(trimmed)) return true
  if (/^node_/i.test(trimmed)) return true

  // UUID-like (8-4-4-4-12 hex)
  if (/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(trimmed)) return true
  // Shorter UUID prefix
  if (/^[a-f0-9]{8}-[a-f0-9]{4}/i.test(trimmed)) return true

  // Pure hex hash (8+ chars)
  if (/^[a-f0-9]{8,}$/i.test(trimmed)) return true

  // Truncated hash patterns like "a4bee83..." or "a4bee83…"
  if (/^[a-f0-9]{4,}(\.\.\.|…)/i.test(trimmed)) return true

  // Hash-like words inside the string (e.g. "memory a4bee83d…")
  if (/[a-f0-9]{6,}(\.\.\.|…)/i.test(trimmed)) return true

  // Long random alphanumeric run
  if (trimmed.length > 16 && /^[a-z0-9_-]{16,}$/i.test(trimmed)) return true

  // Low alphabetic word content (>70% non-letter characters)
  const letters = trimmed.replace(/[^a-zA-Z]/g, "")
  if (trimmed.length >= 8 && letters.length / trimmed.length < 0.3) return true

  return false
}

function fallbackByType(node: AnyNodeShape): string {
  const t = (node.node_type ?? "").toString().toLowerCase()
  if (t.includes("entity")) return "Entity"
  if (t.includes("community")) return "Community"
  if (t.includes("evidence")) return "Evidence"
  return "Memory"
}

export function getDisplayLabel(node: AnyNodeShape): string {
  if (!node) return "Memory"

  // 1. Primary user-set name (title for 3D GraphNode, label for 2D InsightGraphNode)
  const primary = (node.title ?? node.label ?? "").trim()
  if (primary && !isIdLikeTitle(primary, node.id)) {
    return primary.length > 40 ? primary.slice(0, 40) : primary
  }

  // 2. First sentence of summary
  if (node.summary) {
    const firstSentence = node.summary.split(/[.!?]+/)[0].trim()
    if (firstSentence && !isIdLikeTitle(firstSentence, node.id)) {
      return firstSentence.length > 40 ? firstSentence.slice(0, 40) : firstSentence
    }
  }

  // 3. Project / topic / activity type
  if (node.project && !isIdLikeTitle(node.project, node.id)) {
    return node.project.length > 30 ? node.project.slice(0, 30) : node.project
  }
  if (node.topic && !isIdLikeTitle(node.topic, node.id)) {
    return node.topic.length > 30 ? node.topic.slice(0, 30) : node.topic
  }
  if (node.activity_type && !isIdLikeTitle(node.activity_type, node.id)) {
    return node.activity_type.length > 30 ? node.activity_type.slice(0, 30) : node.activity_type
  }

  // 4. App name + window title
  if (node.app_name) {
    if (node.window_title && !isIdLikeTitle(node.window_title, node.id)) {
      const combined = `${node.app_name}: ${node.window_title}`
      return combined.length > 40 ? combined.slice(0, 40) : combined
    }
    if (!isIdLikeTitle(node.app_name, node.id)) {
      return node.app_name.length > 30 ? node.app_name.slice(0, 30) : node.app_name
    }
  }

  // 5. URL hostname or file name
  if (node.url) {
    try {
      const u = new URL(node.url)
      const host = u.hostname.replace(/^www\./, "")
      if (host) return host
      const pathParts = u.pathname.split("/").filter(Boolean)
      if (pathParts.length > 0) return pathParts[pathParts.length - 1]
    } catch {
      // Treat as file path
      const fileName = node.url.split(/[\/\\]/).pop()
      if (fileName && !isIdLikeTitle(fileName, node.id)) return fileName
    }
  }

  // 6. Type-based fallback
  return fallbackByType(node)
}

// Back-compat: existing 3D code uses getNodeDisplayTitle
export function getNodeDisplayTitle(node: GraphNode): string {
  return getDisplayLabel(node as AnyNodeShape)
}

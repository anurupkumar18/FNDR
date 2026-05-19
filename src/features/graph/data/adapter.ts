import { invoke } from "@tauri-apps/api/core"
import type { GraphData, ActiveFocus } from "../types"
import { FocusType } from "../types"

export interface AtlasGraphParams {
  includeEvidence?: boolean
}

export interface ContextGraphParams {
  depth?: number
  includeEvidence?: boolean
}

export class GraphDataAdapter {
  private cache: Map<string, GraphData> = new Map()
  private cacheExpiry: number = 60000 // 1 minute

  async loadAtlasGraph(): Promise<GraphData> {
    const cacheKey = "atlas"
    const cached = this.cache.get(cacheKey)
    if (cached) {
      console.debug("[GraphDataAdapter] Using cached atlas graph")
      return cached
    }

    try {
      // Primary path: call backend graph command
      const startTime = performance.now()
      console.debug("[GraphDataAdapter] 📊 FETCH START: Loading atlas graph from backend")
      const data: GraphData = await invoke("get_memory_graph_atlas")
      const elapsed = Math.round(performance.now() - startTime)
      console.debug(
        `[GraphDataAdapter] 📊 FETCH DONE (${elapsed}ms): ${data.nodes.length} nodes, ${data.edges.length} edges, ${data.communities.length} communities`
      )
      this.cache.set(cacheKey, data)
      setTimeout(() => this.cache.delete(cacheKey), this.cacheExpiry)
      return data
    } catch (error) {
      console.error("[GraphDataAdapter] ❌ Backend failed, using fallback:", error)
      // Fallback: return empty graph (Phase 2 fallback to memory cards would go here)
      return {
        nodes: [],
        edges: [],
        communities: [],
        active_focus: { focus_type: FocusType.Atlas, label: "Full Memory Atlas" },
      }
    }
  }

  async loadContextGraph(focus: ActiveFocus): Promise<GraphData> {
    const cacheKey = `context-${focus.id}-${focus.query}`
    const cached = this.cache.get(cacheKey)
    if (cached) {
      return cached
    }

    try {
      // Primary path: call backend graph command
      console.debug("[GraphDataAdapter] Loading context graph from backend", { focus })
      const data: GraphData = await invoke("get_memory_graph_context", {
        focusId: focus.id,
        query: focus.query,
      })
      console.debug(
        `[GraphDataAdapter] Backend context graph loaded: ${data.nodes.length} nodes, ${data.edges.length} edges`
      )
      this.cache.set(cacheKey, data)
      setTimeout(() => this.cache.delete(cacheKey), this.cacheExpiry)
      return data
    } catch (error) {
      console.error("[GraphDataAdapter] Backend context graph command failed, using empty fallback:", error)
      console.warn(
        "[GraphDataAdapter] ⚠️ FALLBACK PATH USED - Context graph data may be incomplete. This is a compatibility fallback only."
      )
      // Fallback: return empty graph with focus
      return {
        nodes: [],
        edges: [],
        communities: [],
        active_focus: focus,
      }
    }
  }

  async getNodeNeighborhood(nodeId: string, depth: number = 1): Promise<GraphData> {
    try {
      const data: GraphData = await invoke("get_graph_node_neighborhood", {
        nodeId,
        depth,
      })
      return data
    } catch (error) {
      console.warn("Neighborhood query failed:", error)
      return { nodes: [], edges: [], communities: [], active_focus: undefined }
    }
  }

  async getCommunities(): Promise<any[]> {
    try {
      const communities = await invoke("get_graph_communities")
      return communities as any[]
    } catch (error) {
      console.warn("Community fetch failed:", error)
      return []
    }
  }

  clearCache(): void {
    this.cache.clear()
  }
}

export const graphDataAdapter = new GraphDataAdapter()

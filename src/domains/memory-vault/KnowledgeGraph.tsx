import { useEffect, useMemo, useRef } from "react";
import * as d3 from "d3";
import type { InsightGraphEdge, InsightGraphNode } from "@/shared/ipc/tauri";
import { GRAPH_SIM_MAX_TICKS } from "./useGraph";
import "./KnowledgeGraph.css";

interface SimNode extends d3.SimulationNodeDatum {
    id: string;
    label: string;
    nodeType: string;
    raw: InsightGraphNode;
}

interface SimLink extends d3.SimulationLinkDatum<SimNode> {
    id: string;
    confidence: number;
}

export interface KnowledgeGraphProps {
    nodes: InsightGraphNode[];
    edges: InsightGraphEdge[];
    height?: number;
    onNodeClick?: (node: InsightGraphNode) => void;
    selectedNodeId?: string | null;
    pathNodeIds?: readonly string[] | null;
    highlightNodeIds?: readonly string[] | null;
    /** When set, pulls same-community nodes toward shared foci (Louvain ids from backend). */
    louvainByNodeId?: Record<string, number> | null;
    /** Defaults to {@link GRAPH_SIM_MAX_TICKS}. */
    maxSimulationTicks?: number;
}

export function KnowledgeGraph({
    nodes,
    edges,
    height = 480,
    onNodeClick,
    selectedNodeId = null,
    pathNodeIds = null,
    highlightNodeIds = null,
    louvainByNodeId = null,
    maxSimulationTicks = GRAPH_SIM_MAX_TICKS,
}: KnowledgeGraphProps) {
    const ref = useRef<SVGSVGElement | null>(null);

    const simNodes = useMemo<SimNode[]>(
        () =>
            nodes.map((n) => ({
                id: n.id,
                label: n.label.length > 42 ? `${n.label.slice(0, 40)}…` : n.label,
                nodeType: n.node_type,
                raw: n,
            })),
        [nodes]
    );

    const simLinks = useMemo<SimLink[]>(() => {
        const idSet = new Set(simNodes.map((n) => n.id));
        return edges
            .filter((e) => idSet.has(e.source_id) && idSet.has(e.target_id))
            .map((e) => ({
                id: e.id,
                source: e.source_id,
                target: e.target_id,
                confidence: e.confidence,
            }));
    }, [edges, simNodes]);

    useEffect(() => {
        const svgEl = ref.current;
        if (!svgEl) {
            return;
        }
        const width = svgEl.clientWidth || 640;
        svgEl.innerHTML = "";

        const root = d3.select(svgEl);
        const gRoot = root.append("g");

        const zoom = d3
            .zoom<SVGSVGElement, unknown>()
            .scaleExtent([0.35, 4])
            .on("zoom", (event) => {
                gRoot.attr("transform", event.transform.toString());
            });
        root.call(zoom);

        if (simNodes.length === 0) {
            gRoot
                .append("text")
                .attr("x", width / 2)
                .attr("y", height / 2)
                .attr("text-anchor", "middle")
                .attr("fill", "currentColor")
                .attr("opacity", 0.6)
                .text("No graph nodes yet");
            return;
        }

        const pathSet = new Set(pathNodeIds ?? []);
        const hubSet = new Set(highlightNodeIds ?? []);

        const louvain = louvainByNodeId;
        const clusterOf = (id: string) => louvain?.[id];
        const clusterIds = louvain
            ? Array.from(
                  new Set(simNodes.map((n) => clusterOf(n.id)).filter((c): c is number => typeof c === "number"))
              ).sort((a, b) => a - b)
            : [];
        const clusterTarget = new Map<number, { x: number; y: number }>();
        if (louvain && clusterIds.length > 0) {
            const nC = clusterIds.length;
            const ringR = Math.min(width, height) * 0.34;
            clusterIds.forEach((cid, i) => {
                const angle = (i / nC) * Math.PI * 2 - Math.PI / 2;
                clusterTarget.set(cid, {
                    x: width / 2 + ringR * Math.cos(angle),
                    y: height / 2 + ringR * Math.sin(angle),
                });
            });
        }

        const simulation = d3
            .forceSimulation<SimNode>(simNodes)
            .force(
                "link",
                d3
                    .forceLink<SimNode, SimLink>(simLinks)
                    .id((d) => d.id)
                    .distance(88)
                    .strength((d) => {
                        if (!louvain) {
                            return 0.35;
                        }
                        const sa = clusterOf((d.source as SimNode).id);
                        const sb = clusterOf((d.target as SimNode).id);
                        if (sa !== undefined && sb !== undefined && sa === sb) {
                            return 0.58;
                        }
                        return 0.26;
                    })
            )
            .force("charge", d3.forceManyBody<SimNode>().strength(-150))
            .force("center", d3.forceCenter(width / 2, height / 2))
            .force("collision", d3.forceCollide<SimNode>().radius(28));

        if (louvain && clusterIds.length > 0) {
            simulation
                .force(
                    "clusterX",
                    d3.forceX<SimNode>((d) => {
                        const c = clusterOf(d.id);
                        if (c === undefined) {
                            return width / 2;
                        }
                        return clusterTarget.get(c)?.x ?? width / 2;
                    }).strength(0.22)
                )
                .force(
                    "clusterY",
                    d3.forceY<SimNode>((d) => {
                        const c = clusterOf(d.id);
                        if (c === undefined) {
                            return height / 2;
                        }
                        return clusterTarget.get(c)?.y ?? height / 2;
                    }).strength(0.22)
                );
        }

        const link = gRoot
            .append("g")
            .attr("stroke", "currentColor")
            .attr("stroke-opacity", 0.35)
            .selectAll("line")
            .data(simLinks)
            .join("line")
            .attr("stroke-width", (d) => 1 + d.confidence * 2);

        const dragBehavior = d3
            .drag<SVGGElement, SimNode>()
            .on("start", (event, d) => {
                if (!event.active) {
                    simulation.alphaTarget(0.25).restart();
                }
                d.fx = d.x;
                d.fy = d.y;
            })
            .on("drag", (event, d) => {
                d.fx = event.x;
                d.fy = event.y;
            })
            .on("end", (event, d) => {
                if (!event.active) {
                    simulation.alphaTarget(0);
                }
                d.fx = null;
                d.fy = null;
            });

        const node = gRoot
            .append("g")
            .selectAll<SVGGElement, SimNode>("g")
            .data(simNodes)
            .join((enter) => enter.append("g"))
            .call(dragBehavior)
            .on("click", (_event, d) => {
                onNodeClick?.(d.raw);
            });

        const clusterHue = (id: string) => {
            const c = clusterOf(id);
            if (c === undefined || !louvain) {
                return null;
            }
            return ((c * 47) % 360 + 180) % 360;
        };

        node
            .append("circle")
            .attr("r", 14)
            .attr("fill", (d) => {
                const hue = clusterHue(d.id);
                if (hue !== null) {
                    return `hsla(${hue}, 42%, 46%, 0.95)`;
                }
                return d.nodeType === "Project"
                    ? "var(--accent, #6ea8fe)"
                    : d.nodeType === "Error"
                      ? "var(--danger, #f87171)"
                      : "var(--surface-2, #3f3f46)";
            })
            .attr("stroke", (d) => {
                if (d.id === selectedNodeId) {
                    return "var(--accent, #93c5fd)";
                }
                if (pathSet.has(d.id)) {
                    return "#fbbf24";
                }
                if (hubSet.has(d.id)) {
                    return "#a78bfa";
                }
                return "currentColor";
            })
            .attr("stroke-width", (d) => {
                if (d.id === selectedNodeId) {
                    return 3;
                }
                if (pathSet.has(d.id) || hubSet.has(d.id)) {
                    return 2.5;
                }
                return 1;
            });

        node
            .append("text")
            .attr("text-anchor", "middle")
            .attr("dy", 28)
            .attr("font-size", 10)
            .attr("fill", "currentColor")
            .text((d) => d.label);

        let ticks = 0;
        simulation.on("tick", () => {
            ticks += 1;
            link.attr("x1", (d) => (d.source as SimNode).x ?? 0)
                .attr("y1", (d) => (d.source as SimNode).y ?? 0)
                .attr("x2", (d) => (d.target as SimNode).x ?? 0)
                .attr("y2", (d) => (d.target as SimNode).y ?? 0);

            node.attr("transform", (d) => `translate(${d.x ?? 0},${d.y ?? 0})`);

            if (ticks >= maxSimulationTicks) {
                simulation.alphaTarget(0);
                simulation.stop();
            }
        });

        return () => {
            simulation.stop();
            simulation.on("tick", null);
            root.on(".zoom", null);
        };
    }, [simNodes, simLinks, height, onNodeClick, selectedNodeId, pathNodeIds, highlightNodeIds, maxSimulationTicks, louvainByNodeId]);

    return (
        <div className="knowledge-graph-wrap" style={{ height }}>
            <svg ref={ref} className="knowledge-graph-svg" width="100%" height={height} role="img" aria-label="Knowledge graph" />
        </div>
    );
}

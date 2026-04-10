import { useState, useEffect, useRef, useCallback } from "react";
import { getGraphData, GraphNodeData, GraphEdgeData } from "../api/tauri";
import "./GraphPanel.css";

interface GraphPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

interface SimNode extends GraphNodeData {
    x: number;
    y: number;
    vx: number;
    vy: number;
    fx?: number | null;
    fy?: number | null;
}

interface TooltipState {
    node: SimNode;
    x: number;
    y: number;
}

const NODE_COLORS: Record<string, string> = {
    MemoryChunk: "#60a5fa",
    Entity: "#a78bfa",
    Task: "#fb923c",
    Url: "#34d399",
};

const NODE_SIZES: Record<string, number> = {
    MemoryChunk: 5,
    Entity: 8,
    Task: 7,
    Url: 6,
};

export function GraphPanel({ isVisible, onClose }: GraphPanelProps) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const animFrameRef = useRef<number>(0);
    const [nodes, setNodes] = useState<SimNode[]>([]);
    const [edges, setEdges] = useState<GraphEdgeData[]>([]);
    const [loading, setLoading] = useState(true);
    const [tooltip, setTooltip] = useState<TooltipState | null>(null);
    const [activeFilters, setActiveFilters] = useState<Set<string>>(
        new Set(["MemoryChunk", "Entity", "Task", "Url"])
    );
    const [zoom, setZoom] = useState(1);
    const [pan, setPan] = useState({ x: 0, y: 0 });
    const dragRef = useRef<{
        isDragging: boolean;
        node: SimNode | null;
        startX: number;
        startY: number;
        isPanning: boolean;
    }>({ isDragging: false, node: null, startX: 0, startY: 0, isPanning: false });

    // Load graph data
    useEffect(() => {
        if (!isVisible) return;

        setLoading(true);
        getGraphData()
            .then((data) => {
                const centerX = window.innerWidth / 2;
                const centerY = window.innerHeight / 2;

                const simNodes: SimNode[] = data.nodes.map((n, i) => {
                    const angle = (i / Math.max(data.nodes.length, 1)) * Math.PI * 2;
                    const radius = 150 + Math.random() * 200;
                    return {
                        ...n,
                        x: centerX + Math.cos(angle) * radius,
                        y: centerY + Math.sin(angle) * radius,
                        vx: 0,
                        vy: 0,
                    };
                });

                setNodes(simNodes);
                setEdges(data.edges);
                setLoading(false);
            })
            .catch((err) => {
                console.error("Failed to load graph data:", err);
                setLoading(false);
            });
    }, [isVisible]);

    // Force simulation
    useEffect(() => {
        if (!isVisible || nodes.length === 0) return;

        let running = true;
        const nodeMap = new Map<string, SimNode>();
        nodes.forEach((n) => nodeMap.set(n.id, n));

        const simulate = () => {
            if (!running) return;

            const alpha = 0.3;
            const repulsion = 720;
            const attraction = 0.004;
            const damping = 0.85;
            const centerX = window.innerWidth / 2;
            const centerY = window.innerHeight / 2;

            // Repulsion between all nodes
            for (let i = 0; i < nodes.length; i++) {
                for (let j = i + 1; j < nodes.length; j++) {
                    const a = nodes[i];
                    const b = nodes[j];
                    if (!activeFilters.has(a.node_type) || !activeFilters.has(b.node_type))
                        continue;

                    const dx = a.x - b.x;
                    const dy = a.y - b.y;
                    const dist = Math.sqrt(dx * dx + dy * dy) || 1;
                    const force = (repulsion / (dist * dist)) * alpha;

                    if (a.fx == null) {
                        a.vx += (dx / dist) * force;
                        a.vy += (dy / dist) * force;
                    }
                    if (b.fx == null) {
                        b.vx -= (dx / dist) * force;
                        b.vy -= (dy / dist) * force;
                    }
                }
            }

            // Attraction along edges
            for (const edge of edges) {
                const source = nodeMap.get(edge.source);
                const target = nodeMap.get(edge.target);
                if (!source || !target) continue;
                if (
                    !activeFilters.has(source.node_type) ||
                    !activeFilters.has(target.node_type)
                )
                    continue;

                const dx = target.x - source.x;
                const dy = target.y - source.y;
                const dist = Math.sqrt(dx * dx + dy * dy) || 1;
                const force = dist * attraction * alpha;

                if (source.fx == null) {
                    source.vx += (dx / dist) * force;
                    source.vy += (dy / dist) * force;
                }
                if (target.fx == null) {
                    target.vx -= (dx / dist) * force;
                    target.vy -= (dy / dist) * force;
                }
            }

            // Center gravity
            for (const node of nodes) {
                if (!activeFilters.has(node.node_type)) continue;
                if (node.fx != null) continue;
                node.vx += (centerX - node.x) * 0.0005;
                node.vy += (centerY - node.y) * 0.0005;
            }

            // Apply velocity
            for (const node of nodes) {
                if (node.fx != null) {
                    node.x = node.fx;
                    node.y = node.fy!;
                    node.vx = 0;
                    node.vy = 0;
                } else {
                    node.vx *= damping;
                    node.vy *= damping;
                    node.x += node.vx;
                    node.y += node.vy;
                }
            }

            setNodes([...nodes]);
            animFrameRef.current = requestAnimationFrame(simulate);
        };

        animFrameRef.current = requestAnimationFrame(simulate);
        return () => {
            running = false;
            cancelAnimationFrame(animFrameRef.current);
        };
    }, [isVisible, nodes.length, edges.length, activeFilters]);

    // Canvas rendering
    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas || !isVisible) return;

        const ctx = canvas.getContext("2d");
        if (!ctx) return;

        canvas.width = window.innerWidth;
        canvas.height = window.innerHeight;

        const nodeMap = new Map<string, SimNode>();
        nodes.forEach((n) => nodeMap.set(n.id, n));

        ctx.clearRect(0, 0, canvas.width, canvas.height);
        ctx.save();
        ctx.translate(pan.x, pan.y);
        ctx.scale(zoom, zoom);

        // Draw edges
        ctx.globalAlpha = 0.1;
        for (const edge of edges) {
            const source = nodeMap.get(edge.source);
            const target = nodeMap.get(edge.target);
            if (!source || !target) continue;
            if (
                !activeFilters.has(source.node_type) ||
                !activeFilters.has(target.node_type)
            )
                continue;

            ctx.beginPath();
            ctx.moveTo(source.x, source.y);
            ctx.lineTo(target.x, target.y);
            ctx.strokeStyle = "rgba(140, 140, 148, 0.5)";
            ctx.lineWidth = 0.7;
            ctx.stroke();
        }

        // Draw nodes
        ctx.globalAlpha = 1;
        for (const node of nodes) {
            if (!activeFilters.has(node.node_type)) continue;

            const color = NODE_COLORS[node.node_type] || "#888";
            const size = NODE_SIZES[node.node_type] || 6;

            // Glow
            ctx.beginPath();
            ctx.arc(node.x, node.y, size + 4, 0, Math.PI * 2);
            ctx.fillStyle = color.replace(")", ", 0.08)").replace("rgb", "rgba");
            ctx.fill();

            // Node circle
            ctx.beginPath();
            ctx.arc(node.x, node.y, size, 0, Math.PI * 2);
            ctx.fillStyle = color;
            ctx.fill();

            // Labels only when intentionally zoomed in
            if (size >= 8 && zoom > 1.35) {
                ctx.fillStyle = "rgba(220, 220, 224, 0.75)";
                ctx.font = "10px -apple-system, system-ui, sans-serif";
                ctx.textAlign = "center";
                ctx.fillText(
                    node.label.length > 25
                        ? node.label.slice(0, 22) + "..."
                        : node.label,
                    node.x,
                    node.y + size + 14
                );
            }
        }

        ctx.restore();
    }, [nodes, edges, zoom, pan, activeFilters, isVisible]);

    // Mouse handlers
    const getNodeAt = useCallback(
        (mx: number, my: number): SimNode | null => {
            const worldX = (mx - pan.x) / zoom;
            const worldY = (my - pan.y) / zoom;

            for (const node of nodes) {
                if (!activeFilters.has(node.node_type)) continue;
                const size = NODE_SIZES[node.node_type] || 6;
                const dx = worldX - node.x;
                const dy = worldY - node.y;
                if (dx * dx + dy * dy < (size + 5) * (size + 5)) {
                    return node;
                }
            }
            return null;
        },
        [nodes, zoom, pan, activeFilters]
    );

    const handleMouseDown = useCallback(
        (e: React.MouseEvent) => {
            const node = getNodeAt(e.clientX, e.clientY);
            if (node) {
                dragRef.current = {
                    isDragging: true,
                    node,
                    startX: e.clientX,
                    startY: e.clientY,
                    isPanning: false,
                };
                node.fx = node.x;
                node.fy = node.y;
            } else {
                dragRef.current = {
                    isDragging: false,
                    node: null,
                    startX: e.clientX,
                    startY: e.clientY,
                    isPanning: true,
                };
            }
        },
        [getNodeAt]
    );

    const handleMouseMove = useCallback(
        (e: React.MouseEvent) => {
            if (dragRef.current.isDragging && dragRef.current.node) {
                const node = dragRef.current.node;
                node.fx = (e.clientX - pan.x) / zoom;
                node.fy = (e.clientY - pan.y) / zoom;
                setTooltip(null);
            } else if (dragRef.current.isPanning) {
                const dx = e.clientX - dragRef.current.startX;
                const dy = e.clientY - dragRef.current.startY;
                setPan((p) => ({ x: p.x + dx, y: p.y + dy }));
                dragRef.current.startX = e.clientX;
                dragRef.current.startY = e.clientY;
            } else {
                const node = getNodeAt(e.clientX, e.clientY);
                if (node) {
                    setTooltip({ node, x: e.clientX + 15, y: e.clientY + 15 });
                } else {
                    setTooltip(null);
                }
            }
        },
        [getNodeAt, zoom, pan]
    );

    const handleMouseUp = useCallback(() => {
        if (dragRef.current.node) {
            dragRef.current.node.fx = null;
            dragRef.current.node.fy = null;
        }
        dragRef.current = {
            isDragging: false,
            node: null,
            startX: 0,
            startY: 0,
            isPanning: false,
        };
    }, []);

    const handleWheel = useCallback((e: React.WheelEvent) => {
        e.preventDefault();
        const delta = e.deltaY > 0 ? 0.9 : 1.1;
        setZoom((z) => Math.min(Math.max(z * delta, 0.1), 5));
    }, []);

    const toggleFilter = (type: string) => {
        setActiveFilters((prev) => {
            const next = new Set(prev);
            if (next.has(type)) {
                next.delete(type);
            } else {
                next.add(type);
            }
            return next;
        });
    };

    if (!isVisible) return null;

    const filteredNodes = nodes.filter((n) => activeFilters.has(n.node_type));

    return (
        <div className="graph-panel">
            <div className="graph-header">
                <div className="graph-header-left">
                    <h2>Knowledge Graph</h2>
                    <div className="graph-stats">
                        <span>◉ {filteredNodes.length} nodes</span>
                        <span>─ {edges.length} edges</span>
                    </div>
                </div>
                <button className="ui-action-btn graph-close-btn" onClick={onClose}>
                    ✕ Close
                </button>
            </div>

            <div className="graph-filters">
                {(
                    [
                        ["MemoryChunk", "memory", "Memories"],
                        ["Entity", "entity", "Entities"],
                        ["Task", "task", "Tasks"],
                        ["Url", "url", "Links"],
                    ] as const
                ).map(([type, dotClass, label]) => (
                    <button
                        key={type}
                        className={`ui-action-btn graph-filter-btn ${activeFilters.has(type) ? "active" : ""}`}
                        onClick={() => toggleFilter(type)}
                    >
                        <span className={`graph-filter-dot dot-${dotClass}`} />
                        {label}
                    </button>
                ))}
            </div>

            <div className="graph-canvas-container">
                {loading ? (
                    <div className="graph-loading">
                        <div className="spinner" />
                        Loading graph data...
                    </div>
                ) : nodes.length === 0 ? (
                    <div className="graph-empty">
                        <p>
                            No graph data yet. Keep FNDR running to build your
                            knowledge graph from screen captures.
                        </p>
                    </div>
                ) : (
                    <>
                        <canvas
                            ref={canvasRef}
                            className="graph-canvas"
                            onMouseDown={handleMouseDown}
                            onMouseMove={handleMouseMove}
                            onMouseUp={handleMouseUp}
                            onMouseLeave={handleMouseUp}
                            onWheel={handleWheel}
                        />
                        <div className="graph-controls">
                            <button
                                className="ui-action-btn graph-control-btn"
                                onClick={() =>
                                    setZoom((z) => Math.min(z * 1.3, 5))
                                }
                            >
                                +
                            </button>
                            <button
                                className="ui-action-btn graph-control-btn"
                                onClick={() =>
                                    setZoom((z) => Math.max(z * 0.7, 0.1))
                                }
                            >
                                −
                            </button>
                            <button
                                className="ui-action-btn graph-control-btn"
                                onClick={() => {
                                    setZoom(1);
                                    setPan({ x: 0, y: 0 });
                                }}
                            >
                                ⟲
                            </button>
                        </div>
                    </>
                )}
            </div>

            {tooltip && (
                <div
                    className="graph-tooltip"
                    style={{ left: tooltip.x, top: tooltip.y }}
                >
                    <span
                        className={`tooltip-type tooltip-type-${tooltip.node.node_type}`}
                    >
                        {tooltip.node.node_type}
                    </span>
                    <h4>{tooltip.node.label}</h4>
                    <div className="tooltip-meta">
                        {new Date(tooltip.node.created_at).toLocaleString()}
                    </div>
                </div>
            )}
        </div>
    );
}

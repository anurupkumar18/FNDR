import { useState, useEffect, useRef, useCallback } from "react";
import { getGraphData, GraphNodeData, GraphEdgeData } from "../api/tauri";
import "./GraphPanel.css";

interface GraphPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type ClusterMode = "session" | "app" | "memoryType";

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

const CLUSTER_MODES: Array<{ key: ClusterMode; label: string }> = [
    { key: "session", label: "Session" },
    { key: "app", label: "App" },
    { key: "memoryType", label: "Memory Type" },
];

function hashCode(input: string): number {
    let hash = 0;
    for (let i = 0; i < input.length; i++) {
        hash = (hash << 5) - hash + input.charCodeAt(i);
        hash |= 0;
    }
    return Math.abs(hash);
}

function safeString(value: unknown): string {
    return typeof value === "string" ? value.trim() : "";
}

function classifyMemoryTypeFromApp(appName: string): string {
    const app = appName.toLowerCase();
    if (
        app.includes("safari") ||
        app.includes("chrome") ||
        app.includes("arc") ||
        app.includes("brave") ||
        app.includes("firefox") ||
        app.includes("edge")
    ) {
        return "web";
    }
    if (
        app.includes("code") ||
        app.includes("terminal") ||
        app.includes("xcode") ||
        app.includes("iterm")
    ) {
        return "development";
    }
    if (
        app.includes("meeting") ||
        app.includes("zoom") ||
        app.includes("teams")
    ) {
        return "meeting";
    }
    if (
        app.includes("mail") ||
        app.includes("slack") ||
        app.includes("messages") ||
        app.includes("discord")
    ) {
        return "communication";
    }
    if (
        app.includes("docs") ||
        app.includes("notion") ||
        app.includes("word") ||
        app.includes("pages") ||
        app.includes("preview") ||
        app.includes("pdf")
    ) {
        return "documents";
    }
    return "general";
}

function clusterKeyForNode(node: GraphNodeData, mode: ClusterMode): string {
    const metadata = node.metadata ?? {};

    if (mode === "session") {
        if (node.node_type === "MemoryChunk") {
            const sessionId = safeString(metadata.session_id);
            if (sessionId) {
                return `session:${sessionId}`;
            }
        }
        if (node.node_type === "Entity" && safeString(metadata.entity_type) === "session") {
            const sessionId = safeString(metadata.session_id);
            if (sessionId) {
                return `session:${sessionId}`;
            }
        }
        return node.node_type.toLowerCase();
    }

    if (mode === "app") {
        if (node.node_type === "MemoryChunk") {
            const appName = safeString(metadata.app_name);
            if (appName) {
                return `app:${appName}`;
            }
        }
        if (node.node_type === "Task") {
            const sourceApp = safeString(metadata.source_app);
            if (sourceApp) {
                return `app:${sourceApp}`;
            }
        }
        if (node.node_type === "Url") {
            const host = safeString(metadata.host);
            if (host) {
                return `site:${host}`;
            }
        }
        return node.node_type.toLowerCase();
    }

    if (node.node_type === "MemoryChunk") {
        const appName = safeString(metadata.app_name);
        const memoryType = safeString(metadata.memory_type);
        if (memoryType) {
            return `type:${memoryType}`;
        }
        return `type:${classifyMemoryTypeFromApp(appName)}`;
    }
    if (node.node_type === "Task") {
        return "type:task";
    }
    if (node.node_type === "Url") {
        return "type:web";
    }
    if (node.node_type === "Entity") {
        const entityType = safeString(metadata.entity_type);
        return entityType ? `type:${entityType}` : "type:entity";
    }
    return "type:other";
}

function buildClusterCenters(keys: string[], width: number, height: number): Map<string, { x: number; y: number }> {
    const unique = [...new Set(keys)];
    const centers = new Map<string, { x: number; y: number }>();
    const cx = width / 2;
    const cy = height / 2;

    if (unique.length <= 1) {
        if (unique.length === 1) {
            centers.set(unique[0], { x: cx, y: cy });
        }
        return centers;
    }

    const radius = Math.min(width, height) * 0.28;
    unique.forEach((key, index) => {
        const angle = (index / unique.length) * Math.PI * 2;
        centers.set(key, {
            x: cx + Math.cos(angle) * radius,
            y: cy + Math.sin(angle) * radius,
        });
    });

    return centers;
}

function layoutNodes(nodes: GraphNodeData[], clusterMode: ClusterMode): SimNode[] {
    const width = window.innerWidth;
    const height = window.innerHeight;
    const keys = nodes.map((node) => clusterKeyForNode(node, clusterMode));
    const centers = buildClusterCenters(keys, width, height);
    const perClusterCount = new Map<string, number>();

    return nodes.map((node) => {
        const key = clusterKeyForNode(node, clusterMode);
        const center = centers.get(key) ?? { x: width / 2, y: height / 2 };
        const ordinal = perClusterCount.get(key) ?? 0;
        perClusterCount.set(key, ordinal + 1);

        const seed = hashCode(`${node.id}:${key}:${ordinal}`);
        const angle = ((seed % 360) * Math.PI) / 180;
        const spread = 20 + (seed % 90);

        return {
            ...node,
            x: center.x + Math.cos(angle) * spread,
            y: center.y + Math.sin(angle) * spread,
            vx: 0,
            vy: 0,
            fx: null,
            fy: null,
        };
    });
}

export function GraphPanel({ isVisible, onClose }: GraphPanelProps) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const animFrameRef = useRef<number>(0);
    const [rawNodes, setRawNodes] = useState<GraphNodeData[]>([]);
    const [nodes, setNodes] = useState<SimNode[]>([]);
    const [edges, setEdges] = useState<GraphEdgeData[]>([]);
    const [loading, setLoading] = useState(true);
    const [tooltip, setTooltip] = useState<TooltipState | null>(null);
    const [activeFilters, setActiveFilters] = useState<Set<string>>(
        new Set(["MemoryChunk", "Entity", "Task", "Url"])
    );
    const [clusterMode, setClusterMode] = useState<ClusterMode>("session");
    const [zoom, setZoom] = useState(1);
    const [pan, setPan] = useState({ x: 0, y: 0 });
    const dragRef = useRef<{
        isDragging: boolean;
        node: SimNode | null;
        startX: number;
        startY: number;
        isPanning: boolean;
    }>({ isDragging: false, node: null, startX: 0, startY: 0, isPanning: false });

    useEffect(() => {
        if (!isVisible) {
            return;
        }

        setLoading(true);
        getGraphData()
            .then((data) => {
                setRawNodes(data.nodes);
                setNodes(layoutNodes(data.nodes, clusterMode));
                setEdges(data.edges);
                setLoading(false);
            })
            .catch((err) => {
                console.error("Failed to load graph data:", err);
                setLoading(false);
            });
    }, [isVisible]);

    useEffect(() => {
        if (!isVisible || rawNodes.length === 0) {
            return;
        }
        setNodes(layoutNodes(rawNodes, clusterMode));
        setPan({ x: 0, y: 0 });
        setZoom(1);
    }, [clusterMode, isVisible, rawNodes]);

    useEffect(() => {
        if (!isVisible || nodes.length === 0) {
            return;
        }

        let running = true;
        const nodeMap = new Map<string, SimNode>();
        nodes.forEach((node) => nodeMap.set(node.id, node));

        const centerX = window.innerWidth / 2;
        const centerY = window.innerHeight / 2;
        const clusterCenters = buildClusterCenters(
            nodes.map((node) => clusterKeyForNode(node, clusterMode)),
            window.innerWidth,
            window.innerHeight
        );

        const simulate = () => {
            if (!running) {
                return;
            }

            const alpha = 0.3;
            const repulsion = 720;
            const attraction = 0.004;
            const damping = 0.85;

            for (let i = 0; i < nodes.length; i++) {
                for (let j = i + 1; j < nodes.length; j++) {
                    const a = nodes[i];
                    const b = nodes[j];
                    if (!activeFilters.has(a.node_type) || !activeFilters.has(b.node_type)) {
                        continue;
                    }

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

            for (const edge of edges) {
                const source = nodeMap.get(edge.source);
                const target = nodeMap.get(edge.target);
                if (!source || !target) {
                    continue;
                }
                if (
                    !activeFilters.has(source.node_type) ||
                    !activeFilters.has(target.node_type)
                ) {
                    continue;
                }

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

            for (const node of nodes) {
                if (!activeFilters.has(node.node_type) || node.fx != null) {
                    continue;
                }

                const key = clusterKeyForNode(node, clusterMode);
                const anchor = clusterCenters.get(key) ?? { x: centerX, y: centerY };
                node.vx += (anchor.x - node.x) * 0.0018;
                node.vy += (anchor.y - node.y) * 0.0018;
            }

            for (const node of nodes) {
                if (node.fx != null) {
                    node.x = node.fx;
                    node.y = node.fy ?? node.y;
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
    }, [isVisible, nodes.length, edges.length, activeFilters, clusterMode]);

    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas || !isVisible) {
            return;
        }

        const ctx = canvas.getContext("2d");
        if (!ctx) {
            return;
        }

        canvas.width = window.innerWidth;
        canvas.height = window.innerHeight;

        const nodeMap = new Map<string, SimNode>();
        nodes.forEach((node) => nodeMap.set(node.id, node));

        ctx.clearRect(0, 0, canvas.width, canvas.height);
        ctx.save();
        ctx.translate(pan.x, pan.y);
        ctx.scale(zoom, zoom);

        ctx.globalAlpha = 0.1;
        for (const edge of edges) {
            const source = nodeMap.get(edge.source);
            const target = nodeMap.get(edge.target);
            if (!source || !target) {
                continue;
            }
            if (
                !activeFilters.has(source.node_type) ||
                !activeFilters.has(target.node_type)
            ) {
                continue;
            }

            ctx.beginPath();
            ctx.moveTo(source.x, source.y);
            ctx.lineTo(target.x, target.y);
            ctx.strokeStyle = "rgba(140, 140, 148, 0.5)";
            ctx.lineWidth = 0.7;
            ctx.stroke();
        }

        ctx.globalAlpha = 1;
        for (const node of nodes) {
            if (!activeFilters.has(node.node_type)) {
                continue;
            }

            const color = NODE_COLORS[node.node_type] || "#888";
            const size = NODE_SIZES[node.node_type] || 6;

            ctx.beginPath();
            ctx.arc(node.x, node.y, size + 4, 0, Math.PI * 2);
            ctx.fillStyle = "rgba(255, 255, 255, 0.06)";
            ctx.fill();

            ctx.beginPath();
            ctx.arc(node.x, node.y, size, 0, Math.PI * 2);
            ctx.fillStyle = color;
            ctx.fill();

            if (size >= 8 && zoom > 1.35) {
                ctx.fillStyle = "rgba(220, 220, 224, 0.75)";
                ctx.font = "10px -apple-system, system-ui, sans-serif";
                ctx.textAlign = "center";
                ctx.fillText(
                    node.label.length > 25
                        ? `${node.label.slice(0, 22)}...`
                        : node.label,
                    node.x,
                    node.y + size + 14
                );
            }
        }

        ctx.restore();
    }, [nodes, edges, zoom, pan, activeFilters, isVisible]);

    const getNodeAt = useCallback(
        (mx: number, my: number): SimNode | null => {
            const worldX = (mx - pan.x) / zoom;
            const worldY = (my - pan.y) / zoom;

            for (const node of nodes) {
                if (!activeFilters.has(node.node_type)) {
                    continue;
                }
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
        (event: React.MouseEvent) => {
            const node = getNodeAt(event.clientX, event.clientY);
            if (node) {
                dragRef.current = {
                    isDragging: true,
                    node,
                    startX: event.clientX,
                    startY: event.clientY,
                    isPanning: false,
                };
                node.fx = node.x;
                node.fy = node.y;
            } else {
                dragRef.current = {
                    isDragging: false,
                    node: null,
                    startX: event.clientX,
                    startY: event.clientY,
                    isPanning: true,
                };
            }
        },
        [getNodeAt]
    );

    const handleMouseMove = useCallback(
        (event: React.MouseEvent) => {
            if (dragRef.current.isDragging && dragRef.current.node) {
                const node = dragRef.current.node;
                node.fx = (event.clientX - pan.x) / zoom;
                node.fy = (event.clientY - pan.y) / zoom;
                setTooltip(null);
            } else if (dragRef.current.isPanning) {
                const dx = event.clientX - dragRef.current.startX;
                const dy = event.clientY - dragRef.current.startY;
                setPan((current) => ({ x: current.x + dx, y: current.y + dy }));
                dragRef.current.startX = event.clientX;
                dragRef.current.startY = event.clientY;
            } else {
                const node = getNodeAt(event.clientX, event.clientY);
                if (node) {
                    setTooltip({ node, x: event.clientX + 15, y: event.clientY + 15 });
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

    const handleWheel = useCallback((event: React.WheelEvent) => {
        event.preventDefault();
        const delta = event.deltaY > 0 ? 0.9 : 1.1;
        setZoom((current) => Math.min(Math.max(current * delta, 0.1), 5));
    }, []);

    const toggleFilter = (type: string) => {
        setActiveFilters((previous) => {
            const next = new Set(previous);
            if (next.has(type)) {
                next.delete(type);
            } else {
                next.add(type);
            }
            return next;
        });
    };

    if (!isVisible) {
        return null;
    }

    const filteredNodes = nodes.filter((node) => activeFilters.has(node.node_type));
    const filteredNodeIds = new Set(filteredNodes.map((node) => node.id));
    const filteredEdgesCount = edges.filter(
        (edge) => filteredNodeIds.has(edge.source) && filteredNodeIds.has(edge.target)
    ).length;

    return (
        <div className="graph-panel">
            <div className="graph-header">
                <div className="graph-header-left">
                    <h2>Knowledge Graph</h2>
                    <div className="graph-stats">
                        <span>◉ {filteredNodes.length} nodes</span>
                        <span>─ {filteredEdgesCount} edges</span>
                    </div>
                </div>
                <button className="ui-action-btn graph-close-btn" onClick={onClose}>
                    ✕ Close
                </button>
            </div>

            <div className="graph-cluster-modes">
                <span className="graph-cluster-label">Cluster by</span>
                {CLUSTER_MODES.map((mode) => (
                    <button
                        key={mode.key}
                        className={`ui-action-btn graph-mode-btn ${clusterMode === mode.key ? "active" : ""}`}
                        onClick={() => setClusterMode(mode.key)}
                    >
                        {mode.label}
                    </button>
                ))}
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
                                onClick={() => setZoom((current) => Math.min(current * 1.3, 5))}
                            >
                                +
                            </button>
                            <button
                                className="ui-action-btn graph-control-btn"
                                onClick={() => setZoom((current) => Math.max(current * 0.7, 0.1))}
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
                <div className="graph-tooltip" style={{ left: tooltip.x, top: tooltip.y }}>
                    <span className={`tooltip-type tooltip-type-${tooltip.node.node_type}`}>
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

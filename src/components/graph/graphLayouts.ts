import {
    ClusterInsight,
    Point,
    PositionedNode,
    TimelineSegment,
} from "./graphTypes";

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

function hashString(input: string): number {
    let hash = 0;
    for (let i = 0; i < input.length; i++) {
        hash = (hash << 5) - hash + input.charCodeAt(i);
        hash |= 0;
    }
    return Math.abs(hash);
}

export function radialPositions(
    keys: string[],
    width: number,
    height: number,
    options?: { radius?: number; startAngle?: number }
): Map<string, Point> {
    const unique = [...new Set(keys)];
    const centerX = width / 2;
    const centerY = height / 2;
    const radius = options?.radius ?? Math.min(width, height) * 0.36;
    const startAngle = options?.startAngle ?? -Math.PI / 2;
    const map = new Map<string, Point>();

    if (unique.length === 0) {
        return map;
    }

    if (unique.length === 1) {
        map.set(unique[0], { x: centerX, y: centerY });
        return map;
    }

    unique.forEach((key, index) => {
        const angle = startAngle + (index / unique.length) * Math.PI * 2;
        map.set(key, {
            x: centerX + Math.cos(angle) * radius,
            y: centerY + Math.sin(angle) * radius,
        });
    });

    return map;
}

export function curvedConnectionPath(source: Point, target: Point, bend = 0.18): string {
    const midX = (source.x + target.x) / 2;
    const midY = (source.y + target.y) / 2;
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const nx = -dy;
    const ny = dx;
    const distance = Math.hypot(dx, dy) || 1;
    const controlX = midX + (nx / distance) * distance * bend;
    const controlY = midY + (ny / distance) * distance * bend;
    return `M ${source.x} ${source.y} Q ${controlX} ${controlY} ${target.x} ${target.y}`;
}

function cross(o: Point, a: Point, b: Point): number {
    return (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
}

function convexHull(points: Point[]): Point[] {
    if (points.length <= 3) {
        return points;
    }

    const sorted = [...points].sort((a, b) => (a.x === b.x ? a.y - b.y : a.x - b.x));

    const lower: Point[] = [];
    for (const point of sorted) {
        while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], point) <= 0) {
            lower.pop();
        }
        lower.push(point);
    }

    const upper: Point[] = [];
    for (let i = sorted.length - 1; i >= 0; i--) {
        const point = sorted[i];
        while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], point) <= 0) {
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    return [...lower, ...upper];
}

function expandHull(points: Point[], padding: number): Point[] {
    if (points.length === 0) {
        return [];
    }

    const centerX = points.reduce((sum, point) => sum + point.x, 0) / points.length;
    const centerY = points.reduce((sum, point) => sum + point.y, 0) / points.length;

    return points.map((point) => {
        const dx = point.x - centerX;
        const dy = point.y - centerY;
        const distance = Math.hypot(dx, dy) || 1;
        return {
            x: point.x + (dx / distance) * padding,
            y: point.y + (dy / distance) * padding,
        };
    });
}

export function buildClusterHullPath(points: Point[], padding = 24): string {
    if (points.length === 0) {
        return "";
    }

    if (points.length === 1) {
        const point = points[0];
        const radius = padding;
        return `M ${point.x - radius} ${point.y} a ${radius} ${radius} 0 1 0 ${radius * 2} 0 a ${radius} ${radius} 0 1 0 ${-radius * 2} 0`;
    }

    const hull = expandHull(convexHull(points), padding);
    if (hull.length < 3) {
        return "";
    }

    let path = "";
    for (let i = 0; i < hull.length; i++) {
        const current = hull[i];
        const next = hull[(i + 1) % hull.length];
        const mid = {
            x: (current.x + next.x) / 2,
            y: (current.y + next.y) / 2,
        };

        if (i === 0) {
            path += `M ${mid.x} ${mid.y}`;
        }
        path += ` Q ${next.x} ${next.y} ${(next.x + hull[(i + 2) % hull.length].x) / 2} ${(next.y + hull[(i + 2) % hull.length].y) / 2}`;
    }

    return `${path} Z`;
}

export function layoutClusterIslands(
    clusters: ClusterInsight[],
    width: number,
    height: number
): Map<string, Point> {
    const positions = new Map<string, Point>();
    if (clusters.length === 0) {
        return positions;
    }

    const center = { x: width / 2, y: height / 2 };
    const dominant = clusters.find((cluster) => cluster.role === "dominant") ?? clusters[0];
    positions.set(dominant.key, center);

    const secondary = clusters.filter((cluster) => cluster.key !== dominant.key && cluster.role === "secondary");
    const bridges = clusters.filter((cluster) => cluster.role === "bridge");
    const peripherals = clusters.filter((cluster) => cluster.role === "peripheral");

    secondary.forEach((cluster, index) => {
        const angle = -Math.PI / 2 + (index / Math.max(secondary.length, 1)) * Math.PI * 2;
        const radius = Math.min(width, height) * 0.31;
        positions.set(cluster.key, {
            x: center.x + Math.cos(angle) * radius,
            y: center.y + Math.sin(angle) * (radius * 0.76),
        });
    });

    bridges.forEach((cluster, index) => {
        if (cluster.key === dominant.key) {
            return;
        }

        const hash = hashString(cluster.key);
        const angle = ((hash % 360) * Math.PI) / 180;
        const radius = Math.min(width, height) * 0.22 + (index % 2) * 18;
        positions.set(cluster.key, {
            x: center.x + Math.cos(angle) * radius,
            y: center.y + Math.sin(angle) * radius,
        });
    });

    peripherals.forEach((cluster, index) => {
        if (positions.has(cluster.key)) {
            return;
        }

        const hash = hashString(cluster.key);
        const angle = (((hash + index * 43) % 360) * Math.PI) / 180;
        const radius = Math.min(width, height) * 0.42;
        positions.set(cluster.key, {
            x: center.x + Math.cos(angle) * radius,
            y: center.y + Math.sin(angle) * (radius * 0.84),
        });
    });

    return positions;
}

export function layoutFocusRings(
    centerId: string,
    directIds: string[],
    secondaryIds: string[],
    width: number,
    height: number
): Map<string, { x: number; y: number; ring: 0 | 1 | 2 }> {
    const centerX = width / 2;
    const centerY = height / 2;
    const out = new Map<string, { x: number; y: number; ring: 0 | 1 | 2 }>();

    out.set(centerId, { x: centerX, y: centerY, ring: 0 });

    const firstRadiusX = Math.min(width, height) * 0.28;
    const firstRadiusY = Math.min(width, height) * 0.23;

    directIds.forEach((id, index) => {
        const angle = -Math.PI / 2 + (index / Math.max(directIds.length, 1)) * Math.PI * 2;
        out.set(id, {
            x: centerX + Math.cos(angle) * firstRadiusX,
            y: centerY + Math.sin(angle) * firstRadiusY,
            ring: 1,
        });
    });

    const secondRadiusX = Math.min(width, height) * 0.4;
    const secondRadiusY = Math.min(width, height) * 0.34;

    secondaryIds.forEach((id, index) => {
        const angle = -Math.PI / 2 + (index / Math.max(secondaryIds.length, 1)) * Math.PI * 2;
        out.set(id, {
            x: centerX + Math.cos(angle) * secondRadiusX,
            y: centerY + Math.sin(angle) * secondRadiusY,
            ring: 2,
        });
    });

    return out;
}

export function buildTimelineWaveform(
    segments: TimelineSegment[],
    width: number,
    height: number
): {
    points: Array<{ x: number; y: number; intensity: number; id: string }>;
    linePath: string;
    areaPath: string;
} {
    if (segments.length === 0) {
        return { points: [], linePath: "", areaPath: "" };
    }

    const horizontalPadding = 34;
    const usableWidth = width - horizontalPadding * 2;
    const baseline = height * 0.74;
    const amplitude = height * 0.42;

    const points = segments.map((segment, index) => {
        const x = horizontalPadding + (index / Math.max(segments.length - 1, 1)) * usableWidth;
        const shapedIntensity = clamp(segment.intensity * 0.7 + segment.confidence * 0.3, 0, 1);
        const y = baseline - shapedIntensity * amplitude;
        return { x, y, intensity: shapedIntensity, id: segment.id };
    });

    const linePath = points
        .map((point, index) => {
            if (index === 0) {
                return `M ${point.x} ${point.y}`;
            }
            const prev = points[index - 1];
            const controlX = (prev.x + point.x) / 2;
            return `Q ${controlX} ${prev.y} ${point.x} ${point.y}`;
        })
        .join(" ");

    const areaPath = `${linePath} L ${points[points.length - 1].x} ${height - 10} L ${points[0].x} ${height - 10} Z`;

    return {
        points,
        linePath,
        areaPath,
    };
}

export function buildConstellationLayout(
    nodes: Array<{ id: string; cluster: string; importance: number; roleWeight: number }>,
    clusterCenters: Map<string, Point>
): Map<string, PositionedNode> {
    const grouped = new Map<string, Array<{ id: string; importance: number; roleWeight: number }>>();

    nodes.forEach((node) => {
        if (!grouped.has(node.cluster)) {
            grouped.set(node.cluster, []);
        }
        grouped.get(node.cluster)?.push({ id: node.id, importance: node.importance, roleWeight: node.roleWeight });
    });

    const out = new Map<string, PositionedNode>();

    grouped.forEach((clusterNodes, clusterKey) => {
        const center = clusterCenters.get(clusterKey) ?? { x: 490, y: 215 };
        clusterNodes
            .sort((a, b) => b.importance - a.importance)
            .forEach((node, index) => {
                const angle = (index / Math.max(clusterNodes.length, 1)) * Math.PI * 2;
                const ring = Math.floor(index / 7);
                const distance = 16 + ring * 17 + node.roleWeight * 6;
                const jitter = ((index % 4) - 1.5) * 2.6;

                out.set(node.id, {
                    id: node.id,
                    x: center.x + Math.cos(angle) * (distance + jitter),
                    y: center.y + Math.sin(angle) * (distance + jitter),
                });
            });
    });

    return out;
}

export function layoutJourneyPath(
    pathIds: string[],
    adjacency: Map<string, Set<string>>,
    width: number,
    height: number
): {
    points: Array<{ id: string; x: number; y: number; branch: boolean }>;
    edges: Array<{ source: string; target: string; branch: boolean }>;
    branchCandidatesByPathId: Map<string, string[]>;
} {
    if (pathIds.length === 0) {
        return { points: [], edges: [], branchCandidatesByPathId: new Map() };
    }

    const horizontalPadding = 62;
    const usableWidth = width - horizontalPadding * 2;
    const spacing = pathIds.length > 1 ? usableWidth / (pathIds.length - 1) : 0;

    const points = pathIds.map((id, index) => ({
        id,
        x: horizontalPadding + index * spacing,
        y: height * 0.52 + Math.sin(index * 0.95) * 24,
        branch: false,
    }));

    const edges: Array<{ source: string; target: string; branch: boolean }> = [];
    for (let i = 0; i < pathIds.length - 1; i++) {
        edges.push({ source: pathIds[i], target: pathIds[i + 1], branch: false });
    }

    const pathSet = new Set(pathIds);
    const branchCandidatesByPathId = new Map<string, string[]>();

    pathIds.forEach((pathId, index) => {
        const candidates = [...(adjacency.get(pathId) ?? new Set())].filter((candidate) => !pathSet.has(candidate));
        if (candidates.length === 0) {
            return;
        }

        branchCandidatesByPathId.set(pathId, candidates);
        const branchId = candidates[0];
        const sourcePoint = points.find((point) => point.id === pathId);
        if (!sourcePoint) {
            return;
        }

        const direction = index % 2 === 0 ? -1 : 1;
        points.push({
            id: branchId,
            x: sourcePoint.x + direction * 16,
            y: sourcePoint.y + direction * 60,
            branch: true,
        });
        edges.push({ source: pathId, target: branchId, branch: true });
    });

    return { points, edges, branchCandidatesByPathId };
}

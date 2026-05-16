import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import type { InsightGraphEdge, InsightGraphNode } from "@/shared/ipc/tauri";
import { KnowledgeGraph } from "../KnowledgeGraph";

function mkNode(id: string, label = id): InsightGraphNode {
    return {
        id,
        node_type: "Concept",
        label,
        confidence: 1,
        source_memory_ids: [],
        embedding: null,
        created_at: "2026-05-16T00:00:00Z",
        updated_at: "2026-05-16T00:00:00Z",
        stale: false,
        metadata: {},
    };
}
function mkEdge(id: string, s: string, t: string): InsightGraphEdge {
    return {
        id,
        source_id: s,
        target_id: t,
        edge_type: "PartOf",
        confidence: 0.9,
        conflict_flag: false,
        created_at: "x",
        metadata: {},
    };
}

describe("KnowledgeGraph neighborhood highlighting", () => {
    it("marks selected, neighbor, and dimmed nodes when selectedNodeId is set", async () => {
        const nodes = [mkNode("a"), mkNode("b"), mkNode("c")];
        const edges = [mkEdge("e1", "a", "b")];

        const { container } = render(
            <KnowledgeGraph
                nodes={nodes}
                edges={edges}
                showSidePanel={false}
                height={400}
                selectedNodeId="a"
            />,
        );

        // Allow the simulation to flush its first tick and the data-state effect to run.
        await new Promise((resolve) => setTimeout(resolve, 50));

        const aGroup = container.querySelector<SVGGElement>('g.kg-node[data-node-id="a"]');
        const bGroup = container.querySelector<SVGGElement>('g.kg-node[data-node-id="b"]');
        const cGroup = container.querySelector<SVGGElement>('g.kg-node[data-node-id="c"]');
        expect(aGroup).not.toBeNull();
        expect(bGroup).not.toBeNull();
        expect(cGroup).not.toBeNull();

        expect(aGroup!.getAttribute("data-state")).toBe("selected");
        expect(bGroup!.getAttribute("data-state")).toBe("neighbor");
        expect(cGroup!.getAttribute("data-state")).toBe("dimmed");
    });

    it("renders the empty state when there are no nodes", () => {
        const { container } = render(
            <KnowledgeGraph nodes={[]} edges={[]} showSidePanel={false} height={300} />,
        );
        const empty = container.querySelector("text.kg-empty");
        expect(empty?.textContent).toMatch(/nothing to develop yet/);
    });

    it("renders nodes for every InsightGraphNode passed in", async () => {
        const nodes = [mkNode("a"), mkNode("b"), mkNode("c")];
        const edges = [mkEdge("e1", "a", "b"), mkEdge("e2", "b", "c")];

        const { container } = render(
            <KnowledgeGraph nodes={nodes} edges={edges} showSidePanel={false} height={400} />,
        );
        await new Promise((resolve) => setTimeout(resolve, 50));

        expect(container.querySelectorAll("g.kg-node")).toHaveLength(3);
        expect(container.querySelectorAll("line.kg-edge")).toHaveLength(2);
    });
});

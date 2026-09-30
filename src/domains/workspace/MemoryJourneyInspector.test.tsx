import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
    invoke: vi.fn(),
    eventHandler: null as ((payload: unknown) => void) | null,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (_event: string, handler: (payload: unknown) => void) => {
        mocks.eventHandler = handler;
    },
}));

import MemoryJourneyInspector from "./MemoryJourneyInspector";

const manifest = {
    schema_version: 1,
    journey_id: "journey-1",
    label: "Research article",
    mode: "live",
    state: "complete",
    created_at_ms: 1,
    updated_at_ms: 2,
    memory_id: "memory-1",
    stages: [
        {
            name: "ocr",
            status: "observed",
            observed_at_ms: 1,
            duration_ms: 18,
            outcome: "recognized",
            details: { block_count: 4 },
            artifact_ids: ["artifact-1"],
        },
        {
            name: "frame",
            status: "unavailable",
            observed_at_ms: 1,
            duration_ms: null,
            outcome: "not_persisted",
            details: null,
            artifact_ids: [],
        },
    ],
    artifacts: [
        {
            id: "artifact-1",
            stage: "ocr",
            relative_path: "artifacts/recognized.txt",
            sha256: "a".repeat(64),
            size_bytes: 24,
            available: true,
        },
    ],
    query_runs: [],
    pipeline_integrity: { privacy_outcome: "allowed", storage_integrity_valid: true },
    human_usefulness: { exact_findable: null, paraphrase_findable: null },
    agent_grounding: { evidence_complete: null, correct_refusal: null },
};

const status = {
    armed: false,
    active_journey_id: null,
    active_state: null,
    journeys: [
        {
            journey_id: "journey-1",
            label: "Research article",
            mode: "live",
            state: "complete",
            memory_id: "memory-1",
            stage_count: 2,
            artifact_count: 1,
            query_count: 0,
            size_bytes: 1200,
        },
    ],
    manifests: [manifest],
    total_bytes: 1200,
    max_bundles: 6,
    max_age_ms: 86_400_000,
    max_total_bytes: 134_217_728,
};

describe("MemoryJourneyInspector", () => {
    afterEach(() => {
        cleanup();
        mocks.invoke.mockReset();
        mocks.eventHandler = null;
    });

    it("renders only backend-observed stage state and marks unavailable evidence", async () => {
        mocks.invoke.mockResolvedValue(status);
        render(<MemoryJourneyInspector />);

        await screen.findByText("Research article · live · complete");
        expect(screen.getByText("observed · recognized · 18 ms")).toBeInTheDocument();
        expect(screen.getByText("unavailable · not_persisted")).toBeInTheDocument();
        expect(screen.getByText("1 artifact")).toBeInTheDocument();
        expect(screen.getByText("artifacts/recognized.txt")).toBeInTheDocument();
        expect(screen.getByText(/24 B · sha256 aaaaaaaa/i)).toBeInTheDocument();
        expect(screen.getByText(/not model chain-of-thought/i)).toBeInTheDocument();
    });

    it("arms through IPC and advances only when an event-backed status arrives", async () => {
        const armedStatus = { ...status, active_journey_id: "armed-1", active_state: "armed", armed: true, journeys: [], manifests: [] };
        mocks.invoke.mockImplementation((command: string) => {
            if (command === "arm_memory_journey") return Promise.resolve(armedStatus);
            return Promise.resolve({ ...status, journeys: [], manifests: [] });
        });
        render(<MemoryJourneyInspector />);

        fireEvent.click(await screen.findByRole("button", { name: "Record next capture" }));
        await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("arm_memory_journey", { label: "" }));
        expect(screen.getByRole("status")).toHaveTextContent("armed");

        mocks.eventHandler?.({ ...status, active_journey_id: "armed-1", active_state: "capturing", armed: false });
        await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("capturing"));
    });

    it("runs Search with the selected journey rather than a parallel mock path", async () => {
        mocks.invoke.mockImplementation((command: string) => {
            if (command === "run_memory_journey_query") {
                return Promise.resolve({ manifest, cards: [], answer: null });
            }
            return Promise.resolve(status);
        });
        render(<MemoryJourneyInspector />);
        await screen.findByText("Research article · live · complete");

        fireEvent.change(screen.getByLabelText(/exact query/i), { target: { value: "distinctive fact" } });
        fireEvent.click(screen.getByRole("button", { name: "Run Search" }));

        await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("run_memory_journey_query", {
            journeyId: "journey-1",
            query: "distinctive fact",
            path: "search",
            queryKind: "exact",
        }));
    });
});

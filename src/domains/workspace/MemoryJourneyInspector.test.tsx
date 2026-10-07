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
    arm_ready_at_ms: null,
    handoff_grace_ms: 8_000,
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
        expect(screen.getByText(/stages without durable run evidence stay marked unavailable/i)).toBeInTheDocument();
    });

    it("arms through IPC and advances only when an event-backed status arrives", async () => {
        const armedStatus = {
            ...status,
            active_journey_id: "armed-1",
            active_state: "armed",
            arm_ready_at_ms: 9_000,
            armed: true,
            journeys: [],
            manifests: [],
        };
        mocks.invoke.mockImplementation((command: string) => {
            if (command === "arm_memory_journey") return Promise.resolve(armedStatus);
            return Promise.resolve({ ...status, journeys: [], manifests: [] });
        });
        render(<MemoryJourneyInspector />);

        fireEvent.click(await screen.findByRole("button", { name: "Record next capture" }));
        await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("arm_memory_journey", { label: "" }));
        expect(screen.getByRole("status")).toHaveTextContent("armed");
        expect(screen.getByText(/8-second handoff/i)).toBeInTheDocument();

        mocks.eventHandler?.({ ...status, active_journey_id: "armed-1", active_state: "capturing", armed: false });
        await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("capturing"));
        expect(screen.queryByText(/8-second handoff/i)).not.toBeInTheDocument();
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

    it("imports a positive fixture and runs its exact, paraphrase, grounded, and unsupported cases serially", async () => {
        const evaluation = {
            fixture_id: "chat_mock-01",
            required_facts: ["Priya finished the export bug fix and is opening a PR."],
            exact_query: "export bug fix PR",
            paraphrase_query: "what happened to the export issue",
            grounded_question: "What did Priya finish?",
            unsupported_question: "What file size caused the failure?",
        };
        const fixtureManifest = { ...manifest, journey_id: "fixture-journey", memory_id: "fixture-memory" };
        const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
        mocks.invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
            calls.push({ command, args });
            if (command === "get_quality_lab_fixtures") {
                return Promise.resolve([{
                    id: "chat_mock-01",
                    app_class: "chat",
                    file_name: "chat_mock-01.png",
                    expected_text: "synthetic expected OCR",
                    cer_budget: 0.05,
                    evaluation,
                }]);
            }
            if (command === "replay_quality_lab_fixture") {
                return Promise.resolve({ fixture_id: "chat_mock-01", memory_id: "fixture-memory" });
            }
            if (command === "create_reconstructed_memory_journey") return Promise.resolve(fixtureManifest);
            if (command === "run_memory_journey_query") {
                const queryKind = args?.queryKind as string;
                const queryRun = {
                    id: `run-${queryKind}`,
                    path: args?.path,
                    kind: queryKind,
                    query: args?.query,
                    duration_ms: 12,
                    result_ids: queryKind === "unsupported" ? [] : ["fixture-memory"],
                    citation_ids: queryKind === "grounded" ? ["fixture-memory"] : [],
                    refusal: queryKind === "unsupported",
                };
                return Promise.resolve({
                    manifest: { ...fixtureManifest, query_runs: [queryRun] },
                    cards: [],
                    answer: queryKind === "unsupported" ? null : "Supported answer from the fixture.",
                });
            }
            return Promise.resolve(status);
        });
        render(<MemoryJourneyInspector />);

        fireEvent.click(await screen.findByRole("button", { name: "Import + run 4 checks" }));

        await screen.findByText("unsupported · ask");
        expect(screen.getByText(/Checks run 4\/4 · 4 passed · 0 not run/)).toBeInTheDocument();
        expect(screen.getAllByText("Target memory rank 1 · Pass")).toHaveLength(2);
        expect(screen.getByText("Target memory rank 1 · target cited · Pass")).toBeInTheDocument();
        expect(screen.getByText("Target memory not returned · refused · Pass")).toBeInTheDocument();
        const queryCalls = calls.filter((call) => call.command === "run_memory_journey_query");
        expect(queryCalls.map((call) => [call.args?.queryKind, call.args?.path, call.args?.query])).toEqual([
            ["exact", "search", evaluation.exact_query],
            ["paraphrase", "search", evaluation.paraphrase_query],
            ["grounded", "ask", evaluation.grounded_question],
            ["unsupported", "ask", evaluation.unsupported_question],
        ]);
        expect(calls.findIndex((call) => call.command === "replay_quality_lab_fixture"))
            .toBeLessThan(calls.findIndex((call) => call.command === "create_reconstructed_memory_journey"));
    });

    it("reports how many fixture checks were not run after an evaluation error", async () => {
        const evaluation = {
            fixture_id: "chat_mock-01",
            required_facts: ["Priya finished the export bug fix and is opening a PR."],
            exact_query: "export bug fix PR",
            paraphrase_query: "what happened to the export issue",
            grounded_question: "What did Priya finish?",
            unsupported_question: "What file size caused the failure?",
        };
        const fixtureManifest = { ...manifest, journey_id: "fixture-journey", memory_id: "fixture-memory" };
        mocks.invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
            if (command === "get_quality_lab_fixtures") {
                return Promise.resolve([{
                    id: "chat_mock-01",
                    app_class: "chat",
                    file_name: "chat_mock-01.png",
                    expected_text: "synthetic expected OCR",
                    cer_budget: 0.05,
                    evaluation,
                }]);
            }
            if (command === "replay_quality_lab_fixture") {
                return Promise.resolve({ fixture_id: "chat_mock-01", memory_id: "fixture-memory" });
            }
            if (command === "create_reconstructed_memory_journey") return Promise.resolve(fixtureManifest);
            if (command === "run_memory_journey_query") {
                if (args?.queryKind === "grounded") return Promise.reject(new Error("Ask unavailable"));
                const queryRun = {
                    id: `run-${String(args?.queryKind)}`,
                    path: args?.path,
                    kind: args?.queryKind,
                    query: args?.query,
                    duration_ms: 12,
                    result_ids: ["fixture-memory"],
                    citation_ids: [],
                    refusal: false,
                };
                return Promise.resolve({ manifest: { ...fixtureManifest, query_runs: [queryRun] }, cards: [], answer: null });
            }
            return Promise.resolve(status);
        });
        render(<MemoryJourneyInspector />);

        fireEvent.click(await screen.findByRole("button", { name: "Import + run 4 checks" }));

        const progress = await screen.findByText(/Exact\/paraphrase require target rank/);
        expect(progress.closest("p")).toHaveTextContent("Checks run 2/4 · 2 passed · 2 not run.");
        expect(screen.getByRole("alert")).toHaveTextContent("Ask unavailable");
    });

    it("runs all gold fixtures serially and reports per-fixture completion", async () => {
        const makeFixture = (id: string) => ({
            id,
            app_class: "browser",
            file_name: `${id}.png`,
            expected_text: `Expected text for ${id}`,
            cer_budget: 0.1,
            evaluation: {
                fixture_id: id,
                required_facts: [`Fact for ${id}`],
                exact_query: `exact ${id}`,
                paraphrase_query: `paraphrase ${id}`,
                grounded_question: `grounded ${id}`,
                unsupported_question: `unsupported ${id}`,
            },
        });
        const fixtures = [makeFixture("case-one"), makeFixture("case-two")];
        const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
        let activeFixtureId = "";
        mocks.invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
            calls.push({ command, args });
            if (command === "get_quality_lab_fixtures") return Promise.resolve(fixtures);
            if (command === "replay_quality_lab_fixture") {
                activeFixtureId = String(args?.fixtureId);
                return Promise.resolve({ fixture_id: activeFixtureId, memory_id: `memory-${activeFixtureId}` });
            }
            if (command === "create_reconstructed_memory_journey") {
                return Promise.resolve({ ...manifest, journey_id: `journey-${activeFixtureId}`, memory_id: `memory-${activeFixtureId}` });
            }
            if (command === "run_memory_journey_query") {
                const queryKind = String(args?.queryKind);
                const memoryId = `memory-${activeFixtureId}`;
                const queryRun = {
                    id: `run-${activeFixtureId}-${queryKind}`,
                    path: args?.path,
                    kind: queryKind,
                    query: args?.query,
                    duration_ms: 12,
                    result_ids: queryKind === "unsupported" ? [] : [memoryId],
                    citation_ids: queryKind === "grounded" ? [memoryId] : [],
                    refusal: queryKind === "unsupported",
                };
                return Promise.resolve({ manifest: { ...manifest, query_runs: [queryRun] }, cards: [], answer: "Supported answer" });
            }
            return Promise.resolve(status);
        });
        render(<MemoryJourneyInspector />);

        fireEvent.click(await screen.findByRole("button", { name: "Run all 2 gold cases" }));

        await screen.findByText(/Gold set 2\/2 fixtures finished/);
        expect(screen.getByText("case-one: 4/4 checks passed")).toBeInTheDocument();
        expect(screen.getByText("case-two: 4/4 checks passed")).toBeInTheDocument();
        expect(screen.getByText(/8 checks passed; 0 checks need review; 0 checks not run; 0 fixture errors/)).toBeInTheDocument();
        const fixtureStages = calls
            .filter((call) => ["replay_quality_lab_fixture", "create_reconstructed_memory_journey", "run_memory_journey_query"].includes(call.command))
            .map((call) => call.command === "run_memory_journey_query" ? `query:${call.args?.query}` : call.command);
        expect(fixtureStages).toEqual([
            "replay_quality_lab_fixture", "create_reconstructed_memory_journey",
            "query:exact case-one", "query:paraphrase case-one", "query:grounded case-one", "query:unsupported case-one",
            "replay_quality_lab_fixture", "create_reconstructed_memory_journey",
            "query:exact case-two", "query:paraphrase case-two", "query:grounded case-two", "query:unsupported case-two",
        ]);
    });

    it("continues the gold batch after a fixture query fails and counts its unrun checks", async () => {
        const fixtures = ["case-one", "case-two"].map((id) => ({
            id,
            app_class: "browser",
            file_name: `${id}.png`,
            expected_text: `Expected text for ${id}`,
            cer_budget: 0.1,
            evaluation: {
                fixture_id: id,
                required_facts: [`Fact for ${id}`],
                exact_query: `exact ${id}`,
                paraphrase_query: `paraphrase ${id}`,
                grounded_question: `grounded ${id}`,
                unsupported_question: `unsupported ${id}`,
            },
        }));
        let activeFixtureId = "";
        mocks.invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
            if (command === "get_quality_lab_fixtures") return Promise.resolve(fixtures);
            if (command === "replay_quality_lab_fixture") {
                activeFixtureId = String(args?.fixtureId);
                return Promise.resolve({ fixture_id: activeFixtureId, memory_id: `memory-${activeFixtureId}` });
            }
            if (command === "create_reconstructed_memory_journey") {
                return Promise.resolve({ ...manifest, journey_id: `journey-${activeFixtureId}`, memory_id: `memory-${activeFixtureId}` });
            }
            if (command === "run_memory_journey_query") {
                if (activeFixtureId === "case-one" && args?.queryKind === "grounded") {
                    return Promise.reject(new Error("Ask failed"));
                }
                const queryKind = String(args?.queryKind);
                const memoryId = `memory-${activeFixtureId}`;
                const queryRun = {
                    id: `run-${activeFixtureId}-${queryKind}`,
                    path: args?.path,
                    kind: queryKind,
                    query: args?.query,
                    duration_ms: 12,
                    result_ids: queryKind === "unsupported" ? [] : [memoryId],
                    citation_ids: queryKind === "grounded" ? [memoryId] : [],
                    refusal: queryKind === "unsupported",
                };
                return Promise.resolve({ manifest: { ...manifest, query_runs: [queryRun] }, cards: [], answer: "Supported answer" });
            }
            return Promise.resolve(status);
        });
        render(<MemoryJourneyInspector />);

        fireEvent.click(await screen.findByRole("button", { name: "Run all 2 gold cases" }));

        await screen.findByText(/Gold set 2\/2 fixtures finished/);
        expect(screen.getByText(/6 checks passed; 0 checks need review; 2 checks not run; 1 fixture errors/)).toBeInTheDocument();
        expect(screen.getByText("case-one: failed · Error: Ask failed")).toBeInTheDocument();
        expect(screen.getByText("case-two: 4/4 checks passed")).toBeInTheDocument();
    });
});

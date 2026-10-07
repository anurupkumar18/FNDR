import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { useSearch } from "./useSearch";
import { searchMemoryCards, type MemoryCard } from "@/shared/ipc/tauri";
import { SEARCH_LIMITS } from "@/shared/utils/config";
import type { ActivityTraceSnapshot } from "@/shared/activity/activityTrace";

vi.mock("@/shared/ipc/tauri", () => ({
    searchMemoryCards: vi.fn(),
}));

const QUERY_SECRET = "acquisition-codename-nightingale";
const RAW_ERROR = "ENOENT /Users/alex/Library/secret-plan.db at https://internal.example/q?token=abc";

let latest: ActivityTraceSnapshot | null = null;

function Harness({ query }: { query: string }) {
    latest = useSearch(query, null, null).activityTrace;
    return null;
}

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function card(): MemoryCard {
    return {
        id: "memory-1",
        title: "Confidential board deck",
        summary: "Summary text",
        action: "",
        context: [],
        timestamp: 1,
        app_name: "Notes",
        window_title: "board-deck.key",
        score: 0.9,
        source_count: 1,
        raw_snippets: ["snippet body"],
        matched_routes: ["Vector"],
    };
}

function snapshot(): Array<[string, string]> {
    return (latest?.steps ?? []).map((step) => [step.id, step.status]);
}

function traceText(): string {
    return JSON.stringify(latest);
}

async function advance(ms: number) {
    await act(async () => {
        await vi.advanceTimersByTimeAsync(ms);
    });
}

beforeEach(() => {
    vi.useFakeTimers();
    latest = null;
});

afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
});

describe("useSearch activity boundaries", () => {
    it("keeps the debounce step waiting and starts no retrieval until typing settles", async () => {
        render(<Harness query={QUERY_SECRET} />);

        expect(snapshot()).toEqual([["queued", "waiting"]]);
        expect(searchMemoryCards).not.toHaveBeenCalled();

        await advance(SEARCH_LIMITS.typingDebounceMs - 1);
        expect(snapshot()).toEqual([["queued", "waiting"]]);

        vi.mocked(searchMemoryCards).mockReturnValue(deferred<MemoryCard[]>().promise);
        await advance(1);
        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "running"]]);
    });

    it("keeps retrieval running for as long as the backend call is pending", async () => {
        const request = deferred<MemoryCard[]>();
        vi.mocked(searchMemoryCards).mockReturnValue(request.promise);
        render(<Harness query={QUERY_SECRET} />);

        await advance(SEARCH_LIMITS.typingDebounceMs);
        await advance(2_000);

        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "running"]]);
        expect(latest?.status).toBe("running");
        expect(latest?.finishedAtMs).toBeNull();

        await act(async () => {
            request.resolve([card()]);
        });

        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "completed"], ["result", "completed"]]);
        expect(latest?.status).toBe("completed");
    });

    it("fails the same retrieval step when the backend rejects, without raw error text", async () => {
        const request = deferred<MemoryCard[]>();
        vi.mocked(searchMemoryCards).mockReturnValue(request.promise);
        render(<Harness query={QUERY_SECRET} />);
        await advance(SEARCH_LIMITS.typingDebounceMs);

        await act(async () => {
            request.reject(new Error(RAW_ERROR));
        });

        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "failed"]]);
        const step = latest?.steps.find((candidate) => candidate.id === "retrieval");
        expect(step?.label).toBe("Search failed");
        expect(step?.detail).toBe("Backend request failed");
        expect(step?.evidence).toBe("ipc-boundary");
        expect(traceText()).not.toContain("ENOENT");
        expect(traceText()).not.toContain("internal.example");
    });

    it("reports a backend error that mentions a timeout as a backend failure, not a client timeout", async () => {
        const request = deferred<MemoryCard[]>();
        vi.mocked(searchMemoryCards).mockReturnValue(request.promise);
        render(<Harness query={QUERY_SECRET} />);
        await advance(SEARCH_LIMITS.typingDebounceMs);

        await act(async () => {
            request.reject(new Error("lance connection timed out while opening table"));
        });

        const step = latest?.steps.find((candidate) => candidate.id === "retrieval");
        expect(step?.status).toBe("failed");
        expect(step?.label).toBe("Search failed");
        expect(step?.detail).toBe("Backend request failed");
        expect(step?.evidence).toBe("ipc-boundary");
    });

    it("times out the same retrieval step from the renderer timer and names it a frontend event", async () => {
        vi.mocked(searchMemoryCards).mockReturnValue(deferred<MemoryCard[]>().promise);
        render(<Harness query={QUERY_SECRET} />);
        await advance(SEARCH_LIMITS.typingDebounceMs);
        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "running"]]);

        await advance(SEARCH_LIMITS.baseTimeoutMs + 2 * SEARCH_LIMITS.timeoutBonusCapMs);

        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "failed"]]);
        const step = latest?.steps.find((candidate) => candidate.id === "retrieval");
        expect(step?.label).toBe("Search timed out");
        expect(step?.detail).toBe("Client timeout");
        // The backend never answered, so this is a renderer timer, not an IPC result.
        expect(step?.evidence).toBe("frontend-event");
        expect(step?.actor).toBe("FNDR search");
    });

    it("lets a newer query supersede an in-flight one and ignores the stale answer", async () => {
        const first = deferred<MemoryCard[]>();
        const second = deferred<MemoryCard[]>();
        vi.mocked(searchMemoryCards)
            .mockReturnValueOnce(first.promise)
            .mockReturnValueOnce(second.promise);
        const { rerender } = render(<Harness query="first question" />);
        await advance(SEARCH_LIMITS.typingDebounceMs);
        const firstTraceId = latest?.id;
        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "running"]]);

        rerender(<Harness query="second question" />);
        expect(latest?.id).not.toBe(firstTraceId);
        expect(snapshot()).toEqual([["queued", "waiting"]]);
        await advance(SEARCH_LIMITS.typingDebounceMs);
        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "running"]]);

        await act(async () => {
            first.resolve([card()]);
        });
        // The stale answer must not complete the newer trace.
        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "running"]]);

        await act(async () => {
            second.resolve([card(), card()]);
        });
        expect(snapshot()).toEqual([["queued", "completed"], ["retrieval", "completed"], ["result", "completed"]]);
        expect(latest?.steps.find((step) => step.id === "result")?.detail).toBe("2 memories · Vector");
    });

    it("drops the trace when the query is cleared and never completes it afterwards", async () => {
        const request = deferred<MemoryCard[]>();
        vi.mocked(searchMemoryCards).mockReturnValue(request.promise);
        const { rerender } = render(<Harness query="first question" />);
        await advance(SEARCH_LIMITS.typingDebounceMs);

        rerender(<Harness query="  " />);
        expect(latest).toBeNull();

        await act(async () => {
            request.resolve([card()]);
        });
        expect(latest).toBeNull();
    });

    it("never places the query, titles, snippets, or app names in any step", async () => {
        const request = deferred<MemoryCard[]>();
        vi.mocked(searchMemoryCards).mockReturnValue(request.promise);
        render(<Harness query={QUERY_SECRET} />);
        await advance(SEARCH_LIMITS.typingDebounceMs);
        await act(async () => {
            request.resolve([card()]);
        });

        const text = traceText();
        for (const forbidden of [
            QUERY_SECRET,
            "Confidential board deck",
            "board-deck.key",
            "snippet body",
            "Summary text",
        ]) {
            expect(text).not.toContain(forbidden);
        }
    });
});

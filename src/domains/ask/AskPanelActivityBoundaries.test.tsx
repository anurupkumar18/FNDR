import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AskPanel } from "./AskPanel";
import { fndrAnswer, type ComposedAnswer } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({ fndrAnswer: vi.fn() }));

const QUESTION_SECRET = "what did legal say about project nightingale";
const ANSWER_SECRET = "Legal approved the nightingale acquisition on Tuesday";
const RAW_ERROR = "ENOENT /Users/alex/Library/fndr/answers.db token=abc123";

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

function answer(text: string): ComposedAnswer {
    return {
        query: QUESTION_SECRET,
        answer: text,
        evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
        cards: [],
        verify_outcome: { kind: "grounded", confidence: 0.9 },
        surfacing_reasons: [],
    };
}

function ask(text: string) {
    fireEvent.change(screen.getByLabelText("Search or ask FNDR"), { target: { value: text } });
    fireEvent.submit(screen.getByLabelText("Search or ask FNDR").closest("form") as HTMLFormElement);
}

function trace(): HTMLElement {
    return screen.getByLabelText("Ask FNDR activity");
}

function expandDetails() {
    fireEvent.click(screen.getByRole("button", { name: "Show Ask FNDR activity details" }));
}

beforeEach(() => {
    vi.useFakeTimers();
});

afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
});

describe("AskPanel activity boundaries", () => {
    it("keeps the request running while the answer call is pending", async () => {
        vi.mocked(fndrAnswer).mockReturnValue(deferred<ComposedAnswer>().promise);
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);

        ask(QUESTION_SECRET);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(30_000);
        });

        expect(trace()).toHaveTextContent("Requesting an answer from local memory");
        expect(trace()).toHaveTextContent("Running");
        expect(trace()).not.toHaveTextContent(/Completed|Failed/);
    });

    it("fails the same request step on a renderer timeout and names it a frontend event", async () => {
        vi.mocked(fndrAnswer).mockReturnValue(deferred<ComposedAnswer>().promise);
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);

        ask(QUESTION_SECRET);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(60_000);
        });

        expect(trace()).toHaveTextContent("Answer timed out");
        expandDetails();
        expect(screen.getAllByRole("listitem")).toHaveLength(1);
        expect(trace()).toHaveTextContent("Frontend event");
        expect(trace()).not.toHaveTextContent("Request boundary");
    });

    it("fails the same request step on a backend error without exposing the raw error", async () => {
        const request = deferred<ComposedAnswer>();
        vi.mocked(fndrAnswer).mockReturnValue(request.promise);
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);

        ask(QUESTION_SECRET);
        await act(async () => {
            request.reject(new Error(RAW_ERROR));
        });

        expect(trace()).toHaveTextContent("Answer failed");
        expandDetails();
        expect(screen.getAllByRole("listitem")).toHaveLength(1);
        expect(trace()).toHaveTextContent("Request boundary");
        expect(document.body).not.toHaveTextContent("ENOENT");
        expect(document.body).not.toHaveTextContent("abc123");
    });

    it("lets a newer question supersede an in-flight one and ignores the stale answer", async () => {
        const first = deferred<ComposedAnswer>();
        const second = deferred<ComposedAnswer>();
        vi.mocked(fndrAnswer)
            .mockReturnValueOnce(first.promise)
            .mockReturnValueOnce(second.promise);
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);

        ask("first question");
        ask("second question");
        await act(async () => {
            first.resolve(answer("stale answer"));
        });

        expect(trace()).toHaveTextContent("Requesting an answer from local memory");
        expect(trace()).not.toHaveTextContent("Grounded answer returned");

        await act(async () => {
            second.resolve(answer("fresh answer"));
        });
        expect(trace()).toHaveTextContent("Grounded answer returned");
    });

    it("never places the question or the answer text in the trace", async () => {
        const request = deferred<ComposedAnswer>();
        vi.mocked(fndrAnswer).mockReturnValue(request.promise);
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);

        ask(QUESTION_SECRET);
        expandDetails();
        expect(trace()).not.toHaveTextContent("nightingale");
        await act(async () => {
            request.resolve(answer(ANSWER_SECRET));
        });

        expect(trace()).toHaveTextContent("Grounded answer returned");
        expect(trace()).not.toHaveTextContent("nightingale");
        expect(trace()).not.toHaveTextContent("Legal approved");
    });
});

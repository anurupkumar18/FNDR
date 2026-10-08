import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { SEARCH_SUMMARY } from "@/shared/utils/config";
import { SearchBar } from "./SearchBar";

const ipcMocks = vi.hoisted(() => ({
    transcribeVoiceInput: vi.fn(),
    pauseCapture: vi.fn(),
    resumeCapture: vi.fn(),
    searchMemoryCards: vi.fn(),
    summarizeSearch: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipcMocks);

afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
    vi.unstubAllGlobals();
});

const defaultProps = {
    value: "",
    submittedValue: "",
    onChange: vi.fn(),
    onSubmit: vi.fn(),
    timeFilter: null,
    onTimeFilterChange: () => {},
    appFilter: null,
    onAppFilterChange: () => {},
    onSetMemoryCardsPanelOpen: () => {},
    appNames: ["Safari"],
    resultCount: 0,
    searchResults: [],
};

function summaryCard(id: string, text: string, coverage = 1): MemoryCard {
    return {
        id,
        title: `Private title ${id}`,
        summary: text,
        action: "Reviewed",
        context: [],
        timestamp: 1,
        app_name: "Private app",
        window_title: "Private window",
        score: 0.9,
        source_count: 1,
        raw_snippets: [text],
        anchor_coverage_score: coverage,
    };
}

class FakeMediaRecorder {
    static instances: FakeMediaRecorder[] = [];
    static isTypeSupported() {
        return true;
    }

    mimeType = "audio/webm";
    state: RecordingState = "inactive";
    startArgs: unknown[] | null = null;
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;

    constructor() {
        FakeMediaRecorder.instances.push(this);
    }

    start(...args: unknown[]) {
        this.startArgs = args;
        this.state = "recording";
    }

    stop() {
        this.state = "inactive";
        this.ondataavailable?.({ data: new Blob(["audio"], { type: this.mimeType }) });
        this.onstop?.();
    }
}

if (typeof Blob.prototype.arrayBuffer !== "function") {
    Blob.prototype.arrayBuffer = function arrayBuffer(): Promise<ArrayBuffer> {
        return Promise.resolve(new TextEncoder().encode("audio").buffer);
    };
}

describe("SearchBar", () => {
    afterEach(() => {
        ipcMocks.transcribeVoiceInput.mockReset();
    });

    it("renders input and forwards changes", () => {
        const onChange = vi.fn();
        render(<SearchBar {...defaultProps} onChange={onChange} />);
        const input = screen.getByRole("textbox", { name: /search memories/i });
        fireEvent.change(input, { target: { value: "oauth" } });
        expect(onChange).toHaveBeenCalledWith("oauth");
    });

    it("submits only when Enter is pressed", () => {
        const onSubmit = vi.fn();
        render(<SearchBar {...defaultProps} value="oauth flow" onSubmit={onSubmit} />);
        const input = screen.getByRole("textbox", { name: /search memories/i });
        fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
        expect(onSubmit).toHaveBeenCalledTimes(1);
    });

    it("renders the voice button", () => {
        render(<SearchBar {...defaultProps} />);
        expect(screen.getByRole("button", { name: /voice recording/i })).toBeInTheDocument();
    });

    it("keeps typed search available when microphone capture is unavailable", () => {
        vi.stubGlobal("MediaRecorder", undefined);
        render(<SearchBar {...defaultProps} />);

        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        expect(screen.getByText(
            "Microphone isn't available here. Type your search instead."
        )).not.toHaveAttribute("role");
        expect(screen.getByLabelText("Voice input activity")).toContainElement(
            screen.getByRole("status"),
        );
        expect(screen.getByRole("textbox", { name: "Search memories" })).toBeEnabled();
    });

    it("records one complete clip instead of requesting time-sliced fragments", async () => {
        FakeMediaRecorder.instances = [];
        vi.stubGlobal("MediaRecorder", FakeMediaRecorder);
        vi.stubGlobal("navigator", Object.assign(Object.create(navigator), {
            mediaDevices: {
                getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }),
            },
        }));
        render(<SearchBar {...defaultProps} />);

        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        await waitFor(() => expect(FakeMediaRecorder.instances).toHaveLength(1));
        expect(FakeMediaRecorder.instances[0].startArgs).toEqual([]);
    });

    it("shows the real voice workflow without exposing transcript content", async () => {
        FakeMediaRecorder.instances = [];
        ipcMocks.transcribeVoiceInput.mockResolvedValue({ text: "private search phrase" });
        vi.stubGlobal("MediaRecorder", FakeMediaRecorder);
        vi.stubGlobal("navigator", Object.assign(Object.create(navigator), {
            mediaDevices: {
                getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }),
            },
        }));
        render(<SearchBar {...defaultProps} />);

        const startedAt = Date.now();
        const clock = vi.spyOn(Date, "now").mockReturnValue(startedAt);
        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));
        await screen.findByText("Recording voice input");

        clock.mockReturnValue(startedAt + 1_500);
        fireEvent.click(screen.getByRole("button", { name: "Stop voice recording" }));

        await screen.findByText("Transcript ready");
        expect(screen.getByRole("region", { name: "Voice input activity" })).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Show Voice input activity details" }));
        expect(screen.getByText("Requesting microphone access")).toBeInTheDocument();
        expect(screen.getByText("Recording stopped")).toBeInTheDocument();
        expect(screen.getByText("Transcribing on this Mac")).toBeInTheDocument();
        expect(screen.queryByText("private search phrase")).not.toBeInTheDocument();
        clock.mockRestore();
    });

    it("shows the disabled hint and disables the input", () => {
        render(
            <SearchBar
                {...defaultProps}
                disabled={true}
                disabledHint="Waiting for backend"
            />
        );

        expect(screen.getByText(/waiting for backend/i)).toBeInTheDocument();
        expect(screen.getByRole("textbox")).toBeDisabled();
    });

    it("leaves Cmd+K to the command palette and only clears a focused search", () => {
        const onChange = vi.fn();
        const onSubmit = vi.fn();
        render(<SearchBar {...defaultProps} value="oauth" onChange={onChange} onSubmit={onSubmit} />);
        const input = screen.getByRole("textbox", { name: /search memories/i });

        fireEvent.keyDown(window, { key: "k", metaKey: true });
        expect(input).not.toHaveFocus();
        expect(onChange).not.toHaveBeenCalled();

        fireEvent.keyDown(window, { key: "Escape" });
        expect(onChange).not.toHaveBeenCalled();

        input.focus();
        fireEvent.keyDown(window, { key: "Escape" });
        expect(onChange).toHaveBeenCalledWith("");
        expect(onSubmit).toHaveBeenCalledWith("");
    });

    it("labels both filters and announces a grammatically correct result count", () => {
        render(
            <SearchBar
                {...defaultProps}
                submittedValue="oauth"
                resultCount={1}
                timeFilter="24h"
                appFilter="Safari"
            />
        );

        expect(screen.getByRole("combobox", { name: "Time range" })).toHaveValue("24h");
        expect(screen.getByRole("combobox", { name: "App" })).toHaveValue("Safari");
        expect(screen.getByRole("status", { name: "Search result count" })).toHaveTextContent(
            "1 result"
        );
    });

    it("announces that edited text has not been searched yet", () => {
        render(
            <SearchBar
                {...defaultProps}
                value="oauth refresh"
                submittedValue="oauth"
            />
        );

        expect(screen.getByText("Press Enter to search")).toHaveAttribute("role", "status");
    });

    it("traces the observed summary workflow without exposing search evidence", async () => {
        vi.useFakeTimers();
        let resolveSummary: ((value: string) => void) | undefined;
        ipcMocks.summarizeSearch.mockReturnValue(new Promise<string>((resolve) => {
            resolveSummary = resolve;
        }));
        const privateQuery = "oauth migration";
        const privateSnippet = "oauth migration private evidence";

        render(
            <SearchBar
                {...defaultProps}
                submittedValue={privateQuery}
                resultCount={2}
                searchResults={[
                    summaryCard("one", privateSnippet),
                    summaryCard("two", `${privateSnippet} two`),
                ]}
            />,
        );

        const trace = screen.getByRole("region", { name: "Search summary activity" });
        expect(within(trace).getByText("Waiting for search results to settle")).toBeInTheDocument();
        expect(screen.queryByText("Synthesizing memories...")).not.toBeInTheDocument();

        await act(async () => {
            await vi.advanceTimersByTimeAsync(SEARCH_SUMMARY.delayMs);
        });

        expect(within(trace).getByText("Requesting grounded summary")).toBeInTheDocument();
        fireEvent.click(within(trace).getByRole("button", { name: "Show Search summary activity details" }));
        expect(within(trace).getByText("Search-result settle delay completed")).toBeInTheDocument();
        expect(within(trace).getByText("Evaluated evidence coverage")).toBeInTheDocument();
        expect(within(trace).getByText(/2 of 2 memories met coverage/i)).toBeInTheDocument();
        expect(trace).not.toHaveTextContent(privateQuery);
        expect(trace).not.toHaveTextContent(privateSnippet);
        expect(trace).not.toHaveTextContent("Private title");

        await act(async () => {
            resolveSummary?.("OAuth migration is grounded in OAuth migration evidence.");
            await Promise.resolve();
        });

        expect(within(trace).getAllByText("Summary ready")).toHaveLength(2);
        expect(screen.getByText("OAuth migration is grounded in OAuth migration evidence.")).toBeInTheDocument();
    });

    it("keeps an observed skipped result when evidence coverage is insufficient", async () => {
        vi.useFakeTimers();
        render(
            <SearchBar
                {...defaultProps}
                submittedValue="oauth migration"
                resultCount={2}
                searchResults={[
                    summaryCard("one", "private unrelated evidence", 0),
                    summaryCard("two", "another private unrelated item", 0),
                ]}
            />,
        );

        await act(async () => {
            await vi.advanceTimersByTimeAsync(SEARCH_SUMMARY.delayMs);
        });

        const trace = screen.getByRole("region", { name: "Search summary activity" });
        expect(within(trace).getByText("Summary skipped: insufficient evidence")).toBeInTheDocument();
        expect(ipcMocks.summarizeSearch).not.toHaveBeenCalled();
        expect(trace).not.toHaveTextContent("private unrelated evidence");
    });

    it("records when a returned summary is withheld by the evidence check", async () => {
        vi.useFakeTimers();
        ipcMocks.summarizeSearch.mockResolvedValue("private unrelated generated content");
        render(
            <SearchBar
                {...defaultProps}
                submittedValue="oauth migration"
                resultCount={2}
                searchResults={[
                    summaryCard("one", "oauth migration evidence one"),
                    summaryCard("two", "oauth migration evidence two"),
                ]}
            />,
        );

        await act(async () => {
            await vi.advanceTimersByTimeAsync(SEARCH_SUMMARY.delayMs);
        });

        const trace = screen.getByRole("region", { name: "Search summary activity" });
        expect(within(trace).getByText("Summary withheld by evidence check")).toBeInTheDocument();
        expect(trace).toHaveAttribute("data-status", "degraded");
        expect(trace).not.toHaveTextContent("private unrelated generated content");
        expect(screen.queryByText("private unrelated generated content")).not.toBeInTheDocument();
    });

    it("shows a sanitized failure when the summary request fails", async () => {
        vi.useFakeTimers();
        const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
        ipcMocks.summarizeSearch.mockRejectedValue(
            new Error("private prompt failed at /Users/private/vault.db"),
        );
        render(
            <SearchBar
                {...defaultProps}
                submittedValue="oauth migration"
                resultCount={2}
                searchResults={[
                    summaryCard("one", "oauth migration evidence one"),
                    summaryCard("two", "oauth migration evidence two"),
                ]}
            />,
        );

        await act(async () => {
            await vi.advanceTimersByTimeAsync(SEARCH_SUMMARY.delayMs);
        });

        const trace = screen.getByRole("region", { name: "Search summary activity" });
        expect(within(trace).getByText("Summary request failed")).toBeInTheDocument();
        expect(trace).not.toHaveTextContent("private prompt");
        expect(trace).not.toHaveTextContent("vault.db");
        expect(consoleError).toHaveBeenCalledWith("Summary generation failed");
        consoleError.mockRestore();
    });
});

/**
 * MemoryCard lifecycle presentation — Subagent 10.
 *
 * Covers the vault's reviewed-memory surface: the lifecycle status chip
 * (DEVELOPED / PENDING / RAW / REVIEW_FAILED / VISUAL_FAILED), the compact-card
 * preview-text priority (insight_what_happened > reviewed display_summary >
 * memory_context excerpt > safe fallback), and meta-OCR narration cleanup.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { MemoryCard as MemoryCardData } from "@/shared/ipc/tauri";
import {
    MemoryCard,
    deriveLifecycleStatus,
    isMetaOcrNarration,
} from "../MemoryCard";
import { MemoryProvenanceStrip } from "../MemoryProvenanceStrip";
import { InsightLayers } from "../InsightLayers";

afterEach(() => {
    cleanup();
});

function makeCard(overrides: Partial<MemoryCardData> = {}): MemoryCardData {
    return {
        id: "test-card-abcd-1234",
        title: "Lifecycle stress test",
        summary: "Captured activity in VS Code.",
        action: "Reviewed key details",
        context: ["FNDR"],
        timestamp: new Date("2026-05-21T10:30:00Z").getTime(),
        app_name: "VS Code",
        window_title: "src/retrieval/hybrid.rs",
        score: 0.82,
        source_count: 1,
        raw_snippets: [],
        ...overrides,
    };
}

describe("deriveLifecycleStatus", () => {
    it("returns DEVELOPED for reviewed_local", () => {
        expect(deriveLifecycleStatus(makeCard({ enrichment_status: "reviewed_local" }))).toBe(
            "DEVELOPED",
        );
    });
    it("returns DEVELOPED for reviewed_daily", () => {
        expect(deriveLifecycleStatus(makeCard({ enrichment_status: "reviewed_daily" }))).toBe(
            "DEVELOPED",
        );
    });
    it("returns PENDING for pending", () => {
        expect(deriveLifecycleStatus(makeCard({ enrichment_status: "pending" }))).toBe("PENDING");
    });
    it("returns REVIEW_FAILED for review_failed", () => {
        expect(deriveLifecycleStatus(makeCard({ enrichment_status: "review_failed" }))).toBe(
            "REVIEW_FAILED",
        );
    });
    it("returns RAW when enrichment_status is empty", () => {
        expect(deriveLifecycleStatus(makeCard())).toBe("RAW");
    });
    it("returns VISUAL_FAILED when storage_outcome is visual_semantics_failed", () => {
        // VISUAL_FAILED overrides anything enrichment_status might claim — a
        // failed visual ingest must never look like a reviewed memory.
        expect(
            deriveLifecycleStatus(
                makeCard({
                    enrichment_status: "reviewed_local",
                    storage_outcome: "visual_semantics_failed",
                }),
            ),
        ).toBe("VISUAL_FAILED");
    });
});

describe("MemoryCard — lifecycle chip rendering (expanded variant)", () => {
    it("reviewed_local renders the DEVELOPED stamp", () => {
        render(
            <MemoryCard
                variant="expanded"
                card={makeCard({ enrichment_status: "reviewed_local" })}
            />,
        );
        const stamp = screen.getByLabelText("memory status: DEVELOPED");
        expect(stamp).toBeTruthy();
        expect(stamp.textContent).toBe("DEVELOPED");
    });

    it("pending renders the PENDING stamp", () => {
        render(
            <MemoryCard
                variant="expanded"
                card={makeCard({ enrichment_status: "pending" })}
            />,
        );
        const stamp = screen.getByLabelText("memory status: PENDING");
        expect(stamp).toBeTruthy();
        expect(stamp.textContent).toBe("PENDING");
    });

    it("review_failed renders the REVIEW FAILED stamp", () => {
        render(
            <MemoryCard
                variant="expanded"
                card={makeCard({ enrichment_status: "review_failed" })}
            />,
        );
        const stamp = screen.getByLabelText("memory status: REVIEW_FAILED");
        expect(stamp).toBeTruthy();
        expect(stamp.textContent).toBe("REVIEW FAILED");
    });

    it("visual_semantics_failed renders VISUAL FAILED — does not look like a good memory", () => {
        render(
            <MemoryCard
                variant="expanded"
                card={makeCard({
                    enrichment_status: "reviewed_local",
                    storage_outcome: "visual_semantics_failed",
                    insight_what_happened: "",
                    display_summary: "",
                })}
            />,
        );
        const stamp = screen.getByLabelText("memory status: VISUAL_FAILED");
        expect(stamp).toBeTruthy();
        expect(stamp.textContent).toBe("VISUAL FAILED");
        // The "DEVELOPED" label must not be present for a failed visual ingest.
        expect(screen.queryByLabelText("memory status: DEVELOPED")).toBeNull();
    });

    it("absent lifecycle fields fall back to RAW", () => {
        render(<MemoryCard variant="expanded" card={makeCard()} />);
        const stamp = screen.getByLabelText("memory status: RAW");
        expect(stamp).toBeTruthy();
    });

    it("confirms permanent deletion in-product and reports a failed delete", async () => {
        const onDelete = vi.fn().mockResolvedValue(false);
        render(<MemoryCard variant="expanded" card={makeCard()} onDelete={onDelete} />);

        fireEvent.click(screen.getByRole("button", { name: "Delete memory" }));

        expect(screen.getByText(/delete “lifecycle stress test” permanently/i)).toBeTruthy();
        expect(onDelete).not.toHaveBeenCalled();

        fireEvent.click(screen.getByRole("button", { name: "Delete permanently" }));

        await waitFor(() => expect(onDelete).toHaveBeenCalledWith("test-card-abcd-1234"));
        expect(await screen.findByRole("alert")).toHaveTextContent(
            "FNDR could not delete this memory. It is still in your vault.",
        );
    });

    it("lets the user cancel permanent deletion without mutating anything", () => {
        const onDelete = vi.fn();
        render(<MemoryCard variant="expanded" card={makeCard()} onDelete={onDelete} />);

        fireEvent.click(screen.getByRole("button", { name: "Delete memory" }));
        fireEvent.click(screen.getByRole("button", { name: "Cancel deletion" }));

        expect(screen.queryByText(/delete “lifecycle stress test” permanently/i)).toBeNull();
        expect(onDelete).not.toHaveBeenCalled();
    });
});

describe("MemoryProvenanceStrip", () => {
    it("identifies an agent note as added rather than captured", () => {
        render(<MemoryProvenanceStrip card={{ ...makeCard(), ...{ source_type: "agent", added_by: "Claude Code" } }} />);
        expect(screen.getByText("Added")).toBeTruthy();
        expect(screen.getByText("Added by")).toBeTruthy();
        expect(screen.getByText("Claude Code")).toBeTruthy();
        expect(screen.queryByText("Captured")).toBeNull();
        expect(screen.queryByText("Text captured via")).toBeNull();
        expect(screen.getByText("Title")).toBeTruthy();
        expect(screen.queryByText("Window")).toBeNull();
        expect(screen.getByText("ADDED")).toBeTruthy();
    });

    it.each([
        ["ax", "Accessibility"],
        ["ocr", "Screen text (OCR)"],
        ["browser_semantic", "Browser page text"],
        ["mixed", "Multiple sources"],
        ["unknown", "Unknown"],
        [undefined, "Unknown"],
    ] as const)("shows captured text source %s independently of synthesis", (source, label) => {
        render(<MemoryProvenanceStrip card={makeCard({ synthesis_branch: "vlm", text_source: source })} />);
        expect(screen.getByText("Text captured via")).toBeTruthy();
        expect(screen.getByText(label)).toBeTruthy();
    });

    it("does not treat synthesis_branch alone as DEVELOPED", () => {
        render(
            <MemoryProvenanceStrip
                card={makeCard({
                    synthesis_branch: "llm_ocr_grounded_visual_fallback",
                    enrichment_status: "pending_visual_semantics",
                    storage_outcome: "low_quality_evidence",
                })}
            />,
        );

        expect(screen.getByText("PENDING")).toBeTruthy();
        expect(screen.queryByText("DEVELOPED")).toBeNull();
    });
});

describe("MemoryCard — compact preview priority", () => {
    it("does not present an agent note as a developed screen capture", () => {
        const card = makeCard({ source_type: "agent", enrichment_status: "reviewed_local" });
        expect(deriveLifecycleStatus(card)).toBe("ADDED");
        render(<MemoryCard variant="compact" card={card} sourceIcon={<span>Screen capture</span>} />);
        expect(screen.queryByText("Screen capture")).toBeNull();
    });

    it("preserves an agent's note even when its words resemble OCR narration", () => {
        const text = "The screen shows a regression; keep this decision until the review finishes.";
        render(<MemoryCard variant="compact" card={makeCard({ source_type: "agent", display_summary: text, summary: text })} />);
        expect(screen.getByTitle(text)).toBeTruthy();
    });

    it.each(["compact", "preview", "expanded"] as const)("labels agent authorship in %s cards and never offers reopen", (variant) => {
        render(<MemoryCard variant={variant} onReopen={vi.fn()} card={{ ...makeCard({ reopen_target: "https://example.com" }), ...{ source_type: "agent", added_by: "Claude Code" } }} />);
        expect(screen.getByText(/Agent note · Added by Claude Code/)).toBeTruthy();
        expect(screen.queryByRole("button", { name: /open source|reopen/i })).toBeNull();
    });

    it("reviewed summary wins over raw OCR / window title", () => {
        render(
            <MemoryCard
                variant="compact"
                card={makeCard({
                    enrichment_status: "reviewed_local",
                    display_summary:
                        "Re-ranked hybrid retrieval results and tuned the chunk-first router.",
                    summary: "raw OCR junk that should never surface",
                })}
            />,
        );
        // The reviewed display_summary is used; the noisy `summary` should be hidden.
        expect(
            screen.getByTitle(
                "Re-ranked hybrid retrieval results and tuned the chunk-first router.",
            ),
        ).toBeTruthy();
        expect(screen.queryByTitle("raw OCR junk that should never surface")).toBeNull();
    });

    it("insight_what_happened wins over reviewed display_summary", () => {
        render(
            <MemoryCard
                variant="compact"
                card={makeCard({
                    enrichment_status: "reviewed_local",
                    insight_what_happened:
                        "User finalised the chunk-first retrieval router design.",
                    display_summary: "Reviewed retrieval design",
                })}
            />,
        );
        expect(
            screen.getByTitle("User finalised the chunk-first retrieval router design."),
        ).toBeTruthy();
    });

    it("meta OCR narration is hidden / cleaned from the preview", () => {
        render(
            <MemoryCard
                variant="compact"
                card={makeCard({
                    enrichment_status: "reviewed_local",
                    display_summary:
                        "The OCR text indicates the user is on a settings page with toggles.",
                    internal_context: "User adjusted memory-review settings to enable local review.",
                })}
            />,
        );
        // Meta narration is stripped — the cleaner internal_context surfaces instead.
        expect(
            screen.getByTitle(
                "User adjusted memory-review settings to enable local review.",
            ),
        ).toBeTruthy();
        expect(
            screen.queryByText(/The OCR text indicates/i),
        ).toBeNull();
    });

    it("never exposes raw clean_text-style meta narration as preview", () => {
        render(
            <MemoryCard
                variant="compact"
                card={makeCard({
                    summary: "The screen shows a New Tab page with toolbar buttons.",
                    window_title: "Settings — Privacy",
                })}
            />,
        );
        // Meta-OCR `summary` is rejected; the safe fallback (window_title) is used.
        expect(
            screen.queryByTitle("The screen shows a New Tab page with toolbar buttons."),
        ).toBeNull();
    });
});

describe("InsightLayers", () => {
    it("hides empty insight rows and internal synthesis identifiers", () => {
        render(
            <InsightLayers
                card={makeCard({
                    insight_what_happened: "Resolved the retrieval router borrow error.",
                    insight_why_mattered: "The capture pipeline can continue safely.",
                    synthesis_branch: "llm_ocr_grounded_visual_fallback",
                })}
            />,
        );

        expect(screen.getByText("What happened")).toBeTruthy();
        expect(screen.getByText("Why it mattered")).toBeTruthy();
        expect(screen.queryByText("What changed")).toBeNull();
        expect(screen.queryByText("Thread")).toBeNull();
        expect(screen.queryByText("llm_ocr_grounded_visual_fallback")).toBeNull();
        expect(screen.queryByText(/not yet extracted/i)).toBeNull();
    });
});

describe("isMetaOcrNarration", () => {
    it("flags classic OCR-narration prefixes", () => {
        expect(isMetaOcrNarration("The OCR text indicates the user is browsing.")).toBe(true);
        expect(isMetaOcrNarration("The screen shows a settings panel.")).toBe(true);
        expect(isMetaOcrNarration("Based on the OCR, the user opened a PR.")).toBe(true);
        expect(isMetaOcrNarration("I can see a list of memory records.")).toBe(true);
    });
    it("leaves clean reviewer-grade summaries alone", () => {
        expect(
            isMetaOcrNarration("User finalised the chunk-first retrieval router design."),
        ).toBe(false);
        expect(isMetaOcrNarration("")).toBe(false);
    });
});

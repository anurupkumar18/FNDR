import { useState, useEffect } from "react";
import { MemoryCard } from "@/shared/ipc/tauri";
import { cleanupCardsForRender, tokenOverlap } from "@/shared/utils/cardCleanup";
import { extractAnchorTerms, scoreAnchorCoverage } from "@/shared/utils/search";
import { TIMELINE_DEDUPE, TIMELINE_MATCH_LABEL, TIMELINE_STREAM } from "./timelineConfig";
import { InsightLayers } from "@/domains/memory-vault/InsightLayers";
import { Icon } from "@/shared/components/atoms";
import "./Timeline.css";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";

interface TimelineProps {
    results: MemoryCard[];
    isLoading: boolean;
    query: string;
    selectedResultId: string | null;
    onSelectResult: (result: MemoryCard) => void;
    onDeleteMemory?: (memoryId: string) => void;
    evalUi?: boolean;
}

function formatDay(timestamp: number): string {
    const date = new Date(timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date.toDateString() === today.toDateString()) {
        return "Today";
    }
    if (date.toDateString() === yesterday.toDateString()) {
        return "Yesterday";
    }
    return date.toLocaleDateString(undefined, {
        weekday: "long",
        month: "long",
        day: "numeric",
    });
}

function normalizePreview(value: string): string {
    return value
        .trim()
        .toLowerCase()
        .replace(/\s+/g, " ");
}

function isLowSignalPreview(summary: string, appName: string): boolean {
    const normalized = normalizePreview(summary);
    if (!normalized) {
        return true;
    }
    const app = normalizePreview(appName);
    if (normalized === app || normalized === "fndr" || normalized === "codex") {
        return true;
    }
    return normalized.split(" ").length <= 2;
}

function stripLegacySources(summary: string): string {
    return summary.replace(/\s*Sources:\s*[A-Za-z0-9,\-\s]+\.?$/i, "").trim();
}

export function Timeline({
    results,
    isLoading,
    query,
    selectedResultId,
    onSelectResult,
    onDeleteMemory,
    evalUi = false,
}: TimelineProps) {
    const [visibleCount, setVisibleCount] = useState<number>(TIMELINE_STREAM.initialVisible);

    useEffect(() => {
        setVisibleCount(TIMELINE_STREAM.initialVisible);
    }, [query]);

    if (isLoading) {
        return (
            <div className="timeline-state" role="status" aria-live="polite" aria-busy="true">
                <ThinkingIndicator state="searching" size="md" />
                <p>Searching saved memories…</p>
            </div>
        );
    }

    if (results.length === 0) {
        if (!query.trim()) {
            return (
                <div className="timeline-state timeline-welcome">
                    <div className="welcome-icon">⌘</div>
                    <h2>Find a saved memory</h2>
                    <p>Search by topic, app, person, or time to revisit what FNDR has saved.</p>
                </div>
            );
        }
        return (
            <div className="timeline-state">
                <div className="empty-icon"><Icon name="search" size={48} /></div>
                <h3>No saved memories match</h3>
                <p className="timeline-query-preview">“{formatQueryPreview(query)}”</p>
                <p>Try fewer words, or change the time range or app filter.</p>
            </div>
        );
    }

    const visibleResults = cleanupCardsForRender(results.slice(0, visibleCount));
    const hasMore = results.length > visibleCount;
    const filteredResults = filterConsecutiveSimilar(visibleResults);
    return (
        <div className="timeline-container">
            <div className="timeline-stream">
                {filteredResults.map((result) => {
                    const cleanSummary = stripLegacySources(result.display_summary ?? result.summary);
                    const displayTitle = preferredTitle(result);
                    const primaryText = cleanSummary || displayTitle || "Captured memory";
                    const appLabel = result.app_name.trim() || "Unknown app";
                    const controlLabel = formatControlLabel(displayTitle || primaryText);
                    const showPrimaryText =
                        !displayTitle ||
                        (!isLowSignalPreview(primaryText, result.app_name) &&
                            tokenOverlap(displayTitle, primaryText) < 0.75);
                    const matchReason = preferredMatchReason(result, query);
                    const domain = domainFromUrl(result.url);
                    const confidence = result.confidence ?? result.score;
                    const detailLabels = Array.from(new Set([
                        matchReason,
                        qualityLabel(result, query, confidence),
                        domain,
                        result.timeline_action_class && result.timeline_action_class !== "other"
                            ? result.timeline_action_class
                            : "",
                        result.source_count > 1 ? `${result.source_count} sources` : "",
                    ].filter(Boolean)));
                    const evidence = (result.raw_snippets ?? [])
                        .map((snippet) => stripLegacySources(snippet).trim())
                        .filter(Boolean)
                        .slice(0, 3);
                    return (
                        <article
                            key={result.id}
                            className={`result-card ${selectedResultId === result.id ? "selected" : ""}`}
                            onClick={() => onSelectResult(result)}
                            aria-current={selectedResultId === result.id ? "true" : undefined}
                        >
                            <div className={`result-meta ${evalUi ? "result-meta-eval" : ""}`}>
                                <div className="result-meta-main">
                                    <span className="result-app">{appLabel}</span>
                                    <span className="result-time">
                                        {formatTimestamp(result.timestamp)}
                                    </span>
                                </div>
                                <div className="result-meta-actions">
                                    {evalUi && (
                                        <span className="result-score" title="Relevance score">
                                            score {result.score.toFixed(3)}
                                        </span>
                                    )}
                                    <button
                                        type="button"
                                        className="ui-action-btn timeline-select-btn"
                                        onClick={(event) => {
                                            event.stopPropagation();
                                            onSelectResult(result);
                                        }}
                                        aria-label={`${selectedResultId === result.id ? "Selected" : "Select"} memory: ${controlLabel}`}
                                        aria-pressed={selectedResultId === result.id}
                                        title={selectedResultId === result.id
                                            ? "Selected for memory commands"
                                            : "Select for memory commands"}
                                    >
                                        {selectedResultId === result.id ? "Selected" : "Select"}
                                    </button>
                                    {onDeleteMemory && (
                                        <button
                                            type="button"
                                            className="ui-action-btn timeline-delete-btn"
                                            onClick={(event) => {
                                                event.stopPropagation();
                                                onDeleteMemory(result.id);
                                            }}
                                            aria-label="Delete this memory"
                                            title="Delete this memory"
                                        >
                                            Delete
                                        </button>
                                    )}
                                </div>
                            </div>
                            {displayTitle && (
                                <h3 className="result-title">
                                    {displayTitle}
                                </h3>
                            )}
                            {showPrimaryText && <p className="result-primary">{primaryText}</p>}
                            <InsightLayers card={result} evalUi={evalUi} />
                            <div
                                className="result-context-chips"
                                aria-label="Why this memory matched and where it came from"
                            >
                                <span className="result-context-label">Why this matched</span>
                                {detailLabels.map((label) => (
                                    <span className="result-chip" key={label}>{label}</span>
                                ))}
                            </div>
                            {evidence.length > 0 && (
                                <details
                                    className="source-details"
                                    onClick={(event) => event.stopPropagation()}
                                >
                                    <summary>Evidence snippets</summary>
                                    <ul>
                                        {evidence.map((snippet, index) => (
                                            <li key={`${result.id}-evidence-${index}`}>{snippet}</li>
                                        ))}
                                    </ul>
                                </details>
                            )}
                        </article>
                    );
                })}
            </div>

            {results.length > TIMELINE_STREAM.initialVisible && (
                <div className="load-more-container">
                    <button
                        type="button"
                        onClick={() => {
                            if (hasMore) {
                                setVisibleCount((n) => n + TIMELINE_STREAM.loadMoreStep);
                            }
                        }}
                        className="load-more-btn"
                        aria-disabled={!hasMore}
                    >
                        {hasMore
                            ? `Load ${Math.min(TIMELINE_STREAM.loadMoreStep, results.length - visibleCount)} more`
                            : `All ${results.length} results shown`}
                    </button>
                </div>
            )}
        </div>
    );
}

function formatQueryPreview(query: string): string {
    const normalized = query.replace(/\s+/g, " ").trim();
    return normalized.length > 120 ? `${normalized.slice(0, 117)}…` : normalized;
}

function formatControlLabel(value: string): string {
    const normalized = value.replace(/\s+/g, " ").trim();
    return normalized.length > 96 ? `${normalized.slice(0, 93)}…` : normalized;
}

function formatTimestamp(timestamp: number): string {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) {
        return "Time unavailable";
    }
    return `${formatDay(timestamp)} · ${date.toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
    })}`;
}

function preferredTitle(result: MemoryCard): string {
    const title = (result.title || "").trim();
    const windowTitle = (result.window_title || "").trim();
    const app = result.app_name.toLowerCase();
    const lowerWindow = windowTitle.toLowerCase();
    const genericWindow =
        !windowTitle
        || lowerWindow === app
        || includesAny(lowerWindow, ["new tab", "dashboard", "home", "settings"]);

    if (!genericWindow && (title.endsWith("...") || !title)) {
        return windowTitle;
    }

    return title || windowTitle;
}

function includesAny(haystack: string, needles: string[]): boolean {
    return needles.some((needle) => haystack.includes(needle));
}

function domainFromUrl(url: string | undefined): string {
    if (!url) {
        return "";
    }
    try {
        return new URL(url).hostname.replace(/^www\./, "");
    } catch {
        return url
            .replace(/^https?:\/\//i, "")
            .split("/")[0]
            .replace(/^www\./, "");
    }
}

function preferredMatchReason(result: MemoryCard, query: string): string {
    const appName = normalizePreview(result.app_name);
    const windowTitle = normalizePreview(result.window_title);
    const explicit = (result.context ?? [])
        .map((value) => value.trim())
        .find((value) => {
            const normalized = normalizePreview(value);
            return normalized.length > 0
                && normalized !== appName
                && normalized !== windowTitle;
        });
    if (explicit) {
        return explicit;
    }
    if (!query.trim()) {
        return "Recent memory";
    }
    if ((result.confidence ?? result.score) < TIMELINE_MATCH_LABEL.lowConfidenceMatchMax) {
        return "Low-confidence match";
    }
    return "Semantic + keyword match";
}

function qualityLabel(result: MemoryCard, query: string, confidence: number): string {
    if (!query.trim()) {
        return "Recent memory";
    }

    const coverage = result.anchor_coverage_score
        ?? scoreAnchorCoverage(
            `${result.title} ${result.display_summary ?? result.summary} ${(result.raw_snippets ?? []).join(" ")}`,
            extractAnchorTerms(query)
        );

    if (coverage >= TIMELINE_MATCH_LABEL.anchorCoverageDirect) {
        return "Direct match";
    }
    if (coverage >= TIMELINE_MATCH_LABEL.anchorCoverageRelated) {
        return "Related";
    }
    if (confidence >= TIMELINE_MATCH_LABEL.semanticStrongMin) {
        return "Strong match";
    }
    return "Contextual";
}

function filterConsecutiveSimilar(results: MemoryCard[]): MemoryCard[] {
    if (results.length <= 1) return results;

    const filtered: MemoryCard[] = [results[0]];
    for (let i = 1; i < results.length; i++) {
        const prev = filtered[filtered.length - 1];
        const curr = results[i];

        // Skip if same app and < 30s diff and highly similar title.
        const summaryOverlap = tokenOverlap(
            curr.display_summary ?? curr.summary,
            prev.display_summary ?? prev.summary
        );
        if (
            curr.app_name === prev.app_name &&
            Math.abs(curr.timestamp - prev.timestamp) < TIMELINE_DEDUPE.sameAppWindowMs &&
            (curr.title.toLowerCase() === prev.title.toLowerCase()
                || summaryOverlap > TIMELINE_DEDUPE.summaryOverlapMax)
        ) {
            continue;
        }
        filtered.push(curr);
    }
    return filtered;
}

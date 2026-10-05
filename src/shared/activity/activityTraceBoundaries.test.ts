import { describe, expect, it } from "vitest";

/**
 * PX-07 privacy gate. Every producer that feeds the shared activity trace is
 * scanned for the expressions that can reach a step label or detail. A new
 * interpolation or non-literal value fails here until a reviewer has confirmed
 * it is a count, a logical model identifier, or producer-authored copy, and
 * added it to the lists below. Query text, transcripts, OCR, prompts, answers,
 * titles, URLs, paths, and raw errors must never be added to these lists.
 */
const sources = import.meta.glob<string>("/src/**/*.{ts,tsx}", {
    query: "?raw",
    import: "default",
    eager: true,
});

const producers = Object.entries(sources).filter(([path, text]) =>
    !/\.test\.tsx?$/.test(path)
    && !path.startsWith("/src/shared/activity/activityTrace.ts")
    && /ActivityTraceStep|recordActivityStep\(|beginActivityTrace\(/.test(text),
);

/** Interpolations inside a template-literal `label:` or `detail:`. */
const ALLOWED_INTERPOLATIONS: Record<string, string[]> = {
    "/src/domains/memory-vault/MemoryCardsPanel.tsx": [
        'hits.length',
        'hits.length === 1 ? "match" : "matches"',
        'items.length === 1 ? "memory" : "memories"',
        'items.length.toLocaleString()',
    ],
    "/src/domains/notch/NotchHud.tsx": [
        'count',
        'count === 1 ? "match" : "matches"',
        'count === 1 ? "source" : "sources"',
    ],
    "/src/domains/omnibar/OmnibarApp.tsx": [
        'matchCountLabel(cards.length)',
        'matchCountLabel(entries.length)',
    ],
    "/src/domains/privacy-proof/PrivacyProof.tsx": [
        'next.egress_requests',
        'next.egress_requests === 1 ? "request" : "requests"',
        'next.evaluated',
        'next.stored',
        'skipped',
    ],
    "/src/domains/search/SearchBar.tsx": [
        'latestResults.length',
        'snippets.length',
        'topicalCards.length',
    ],
    "/src/domains/workspace/AgentWorkspace.tsx": [
        'memories.length',
        'memories.length === 1 ? "memory" : "memories"',
        // Bounded logical identifier from activityModelLabel; never a path or URL.
        'model',
    ],
    "/src/domains/workspace/AutofillOverlay.tsx": [
        'candidateCount',
        'candidateCount === 1 ? "match" : "matches"',
    ],
    "/src/domains/workspace/ControlPanel.tsx": [
        // Both pass through safeIdentifier, a bounded [A-Za-z0-9._-] filter.
        'embeddingBackend',
        'embeddingModel',
        'status.pipeline.skipped_total.toLocaleString()',
        'status.pipeline.stored_total.toLocaleString()',
    ],
    "/src/domains/workspace/EngineMetricsCard.tsx": [
        'aggregateCount',
        'aggregateCount === 1 ? "group" : "groups"',
        'recentCount',
        'recentCount === 1 ? "operation" : "operations"',
        'snap.embedding.degraded ? "degraded" : "standard"',
    ],
    "/src/domains/workspace/FndrWrappedPanel.tsx": [
        'recap.active_days === 1 ? "day" : "days"',
        'recap.active_days.toLocaleString()',
        'recap.total_captures.toLocaleString()',
    ],
    // Catalog display names of downloadable models, not file locations.
    "/src/domains/workspace/ModelDownloadBanner.tsx": ["selected.name"],
    // Byte and percent figures come from the backend download status event.
    "/src/domains/workspace/modelDownloadActivity.ts": [
        "formatBytes(status.bytes_downloaded)",
        "formatBytes(status.total_bytes)",
        "modelName",
        "status.percent.toFixed(0)",
    ],
    "/src/domains/workspace/Onboarding.tsx": ["embedder.name", "granted", "selected.name"],
    "/src/domains/workspace/StatsPanel.tsx": ["snapshot.total_records.toLocaleString()"],
};

/** Non-literal `label:` or `detail:` values, each reviewed as producer copy. */
const ALLOWED_BARE_VALUES: Record<string, string[]> = {
    "/src/domains/ask/AskPanel.tsx": ["label resultLabel", "detail resultDetail"],
    "/src/domains/notch/NotchOperator.tsx": [
        "label input.label",
        "label event.final",
        // Ternary of two fixed strings; the raw event.error is never read here.
        "label event.error",
    ],
    "/src/domains/omnibar/OmnibarApp.tsx": ["label result.label"],
    "/src/domains/screen-guide/screenGuideState.ts": ["label screenGuideActivityCopy(activity)"],
    "/src/domains/search/SearchBar.tsx": ["label requestStartedAt === null"],
    "/src/domains/workspace/AgentWorkspace.tsx": [
        "label labels.start",
        "label labels.success",
        "label labels.failure",
        // A fixed provider display name, not user input.
        "label PROVIDER_LABEL[value]",
    ],
    "/src/domains/workspace/AutofillOverlay.tsx": ["label pendingLabel"],
    "/src/domains/workspace/CodexAccountCard.tsx": ["label input.label"],
    "/src/domains/workspace/ControlPanel.tsx": [
        "label status.ai_model_loaded",
        "detail status.ai_model_loaded",
        "label captureLabel",
    ],
    "/src/domains/workspace/EngineMetricsCard.tsx": ["detail runtimeMetricsRef.current"],
    "/src/domains/workspace/StatsPanel.tsx": ["label slice.label", "label hasLoadedStatsRef.current"],
    "/src/domains/notch/NotchHud.tsx": ["label trimmed"],
};

function interpolations(text: string): string[] {
    const found = new Set<string>();
    for (const match of text.matchAll(/^\s*(?:label|detail):\s*(`(?:[^`\\]|\\.)*`)/gm)) {
        for (const expr of match[1].matchAll(/\$\{([^}]*)\}/g)) found.add(expr[1].trim());
    }
    return [...found].sort();
}

function bareValues(text: string): string[] {
    const found = new Set<string>();
    for (const match of text.matchAll(/^\s*(label|detail):\s*([A-Za-z_][^`"'\n]*?),?\s*$/gm)) {
        if (match[2].startsWith("string")) continue; // type declaration
        found.add(`${match[1]} ${match[2]}`);
    }
    return [...found].sort();
}

describe("activity trace producers", () => {
    it("finds the mounted producers it is meant to guard", () => {
        const paths = producers.map(([path]) => path);
        expect(paths.length).toBeGreaterThanOrEqual(20);
        expect(paths).toContain("/src/shared/hooks/useSearch.ts");
        expect(paths).toContain("/src/domains/workspace/modelDownloadActivity.ts");
    });

    it("interpolates only counts, logical identifiers, and fixed copy into labels and details", () => {
        for (const [path, text] of producers) {
            expect(interpolations(text), path).toEqual([...(ALLOWED_INTERPOLATIONS[path] ?? [])].sort());
        }
    });

    it("assigns non-literal labels and details only from reviewed producer copy", () => {
        for (const [path, text] of producers) {
            expect(bareValues(text), path).toEqual([...(ALLOWED_BARE_VALUES[path] ?? [])].sort());
        }
    });

    it("never reads query, transcript, OCR, prompt, answer, title, URL, path, or raw error fields into a step", () => {
        const stripLiterals = (line: string) => line
            .replace(/"(?:[^"\\]|\\.)*"/g, '""')
            .replace(/`(?:[^`$\\]|\\.|\$(?!\{))*/g, "`")
            .replace(/\}(?:[^`$}\\]|\\.)*`/g, "}`");
        const forbidden = /\b(?:label|detail):\s*[^\n]*\b(?:query|transcript|ocr|prompt|answer|window_title|url|path|reason\.message|err(?:or)?\.message|e\.message)\b/i;
        for (const [path, text] of producers) {
            const offending = text.split("\n").filter((line) => forbidden.test(stripLiterals(line)));
            expect(offending, path).toEqual([]);
        }
    });

    it("would flag a leaking label or detail", () => {
        expect(interpolations("    label: `Results for ${query}`,")).toEqual(["query"]);
        expect(bareValues("    detail: error.message,")).toEqual(["detail error.message"]);
        expect(bareValues('    label: "Fixed copy",')).toEqual([]);
    });
});

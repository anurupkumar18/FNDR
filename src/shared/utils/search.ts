const STOP_WORDS = new Set([
    "a",
    "an",
    "and",
    "are",
    "as",
    "at",
    "be",
    "for",
    "from",
    "how",
    "in",
    "is",
    "it",
    "of",
    "on",
    "or",
    "that",
    "the",
    "this",
    "to",
    "was",
    "what",
    "when",
    "where",
    "who",
    "why",
    "with",
    "open",
    "go",
]);

const SYNONYM_MAP: Record<string, string[]> = {
    ipl: ["cricket"],
    cricket: ["ipl"],
    football: ["soccer"],
    soccer: ["football"],
    auth: ["authentication"],
    authentication: ["auth"],
};

export function normalizeForSearch(value: string): string {
    return value
        .toLowerCase()
        .replace(/[^a-z0-9\s]/g, " ")
        .replace(/\s+/g, " ")
        .trim();
}

export function extractAnchorTerms(query: string): string[] {
    const normalized = normalizeForSearch(query);
    if (!normalized) {
        return [];
    }

    const anchors: string[] = [normalized];
    for (const token of normalized.split(" ")) {
        if (token.length <= 1) {
            continue;
        }
        if (STOP_WORDS.has(token) && !/\d/.test(token)) {
            continue;
        }
        anchors.push(token);
        for (const synonym of SYNONYM_MAP[token] ?? []) {
            anchors.push(synonym);
        }
    }

    const deduped: string[] = [];
    const seen = new Set<string>();
    for (const term of anchors) {
        const normalizedTerm = normalizeForSearch(term);
        if (!normalizedTerm || seen.has(normalizedTerm)) {
            continue;
        }
        seen.add(normalizedTerm);
        deduped.push(normalizedTerm);
        if (deduped.length >= 8) {
            break;
        }
    }

    return deduped;
}

export function scoreAnchorCoverage(text: string, anchorTerms: string[]): number {
    if (anchorTerms.length === 0) {
        return 1;
    }

    const normalizedText = normalizeForSearch(text);
    if (!normalizedText) {
        return 0;
    }

    let matched = 0;
    for (const term of anchorTerms) {
        if (normalizedText.includes(term)) {
            matched += 1;
        }
    }

    return Math.max(0, Math.min(1, matched / anchorTerms.length));
}

export function bubblePurityGate(summary: string, anchorTerms: string[]): { pass: boolean; purity: number } {
    if (!summary.trim()) {
        return { pass: false, purity: 0 };
    }
    if (anchorTerms.length === 0) {
        return { pass: true, purity: 1 };
    }

    const normalizedSummary = normalizeForSearch(summary);
    const tokens = normalizedSummary
        .split(" ")
        .map((token) => token.trim())
        .filter((token) => token.length > 1)
        .filter((token) => !STOP_WORDS.has(token));

    if (tokens.length === 0) {
        return { pass: false, purity: 0 };
    }

    let topicalTokenCount = 0;
    for (const token of tokens) {
        const isTopical = anchorTerms.some((anchor) => token === anchor || token.startsWith(anchor) || anchor.startsWith(token));
        if (isTopical) {
            topicalTokenCount += 1;
        }
    }

    const purity = topicalTokenCount / tokens.length;
    return {
        pass: purity >= 0.4,
        purity,
    };
}

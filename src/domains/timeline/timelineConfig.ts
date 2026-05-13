/**
 * Centralized timeline UI thresholds and pagination (search results / stream).
 * Tune here rather than scattering magic numbers across `Timeline.tsx`.
 */
export const TIMELINE_STREAM = {
    initialVisible: 30,
    loadMoreStep: 30,
} as const;

/** Dedupe consecutive search hits that are almost the same moment + summary. */
export const TIMELINE_DEDUPE = {
    /** Max gap between captures (ms) to consider "consecutive" for deduping. */
    sameAppWindowMs: 30_000,
    /** Jaccard-style token overlap above which summaries count as duplicate. */
    summaryOverlapMax: 0.85,
} as const;

/** Match-quality copy thresholds (aligned with hybrid search scores). */
export const TIMELINE_MATCH_LABEL = {
    lowConfidenceMatchMax: 0.42,
    anchorCoverageDirect: 0.8,
    anchorCoverageRelated: 0.4,
    semanticStrongMin: 0.72,
} as const;

import { describe, expect, it } from "vitest";
import { TIMELINE_DEDUPE, TIMELINE_MATCH_LABEL, TIMELINE_STREAM } from "./timelineConfig";

describe("timelineConfig", () => {
    it("uses sane ordering for match label thresholds", () => {
        expect(TIMELINE_MATCH_LABEL.anchorCoverageRelated).toBeLessThan(
            TIMELINE_MATCH_LABEL.anchorCoverageDirect
        );
        expect(TIMELINE_MATCH_LABEL.lowConfidenceMatchMax).toBeLessThan(0.5);
    });

    it("stream step is positive", () => {
        expect(TIMELINE_STREAM.loadMoreStep).toBeGreaterThan(0);
        expect(TIMELINE_DEDUPE.sameAppWindowMs).toBeGreaterThan(0);
    });
});

import { describe, expect, it } from "vitest";
import type { ModelDownloadStatus } from "@/shared/ipc/onboarding";
import {
    preferNewestModelDownloadStatus,
    type ObservedModelDownloadStatus,
} from "./useModelDownloadStatus";

function observed(
    updatedAtMs: number,
    evidence: ObservedModelDownloadStatus["activity_evidence"],
): ObservedModelDownloadStatus {
    const status: ModelDownloadStatus = {
        state: "downloading",
        model_id: "local-model",
        filename: null,
        download_url: null,
        destination_path: null,
        temp_path: null,
        bytes_downloaded: updatedAtMs,
        total_bytes: 100,
        percent: updatedAtMs,
        done: false,
        error: null,
        logs: [],
        updated_at_ms: updatedAtMs,
    };
    return { ...status, activity_evidence: evidence };
}

describe("preferNewestModelDownloadStatus", () => {
    it("does not let a late older snapshot overwrite a live event", () => {
        const live = observed(20, "backend-event");

        expect(preferNewestModelDownloadStatus(live, observed(10, "backend-snapshot"))).toBe(live);
        expect(preferNewestModelDownloadStatus(live, observed(20, "backend-snapshot"))).toBe(live);
    });

    it("accepts a newer observation and preserves its evidence origin", () => {
        const next = observed(30, "backend-snapshot");
        expect(preferNewestModelDownloadStatus(observed(20, "backend-event"), next)).toBe(next);
        expect(next.activity_evidence).toBe("backend-snapshot");
    });
});

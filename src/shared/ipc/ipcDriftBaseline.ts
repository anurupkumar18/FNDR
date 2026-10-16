/**
 * Dead IPC surface that existed when `ipcDrift.test.ts` was added
 * (2026-10-08). This list may only shrink: delete an entry when the command
 * or wrapper is removed or gains a caller. Never add one to get a green run.
 */
export const baseline = {
    uncalledCommands: [
        "models_cleanup_confirm",
        "models_cleanup_dry_run",
        "reindex_memories_v5"
    ],
    unusedWrappers: [
        "backfillInsightLayersForRange",
        "backfillMemoryReview",
        "cleanDevBuildCache",
        "companionGetEndpoint",
        "companionStartServer",
        "companionStopServer",
        "deleteAllData",
        "deleteOlderThan",
        "dismissNotchHud",
        "evaluateRecentMemoryQuality",
        "fndrSearch",
        "fndrSubscribe",
        "fndrTimeline",
        "fndrUnsubscribe",
        "getAutofillSettings",
        "getContextRuntimeStatus",
        "getMcpServerStatus",
        "getMemoryRepairProgress",
        "getMemoryTimelineThread",
        "getRetentionDays",
        "getStorageHealth",
        "getStorageReclaimProgress",
        "inspectMemoryPipeline",
        "listRecentContextPacks",
        "onContextDelta",
        "rebuildMemoryContextForRange",
        "reclaimMemoryStorage",
        "runDailyMemoryReview",
        "runIdleWikiKnowledgeCompile",
        "runMemoryRepairBackfill",
        "runMemoryRetrievalEval",
        "setAutofillSettings",
        "setRetentionDays",
        "startMcpServer",
        "stopMcpServer",
        "toggleNotchHud"
    ],
};

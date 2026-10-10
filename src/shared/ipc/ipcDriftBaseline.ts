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
        "approveAgentAction",
        "backfillInsightLayersForRange",
        "backfillMemoryReview",
        "buildAgentContextPack",
        "cleanDevBuildCache",
        "companionGetEndpoint",
        "companionStartServer",
        "companionStopServer",
        "deleteAllData",
        "deleteOlderThan",
        "dismissNotchHud",
        "evaluateRecentMemoryQuality",
        "executeAgentAction",
        "explainAgentRetrieval",
        "fndrSearch",
        "fndrSubscribe",
        "fndrTimeline",
        "fndrUnsubscribe",
        "getAgentAuditRun",
        "getAgentPrompt",
        "getAutofillSettings",
        "getContextRuntimeStatus",
        "getMcpServerStatus",
        "getMemoryRepairProgress",
        "getMemoryTimelineThread",
        "getRetentionDays",
        "getStorageHealth",
        "getStorageReclaimProgress",
        "inspectMemoryPipeline",
        "listAgentAuditRuns",
        "listAgentEvalDrafts",
        "listAgentPrompts",
        "listAgentSkillDrafts",
        "listRecentContextPacks",
        "onContextDelta",
        "proposeAgentAction",
        "proposeEvalFromRun",
        "proposeSkillFromRun",
        "rateAgentResult",
        "rebuildMemoryContextForRange",
        "reclaimMemoryStorage",
        "runAgentRequest",
        "runDailyMemoryReview",
        "runIdleWikiKnowledgeCompile",
        "runMemoryRepairBackfill",
        "runMemoryRetrievalEval",
        "setAutofillSettings",
        "setRetentionDays",
        "startMcpServer",
        "stopMcpServer",
        "toggleNotchHud",
        // Proactive signals (2026-10-09): the Home thread view that calls these is
        // the next wave's work and lives in src/app, outside this lane. Delete
        // both lines when it lands.
        "markThreadSeen",
        "whatChangedSince",
        // Named work sets and routines (2026-10-09): the Home card that calls
        // these is being built in src/app by a parallel lane. Delete these five
        // lines when it lands.
        "deleteNamedSet",
        "dismissRoutineOffer",
        "listNamedSets",
        "routineOffers",
        "saveNamedSet"
    ],
};

export type ActivityTraceStatus =
    | "running"
    | "waiting"
    | "completed"
    | "degraded"
    | "failed"
    | "cancelled";

/** Describes which observable boundary produced a step. It deliberately does
 * not claim access to a model's private reasoning. */
export type ActivityTraceEvidence =
    | "backend-event"
    | "backend-snapshot"
    | "frontend-event"
    | "ipc-boundary"
    | "result-metadata";

export interface ActivityTraceStep {
    /** Stable within one trace; a later event with the same id updates it. */
    id: string;
    /** Privacy-safe product copy. Never pass prompts, OCR, titles, or URLs. */
    label: string;
    /** The subsystem that actually emitted or completed this step. */
    actor: string;
    status: ActivityTraceStatus;
    evidence: ActivityTraceEvidence;
    atMs: number;
    durationMs?: number;
    /** Optional bounded metadata such as counts, model id, or reason code. */
    detail?: string;
}

export interface ActivityTraceSnapshot {
    id: string;
    title: string;
    status: ActivityTraceStatus;
    startedAtMs: number;
    finishedAtMs: number | null;
    /** Contains observed events only. Expected future stages are not inserted. */
    steps: ActivityTraceStep[];
}

const MAX_ACTIVITY_TRACE_STEPS = 24;

export function beginActivityTrace(input: {
    id: string;
    title: string;
    startedAtMs?: number;
}): ActivityTraceSnapshot {
    return {
        id: input.id,
        title: input.title,
        status: "running",
        startedAtMs: input.startedAtMs ?? Date.now(),
        finishedAtMs: null,
        steps: [],
    };
}

function isTerminal(status: ActivityTraceStatus): boolean {
    return status === "completed"
        || status === "degraded"
        || status === "failed"
        || status === "cancelled";
}

/** Add or update one fact emitted by the owning workflow. Updating by id lets
 * a real start event become completed/failed without duplicating a fake step. */
export function recordActivityStep(
    trace: ActivityTraceSnapshot,
    step: ActivityTraceStep,
): ActivityTraceSnapshot {
    const existingIndex = trace.steps.findIndex((candidate) => candidate.id === step.id);
    const steps = [...trace.steps];
    if (existingIndex >= 0) {
        // An update is itself the newest observed fact. Move the stable-id step
        // to the end so the summary, snapshot status, and expanded chronology
        // all describe the same current state.
        steps.splice(existingIndex, 1);
        steps.push(step);
    } else {
        steps.push(step);
    }

    const boundedSteps = steps.length > MAX_ACTIVITY_TRACE_STEPS
        ? steps.slice(-MAX_ACTIVITY_TRACE_STEPS)
        : steps;

    return {
        ...trace,
        status: step.status,
        finishedAtMs: isTerminal(step.status) ? step.atMs : null,
        steps: boundedSteps,
    };
}

/**
 * Hand-off progress as an activity trace (ADR 021): one observed worker
 * event becomes one step, composed here in code. Labels and details are fixed
 * copy and counts only: no file names, paths, queries or errors (the result
 * card lists the files; the panel shows the reason). Token and rate-limit
 * counts are not steps.
 */
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStatus,
} from "@/shared/activity/activityTrace";
import type { HandOffEvent, HandOffRecord } from "@/shared/ipc/tauri";

export function beginHandOffTrace(startedAtMs: number): ActivityTraceSnapshot {
    const trace = beginActivityTrace({ id: `hand-off-${startedAtMs}`, title: "Hand-off", startedAtMs });
    return recordActivityStep(trace, {
        id: "started",
        label: "Handed off to the worker",
        actor: "FNDR",
        status: "completed",
        evidence: "ipc-boundary",
        atMs: startedAtMs,
    });
}

function counted(trace: ActivityTraceSnapshot, id: string): number {
    const detail = trace.steps.find((step) => step.id === id)?.detail ?? "";
    return Number.parseInt(detail, 10) || 0;
}


export function applyHandOffEvent(
    trace: ActivityTraceSnapshot,
    event: HandOffEvent,
    atMs: number,
): ActivityTraceSnapshot {
    const worker = { actor: "Worker", evidence: "backend-event" as const, atMs };
    switch (event.kind) {
        case "readFile": {
            const n = counted(trace, "read") + 1;
            return recordActivityStep(trace, {
                ...worker,
                id: "read",
                label: "Read the attached files",
                status: "completed",
                detail: `${n} ${n === 1 ? "read" : "reads"}`,
            });
        }
        case "ranCode": {
            const n = counted(trace, "code") + 1;
            const failedBefore = trace.steps.find((step) => step.id === "code")?.status === "degraded";
            return recordActivityStep(trace, {
                ...worker,
                id: "code",
                label: "Ran analysis code",
                status: failedBefore || !event.ok ? "degraded" : "completed",
                detail: `${n} ${n === 1 ? "run" : "runs"}`,
            });
        }
        case "wroteFile": {
            const made = event.path.startsWith("out/");
            const id = made ? "made" : "work";
            const n = counted(trace, id) + 1;
            return recordActivityStep(trace, {
                ...worker,
                id,
                label: made ? "Wrote what it was asked for" : "Saved its working notes",
                status: "completed",
                detail: `${n} ${n === 1 ? "file" : "files"}`,
            });
        }
        case "viewedImage":
            return recordActivityStep(trace, {
                ...worker,
                id: "viewed",
                label: "Looked over what it made",
                status: "completed",
            });
        case "webSearched": {
            const n = counted(trace, "search") + 1;
            return recordActivityStep(trace, {
                ...worker,
                id: "search",
                label: "Searched the web",
                status: "completed",
                detail: `${n} ${n === 1 ? "search" : "searches"}`,
            });
        }
        case "answered":
            return recordActivityStep(trace, {
                ...worker,
                id: "answered",
                label: "Listed its claims and sources",
                status: "completed",
            });
        default:
            return trace;
    }
}

/** The last step: FNDR's own check, never the worker's account. The reason
 * a job stopped is shown by the panel, not here: it may hold a raw error. */
export function finishHandOffTrace(
    trace: ActivityTraceSnapshot,
    record: HandOffRecord | null,
    atMs: number,
): ActivityTraceSnapshot {
    const status: ActivityTraceStatus = !record
        ? "failed"
        : record.status === "ready"
          ? "completed"
          : record.status === "ready_with_gaps"
            ? "degraded"
            : record.status === "stopped"
              ? "cancelled"
              : "failed";
    const step = { id: "checked", actor: "FNDR", status, evidence: "result-metadata" as const, atMs };
    let next: ActivityTraceSnapshot;
    if (record && (record.status === "ready" || record.status === "ready_with_gaps")) {
        const claims = record.checked + record.unsupported + record.notChecked;
        const checked = record.checked;
        next = recordActivityStep(trace, {
            ...step,
            label: `Checked ${claims} ${claims === 1 ? "claim" : "claims"}: ${checked} with their sources`,
        });
    } else {
        next = recordActivityStep(trace, {
            ...step,
            label: !record ? "Did not start" : record.status === "stopped" ? "Stopped" : "Failed",
        });
    }
    return { ...next, status, finishedAtMs: atMs };
}

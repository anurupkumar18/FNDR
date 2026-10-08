import { useMemo, useState } from "react";
import type {
    ActivityTraceEvidence,
    ActivityTraceSnapshot,
    ActivityTraceStatus,
} from "@/shared/activity/activityTrace";
import "./ActivityTrace.css";

const EVIDENCE_LABEL: Record<ActivityTraceEvidence, string> = {
    "backend-event": "Live backend event",
    "backend-snapshot": "Backend status",
    "frontend-event": "Frontend event",
    "ipc-boundary": "Request boundary",
    "result-metadata": "Verified result",
};

const STATUS_LABEL: Record<ActivityTraceStatus, string> = {
    running: "Running",
    waiting: "Waiting",
    completed: "Completed",
    degraded: "Degraded",
    failed: "Failed",
    cancelled: "Cancelled",
};

function formatDuration(durationMs: number | undefined): string | null {
    if (durationMs === undefined || !Number.isFinite(durationMs) || durationMs < 0) {
        return null;
    }
    if (durationMs < 1_000) return `${Math.round(durationMs)} ms`;
    return `${(durationMs / 1_000).toFixed(durationMs < 10_000 ? 1 : 0)} s`;
}

function validActivityDate(atMs: number): Date | null {
    if (!Number.isFinite(atMs) || atMs <= 0) return null;
    const date = new Date(atMs);
    return Number.isNaN(date.getTime()) ? null : date;
}

function formatTime(atMs: number): string {
    const date = validActivityDate(atMs);
    if (!date) return "Time unavailable";
    return date.toLocaleTimeString([], {
        hour: "numeric",
        minute: "2-digit",
        second: "2-digit",
    });
}

interface ActivityTraceProps {
    trace: ActivityTraceSnapshot;
    className?: string;
    defaultExpanded?: boolean;
    /** Let this trace own polite announcements. Disable when a nearby terminal
     * result or error owns the live-region announcement instead. */
    announce?: boolean;
    /** Disable disclosure controls on passive/click-through surfaces. */
    showDetails?: boolean;
}

/** A compact disclosure for privacy-safe, evidence-backed process activity.
 * Callers must add a step only when its event/boundary actually occurred. */
export function ActivityTrace({
    trace,
    className,
    defaultExpanded = false,
    announce = true,
    showDetails = true,
}: ActivityTraceProps) {
    const [expanded, setExpanded] = useState(defaultExpanded);
    const current = trace.steps[trace.steps.length - 1] ?? null;
    const duration = useMemo(() => {
        if (!current) return null;
        return formatDuration(current.durationMs);
    }, [current]);

    if (!current) return null;

    return (
        <section
            className={[
                "activity-trace",
                !showDetails && "activity-trace--summary-only",
                className,
            ].filter(Boolean).join(" ")}
            data-status={trace.status}
            aria-label={trace.title}
        >
            <div className="activity-trace-summary">
                <div
                    className="activity-trace-announcement"
                    role={announce ? "status" : undefined}
                    aria-live={announce ? "polite" : undefined}
                    aria-atomic={announce ? "true" : undefined}
                >
                    <span className="activity-trace-pulse" aria-hidden="true" />
                    <span className="activity-trace-summary-copy">
                        <span className="activity-trace-current">{current.label}</span>
                        <span className="activity-trace-owner">
                            {current.actor}
                            {duration ? ` · ${duration}` : ""}
                        </span>
                    </span>
                    <span className="activity-trace-state">{STATUS_LABEL[trace.status]}</span>
                </div>
                {showDetails && (
                    <button
                        type="button"
                        className="activity-trace-toggle"
                        aria-expanded={expanded}
                        aria-label={`${expanded ? "Hide" : "Show"} ${trace.title} details`}
                        onClick={() => setExpanded((value) => !value)}
                    >
                        {expanded ? "Hide" : "Details"}
                    </button>
                )}
            </div>

            {showDetails && expanded && (
                <div className="activity-trace-details">
                    <ol className="activity-trace-list">
                        {trace.steps.map((step) => {
                            const stepDuration = formatDuration(step.durationMs);
                            return (
                                <li key={step.id} className="activity-trace-step" data-status={step.status}>
                                    <span className="activity-trace-step-marker" aria-hidden="true" />
                                    <span className="activity-trace-step-copy">
                                        <span className="activity-trace-step-heading">
                                            <strong>{step.label}</strong>
                                            <span className="activity-trace-step-state">
                                                {STATUS_LABEL[step.status]}
                                            </span>
                                        </span>
                                        <span>
                                            {step.actor} · {EVIDENCE_LABEL[step.evidence]}
                                            {stepDuration ? ` · ${stepDuration}` : ""}
                                        </span>
                                        {step.detail && <span>{step.detail}</span>}
                                    </span>
                                    <time dateTime={validActivityDate(step.atMs)?.toISOString()}>
                                        {formatTime(step.atMs)}
                                    </time>
                                </li>
                            );
                        })}
                    </ol>
                    <p className="activity-trace-privacy">
                        No prompts, captured text, or private model reasoning are shown here.
                    </p>
                </div>
            )}
        </section>
    );
}

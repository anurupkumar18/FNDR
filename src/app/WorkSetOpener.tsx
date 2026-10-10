import { useEffect, useRef, useState } from "react";
import { completeTodo, openWorkSet, reopenMemory, type WorkItem, type WorkItemOutcome } from "@/shared/ipc/tauri";
import { asksBeforeOpening, layoutFor, OUTCOME_LABEL, outcomeStatus } from "./homeWorkSet";
import "./WorkSetOpener.css";

export type WorkSetLoad = () => Promise<WorkItem[] | { why: string }>;

interface WorkSetOpenerProps {
    /** Names the work for the button's accessible name: "Open all for Lab 4". */
    name: string;
    /** Finds the items; called only when the person taps Open all. */
    load: WorkSetLoad;
    /** The visible button text. */
    label?: string;
    /** The set came from this task, so "Mark this task done" is offered after it opens. */
    taskId?: string;
    /** The thread's latest source, reopened by "Continue where you left off". */
    continueMemoryId?: string;
    onTaskDone?: () => void;
    /**
     * Arrange the windows side by side. The set then opens in one call so the
     * backend can place every window, so Stop cannot fall between items.
     */
    arrange?: boolean;
}

type Phase =
    | { kind: "idle" }
    | { kind: "finding" }
    | { kind: "none"; why: string }
    | { kind: "confirm"; items: WorkItem[] }
    | { kind: "opening"; items: WorkItem[]; outcomes: WorkItemOutcome[]; stopping: boolean }
    | { kind: "done"; items: WorkItem[]; outcomes: WorkItemOutcome[]; stopped: boolean };

const NOT_OPENED = "FNDR could not open this item.";

/**
 * Opens the places of one piece of work from Home, under the Notch Do rules
 * (ADR 027): nothing opens until a tap, more than three items wait for one
 * more tap, Stop works before and between items, and each item's result is
 * FNDR's typed outcome.
 */
export function WorkSetOpener({ name, load, label = "Open all", taskId, continueMemoryId, onTaskDone, arrange = false }: WorkSetOpenerProps) {
    const [phase, setPhase] = useState<Phase>({ kind: "idle" });
    const [taskState, setTaskState] = useState<"open" | "saving" | "done" | "error">("open");
    const [continueState, setContinueState] = useState<"idle" | "opening" | "opened" | "failed">("idle");
    const stopRef = useRef(false);
    const mountedRef = useRef(true);
    const confirmRef = useRef<HTMLButtonElement>(null);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
            stopRef.current = true;
        };
    }, []);

    useEffect(() => {
        if (phase.kind === "confirm") confirmRef.current?.focus();
    }, [phase.kind]);

    async function run(items: WorkItem[]) {
        stopRef.current = false;
        const outcomes: WorkItemOutcome[] = [];
        setPhase({ kind: "opening", items, outcomes, stopping: false });
        if (arrange) {
            try {
                const rows = await openWorkSet(items.map((item) => item.memoryId), layoutFor(items.length));
                outcomes.push(...items.map((item) => (
                    rows.find((row) => row.memoryId === item.memoryId) ?? { memoryId: item.memoryId, label: item.label, ok: false, detail: NOT_OPENED }
                )));
            } catch {
                outcomes.push(...items.map((item) => ({ memoryId: item.memoryId, label: item.label, ok: false, detail: NOT_OPENED })));
            }
            if (mountedRef.current) setPhase({ kind: "done", items, outcomes, stopped: false });
            return;
        }
        for (const item of items) {
            if (stopRef.current) break;
            let outcome: WorkItemOutcome;
            try {
                outcome = (await openWorkSet([item.memoryId]))[0] ?? { memoryId: item.memoryId, label: item.label, ok: false, detail: NOT_OPENED };
            } catch {
                outcome = { memoryId: item.memoryId, label: item.label, ok: false, detail: NOT_OPENED };
            }
            outcomes.push(outcome);
            if (!mountedRef.current) return;
            setPhase((current) => current.kind === "opening" ? { ...current, outcomes: [...outcomes] } : current);
        }
        if (!mountedRef.current) return;
        setPhase({ kind: "done", items, outcomes, stopped: outcomes.length < items.length });
    }

    async function start() {
        setTaskState("open");
        setContinueState("idle");
        setPhase({ kind: "finding" });
        try {
            const found = await load();
            if (!mountedRef.current) return;
            if (!Array.isArray(found)) {
                setPhase({ kind: "none", why: found.why });
            } else if (found.length === 0) {
                setPhase({ kind: "none", why: "Nothing saved to reopen for this yet." });
            } else if (asksBeforeOpening(found.length)) {
                setPhase({ kind: "confirm", items: found });
            } else {
                await run(found);
            }
        } catch {
            if (mountedRef.current) setPhase({ kind: "none", why: "FNDR could not look this up. Try again." });
        }
    }

    function stop() {
        stopRef.current = true;
        setPhase((current) => current.kind === "opening" ? { ...current, stopping: true } : current);
    }

    async function markDone() {
        if (!taskId) return;
        setTaskState("saving");
        try {
            await completeTodo(taskId);
            setTaskState("done");
            onTaskDone?.();
        } catch {
            setTaskState("error");
        }
    }

    async function continueWork() {
        if (!continueMemoryId) return;
        setContinueState("opening");
        try {
            const outcome = await reopenMemory(continueMemoryId);
            setContinueState(["opened", "opened_moved", "app_only"].includes(outcome.kind) ? "opened" : "failed");
        } catch {
            setContinueState("failed");
        }
    }

    const busy = phase.kind === "finding" || phase.kind === "opening";
    const arrangement = phase.kind === "done" ? phase.outcomes.find((outcome) => outcome.arrangement)?.arrangement : undefined;

    return (
        <div className="work-set">
            <button
                type="button"
                className="work-set-open"
                aria-label={`${label} for ${name}`}
                disabled={busy || phase.kind === "confirm"}
                onClick={() => void start()}
            >
                {label}
            </button>
            {phase.kind === "finding" && <p className="work-set-note" role="status">Finding what to reopen…</p>}
            {phase.kind === "none" && <p className="work-set-note" role="status">{phase.why}</p>}
            {phase.kind === "confirm" && (
                <div className="work-set-panel">
                    <p className="work-set-note">These will open, one after another:</p>
                    <ul className="work-set-items">
                        {phase.items.map((item) => (
                            <li key={item.memoryId}>
                                <span className="work-set-label">{item.label || item.appName}</span>
                                {item.appName && item.label && <span className="work-set-detail">{item.appName}</span>}
                            </li>
                        ))}
                    </ul>
                    <div className="work-set-actions">
                        <button ref={confirmRef} type="button" className="work-set-primary" onClick={() => void run(phase.items)}>
                            Open {phase.items.length} items
                        </button>
                        <button type="button" onClick={() => setPhase({ kind: "idle" })}>Cancel</button>
                    </div>
                </div>
            )}
            {(phase.kind === "opening" || phase.kind === "done") && (
                <div className="work-set-panel">
                    <p className="work-set-note" role="status">
                        {phase.kind === "opening"
                            ? arrange
                                ? `Opening ${phase.items.length} together to arrange them…`
                                : phase.stopping
                                ? "Stopping after this item…"
                                : `Opening ${Math.min(phase.outcomes.length + 1, phase.items.length)} of ${phase.items.length}…`
                            : phase.stopped
                              ? `Stopped after ${phase.outcomes.length} of ${phase.items.length}`
                              : `Opened ${phase.outcomes.filter((outcome) => outcome.ok).length} of ${phase.items.length}${arrangement?.arranged ? ", arranged side by side" : ""}`}
                    </p>
                    {arrangement && !arrangement.arranged && <p className="work-set-note">{arrangement.detail}</p>}
                    <ul className="work-set-items">
                        {phase.items.map((item, index) => {
                            const outcome = phase.outcomes[index];
                            const status = outcome ? outcomeStatus(outcome) : null;
                            const waiting = !outcome && phase.kind === "opening";
                            return (
                                <li key={item.memoryId} data-status={status ?? (waiting ? "waiting" : "stopped")}>
                                    <span className="work-set-label">{outcome?.label || item.label || item.appName}</span>
                                    <span className="work-set-status">
                                        {status ? OUTCOME_LABEL[status] : waiting ? "Waiting" : "Not opened, stopped"}
                                    </span>
                                    {outcome && !outcome.ok && <span className="work-set-detail">{outcome.detail}</span>}
                                </li>
                            );
                        })}
                    </ul>
                    <div className="work-set-actions">
                        {phase.kind === "opening" && !arrange && (
                            <button type="button" onClick={stop} disabled={phase.stopping}>Stop</button>
                        )}
                        {phase.kind === "done" && taskId && taskState !== "done" && (
                            <button type="button" className="work-set-primary" disabled={taskState === "saving"} onClick={() => void markDone()}>
                                Mark this task done
                            </button>
                        )}
                        {phase.kind === "done" && taskState === "done" && <span className="work-set-note" role="status">Marked done</span>}
                        {phase.kind === "done" && taskState === "error" && <span className="work-set-note" role="alert">The task could not be marked done. Try again.</span>}
                        {phase.kind === "done" && continueMemoryId && (
                            <button type="button" disabled={continueState === "opening"} onClick={() => void continueWork()}>
                                Continue where you left off
                            </button>
                        )}
                        {continueState === "opened" && <span className="work-set-note" role="status">Opened the latest source</span>}
                        {continueState === "failed" && <span className="work-set-note" role="alert">The latest source could not be opened.</span>}
                        {phase.kind === "done" && <button type="button" onClick={() => setPhase({ kind: "idle" })}>Close</button>}
                    </div>
                </div>
            )}
        </div>
    );
}

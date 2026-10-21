/**
 * Hand-off (ADR 032): the draft card, the job while it works, and the result
 * FNDR checked. Nothing leaves the Mac until the person taps Hand off on a
 * draft whose every item is listed.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import type { ActivityTraceSnapshot } from "@/shared/activity/activityTrace";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import {
    HAND_OFF_FINISHED_EVENT,
    HAND_OFF_PROGRESS_EVENT,
    handOffDraft,
    handOffInstallToolchain,
    handOffOpen,
    handOffRate,
    handOffStart,
    handOffStatus,
    handOffStop,
    handOffUpdateDraft,
    type HandOffDraft,
    type HandOffEvent,
    type HandOffFinished,
    type HandOffGoal,
    type HandOffRecord,
    type HandOffSource,
    type HandOffStatus,
} from "@/shared/ipc/tauri";
import { applyHandOffEvent, beginHandOffTrace, finishHandOffTrace } from "./handOffTrace";
import "./HandOffPanel.css";

const GOALS: { goal: HandOffGoal; label: string }[] = [
    { goal: "doc", label: "Doc" },
    { goal: "spreadsheet", label: "Spreadsheet" },
    { goal: "slides", label: "Slides" },
    { goal: "brief", label: "Brief" },
];

type Phase =
    | { kind: "loading" }
    | { kind: "draft" }
    | { kind: "working"; startedAtMs: number }
    | { kind: "finished"; record: HandOffRecord | null; error: string | null };

function size(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function elapsed(ms: number): string {
    const seconds = Math.max(0, Math.floor(ms / 1000));
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function headline(record: HandOffRecord | null): string {
    if (!record) return "It did not start";
    switch (record.status) {
        case "ready":
            return "Ready";
        case "ready_with_gaps":
            return "Ready, with gaps";
        case "stopped":
            return "Stopped";
        default:
            return "It did not finish";
    }
}

interface HandOffPanelProps {
    source: HandOffSource;
    onClose: () => void;
}

export function HandOffPanel({ source, onClose }: HandOffPanelProps) {
    const [phase, setPhase] = useState<Phase>({ kind: "loading" });
    const [status, setStatus] = useState<HandOffStatus | null>(null);
    const [draft, setDraft] = useState<HandOffDraft | null>(null);
    const [problem, setProblem] = useState<string | null>(null);
    const [busy, setBusy] = useState<"updating" | "installing" | "starting" | "stopping" | null>(null);
    const [trace, setTrace] = useState<ActivityTraceSnapshot | null>(null);
    const [now, setNow] = useState(() => Date.now());
    const [rated, setRated] = useState<boolean | null>(null);
    const dialogRef = useRef<HTMLElement>(null);
    const closeRef = useRef<HTMLButtonElement>(null);
    // Layered over To-dos: this dialog owns Escape and Tab while it is open.
    useModalFocus(true, dialogRef, closeRef, onClose);

    // Keyed on the source's parts: a parent passes a new object on every
    // render, and reloading would throw away the person's choices.
    const { kind: sourceKind, value: sourceValue } = source;
    useEffect(() => {
        let alive = true;
        Promise.all([handOffStatus(), handOffDraft({ kind: sourceKind, value: sourceValue } as HandOffSource)])
            .then(([current, built]) => {
                if (!alive) return;
                setStatus(current);
                setDraft(built);
                if (current.running) {
                    setTrace(beginHandOffTrace(current.running.createdAtMs));
                    setPhase({ kind: "working", startedAtMs: current.running.createdAtMs });
                } else {
                    setPhase({ kind: "draft" });
                }
            })
            .catch((error: unknown) => {
                if (!alive) return;
                setPhase({ kind: "finished", record: null, error: error instanceof Error ? error.message : String(error) });
            });
        return () => {
            alive = false;
        };
    }, [sourceKind, sourceValue]);

    useEffect(() => {
        if (phase.kind !== "working") return;
        const timer = window.setInterval(() => setNow(Date.now()), 1000);
        return () => window.clearInterval(timer);
    }, [phase.kind]);

    useTauriEvent<HandOffEvent>(HAND_OFF_PROGRESS_EVENT, (event) => {
        setTrace((current) => (current ? applyHandOffEvent(current, event, Date.now()) : current));
    });
    useTauriEvent<HandOffFinished>(HAND_OFF_FINISHED_EVENT, ({ record, error }) => {
        const at = Date.now();
        setTrace((current) => finishHandOffTrace(current ?? beginHandOffTrace(at), record, at));
        setPhase({ kind: "finished", record, error });
        setBusy(null);
    });

    const ticked = useMemo(() => {
        const items = draft?.items ?? [];
        return {
            memoryIds: items.filter((i) => i.kind === "memory" && i.ticked).map((i) => i.memoryId),
            fileMemoryIds: items.filter((i) => i.kind === "file" && i.ticked).map((i) => i.memoryId),
        };
    }, [draft]);

    const redraft = useCallback(
        async (goal: HandOffGoal, memoryIds: string[], fileMemoryIds: string[]) => {
            if (!draft) return;
            setBusy("updating");
            setProblem(null);
            try {
                setDraft(
                    await handOffUpdateDraft({
                        taskText: draft.taskText,
                        taskMemoryId: draft.taskMemoryId,
                        goal,
                        memoryIds,
                        fileMemoryIds,
                    }),
                );
            } catch (error) {
                setProblem(error instanceof Error ? error.message : String(error));
            } finally {
                setBusy(null);
            }
        },
        [draft],
    );

    const toggle = (kind: "memory" | "file", memoryId: string) => {
        if (!draft) return;
        const flip = (ids: string[]) => (ids.includes(memoryId) ? ids.filter((id) => id !== memoryId) : [...ids, memoryId]);
        void redraft(
            draft.goal,
            kind === "memory" ? flip(ticked.memoryIds) : ticked.memoryIds,
            kind === "file" ? flip(ticked.fileMemoryIds) : ticked.fileMemoryIds,
        );
    };

    const install = async () => {
        setBusy("installing");
        setProblem(null);
        try {
            setStatus(await handOffInstallToolchain());
        } catch (error) {
            setProblem(error instanceof Error ? error.message : String(error));
        } finally {
            setBusy(null);
        }
    };

    const start = async () => {
        if (!draft) return;
        setBusy("starting");
        setProblem(null);
        try {
            await handOffStart({
                taskText: draft.taskText,
                taskMemoryId: draft.taskMemoryId,
                goal: draft.goal,
                memoryIds: ticked.memoryIds,
                fileMemoryIds: ticked.fileMemoryIds,
                reviewed: draft.rendered,
            });
            const startedAtMs = Date.now();
            setTrace(beginHandOffTrace(startedAtMs));
            setNow(startedAtMs);
            setPhase({ kind: "working", startedAtMs });
        } catch (error) {
            setProblem(error instanceof Error ? error.message : String(error));
        } finally {
            setBusy(null);
        }
    };

    const stop = async () => {
        setBusy("stopping");
        try {
            await handOffStop();
        } catch (error) {
            setProblem(error instanceof Error ? error.message : String(error));
            setBusy(null);
        }
    };

    const ready = status?.toolchain === "installed";
    const memories = draft?.items.filter((i) => i.kind === "memory") ?? [];
    const files = draft?.items.filter((i) => i.kind === "file") ?? [];

    return (
        <div className="hand-off-backdrop" role="presentation">
            <section
                ref={dialogRef}
                className="hand-off-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="hand-off-title"
                tabIndex={-1}
            >
                <header className="hand-off-header">
                    <h2 id="hand-off-title">Hand off</h2>
                    <button ref={closeRef} type="button" className="hand-off-close" onClick={onClose} aria-label="Close hand-off">
                        ×
                    </button>
                </header>

                {phase.kind === "loading" && <p className="hand-off-note">Finding what this needs on your Mac…</p>}

                {phase.kind === "draft" && draft && (
                    <div className="hand-off-body">
                        <blockquote className="hand-off-task">“{draft.taskText}”</blockquote>
                        {draft.taskMemoryId && <p className="hand-off-note">Quoted from a to-do FNDR noticed.</p>}

                        <div className="hand-off-goals" role="group" aria-label="Make">
                            <span>Make</span>
                            {GOALS.map(({ goal, label }) => (
                                <button
                                    key={goal}
                                    type="button"
                                    className="hand-off-goal"
                                    aria-pressed={draft.goal === goal}
                                    disabled={busy !== null}
                                    onClick={() => void redraft(goal, ticked.memoryIds, ticked.fileMemoryIds)}
                                >
                                    {label}
                                </button>
                            ))}
                        </div>

                        <h3 className="hand-off-subhead">Sends to OpenAI on your ChatGPT plan</h3>
                        {memories.length + files.length === 0 ? (
                            <p className="hand-off-note">Only the task. FNDR found no memories or files for it.</p>
                        ) : (
                            <ul className="hand-off-items" aria-label="What would be sent">
                                {[...memories, ...files].map((item) => (
                                    <li key={`${item.kind}:${item.memoryId}`}>
                                        <label>
                                            <input
                                                type="checkbox"
                                                checked={item.ticked}
                                                disabled={busy !== null}
                                                onChange={() => toggle(item.kind, item.memoryId)}
                                            />
                                            <span className="hand-off-item-label">{item.label}</span>
                                            <span className="hand-off-item-kind">
                                                {item.kind === "file" ? `file · ${size(item.bytes ?? 0)}` : "memory"}
                                            </span>
                                        </label>
                                    </li>
                                ))}
                            </ul>
                        )}
                        <p className="hand-off-note">
                            It works in Documents › FNDR Hand-offs. It can search the web. It cannot open apps, read
                            other files, send anything or reach FNDR. Up to 15 minutes. About {size(draft.estimateBytes)} to
                            start.
                        </p>

                        {!ready && (
                            <div className="hand-off-install">
                                <p>Hand-off needs FNDR's document tools (Python and its document libraries, about 200 MB).</p>
                                <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => void install()}>
                                    {busy === "installing" ? "Installing…" : "Install document tools"}
                                </button>
                            </div>
                        )}
                        {problem && <p className="hand-off-problem" role="alert">{problem}</p>}

                        <footer className="hand-off-actions">
                            <button type="button" className="ui-action-btn" onClick={onClose}>
                                Not now
                            </button>
                            <button
                                type="button"
                                className="ui-action-btn hand-off-primary"
                                disabled={!ready || busy !== null}
                                onClick={() => void start()}
                            >
                                {busy === "starting" ? "Starting…" : "Hand off"}
                            </button>
                        </footer>
                    </div>
                )}

                {phase.kind === "working" && (
                    <div className="hand-off-body">
                        <p className="hand-off-working" aria-live="off">
                            Working · {elapsed(now - phase.startedAtMs)}
                        </p>
                        {trace && <ActivityTrace trace={trace} defaultExpanded />}
                        {problem && <p className="hand-off-problem" role="alert">{problem}</p>}
                        <footer className="hand-off-actions">
                            <button type="button" className="ui-action-btn" disabled={busy === "stopping"} onClick={() => void stop()}>
                                {busy === "stopping" ? "Stopping…" : "Stop"}
                            </button>
                        </footer>
                    </div>
                )}

                {phase.kind === "finished" && (
                    <div className="hand-off-body">
                        <h3 className="hand-off-result">{headline(phase.record)}</h3>
                        {phase.record && (phase.record.status === "ready" || phase.record.status === "ready_with_gaps") && (
                            <>
                                <ul className="hand-off-files" aria-label="What it made">
                                    {phase.record.deliverables.map((path) => (
                                        <li key={path}>
                                            <span>{path.replace(/^out\//, "")}</span>
                                            <button
                                                type="button"
                                                className="ui-action-btn"
                                                onClick={() => void handOffOpen(phase.record!.id, path)}
                                            >
                                                Open
                                            </button>
                                        </li>
                                    ))}
                                </ul>
                                <p className="hand-off-note">
                                    {phase.record.checked} of{" "}
                                    {phase.record.checked + phase.record.unsupported + phase.record.notChecked} claims
                                    checked against their sources by FNDR.
                                    {phase.record.unsupported > 0 && ` ${phase.record.unsupported} had no source.`}
                                    {phase.record.notChecked > 0 && ` ${phase.record.notChecked} cited a source FNDR could not confirm.`}
                                </p>
                            </>
                        )}
                        {(phase.record?.reason ?? phase.error) && (
                            <p className="hand-off-problem">{phase.record?.reason ?? phase.error}</p>
                        )}
                        {phase.record && (
                            <p className="hand-off-note">
                                Sent {size(phase.record.bytesSent)} to chatgpt.com. Folder: {phase.record.folder}
                            </p>
                        )}
                        {trace && <ActivityTrace trace={trace} announce={false} />}
                        <footer className="hand-off-actions">
                            {phase.record && rated === null && (
                                <>
                                    <span className="hand-off-note">Did that work?</span>
                                    <button
                                        type="button"
                                        className="ui-action-btn"
                                        onClick={() => void handOffRate(phase.record!.id, true).then(() => setRated(true))}
                                    >
                                        Yes
                                    </button>
                                    <button
                                        type="button"
                                        className="ui-action-btn"
                                        onClick={() => void handOffRate(phase.record!.id, false).then(() => setRated(false))}
                                    >
                                        No
                                    </button>
                                </>
                            )}
                            {rated !== null && <span className="hand-off-note">Thanks, noted.</span>}
                            <button type="button" className="ui-action-btn" onClick={onClose}>
                                Close
                            </button>
                        </footer>
                    </div>
                )}
            </section>
        </div>
    );
}

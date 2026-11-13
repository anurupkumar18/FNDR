import { useEffect, useRef, useState } from "react";
import {
    addTodo,
    investigate,
    investigateStop,
    openWorkSet,
    INVESTIGATE_STEP_EVENT,
    type CaseFile,
    type InvestigateStep,
    type InvestigateStepEvent,
    type WorkItemOutcome,
} from "@/shared/ipc/tauri";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import { PanelState } from "@/shared/components/PanelState";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";
import "./Investigation.css";

const ACTOR = "Investigate";
/** More places than this wait for one more tap, as a work set does (ADR 027). */
const OPENS_WITHOUT_ASKING = 3;
/** Timeline rows shown before "Show all". */
const TIMELINE_PREVIEW = 6;
/** Next steps shown before "Show all". */
const OPEN_PREVIEW = 5;

const INQUIRY_NAME = { decisions: "decisions", open: "open items", problems: "problems" } as const;

function plural(count: number, one: string, many = `${one}s`): string {
    return `${count} ${count === 1 ? one : many}`;
}

/** A step as a line of the trace: what ran and counts, never memory text. */
export function stepCopy(step: InvestigateStep): { label: string; detail?: string } {
    const running = step.status === "running";
    if (step.note === "tool_failed") {
        return { label: "A step could not read memory", detail: "The investigation went on without it" };
    }
    const routes = step.routes.length > 0 ? ` · ${step.routes.join(" + ")}` : "";
    switch (step.kind) {
        case "search":
        case "widen": {
            const widened = step.kind === "widen";
            if (running) return { label: widened ? "Widening the search" : "Searching memory" };
            if (step.note === "weak_match") {
                return {
                    label: widened ? "Widened search found no strong match" : "First search found no strong match",
                    detail: `${plural(step.found, "weak result")}${routes}`,
                };
            }
            return {
                label: widened ? "Widened search found the work" : "Searched memory",
                detail: `${step.found} found, ${step.added} kept${routes}`,
            };
        }
        case "thread":
            if (running) return { label: "Looking for the work thread" };
            if (step.note === "no_thread") return { label: "No work thread joins these memories" };
            if (step.note === "nothing_new") {
                return { label: "The work thread was already covered", detail: plural(step.found, "memory", "memories") };
            }
            return {
                label: "Followed the work thread",
                detail: `${plural(step.found, "memory", "memories")} in the thread, ${step.added} new`,
            };
        case "probe": {
            const what = step.inquiry ? INQUIRY_NAME[step.inquiry] : "more";
            if (running) return { label: `Probing for ${what}` };
            if (step.note) return { label: `Probe for ${what} found nothing new` };
            return { label: `Probed for ${what}`, detail: `${step.added} new${routes}` };
        }
        case "links":
            if (running) return { label: "Following stored links" };
            return { label: "Followed stored links", detail: `${step.added} new` };
        case "todos":
            if (running) return { label: "Reading open to-dos" };
            return { label: "Read open to-dos", detail: `${step.added} tied to this work` };
        case "check":
            if (running) return { label: "Checking every line against its memory" };
            return {
                label: "Checked the case",
                detail: `${plural(step.total, "memory", "memories")}, ${plural(step.added, "line")}, ${step.marked} recorded as done`,
            };
    }
}

function shortDay(ms: number): string {
    return new Date(ms).toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" });
}

function dayAndTime(ms: number): string {
    return `${shortDay(ms)} · ${new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
}

function span(file: CaseFile): string {
    const first = shortDay(file.stats.firstMs);
    const last = shortDay(file.stats.lastMs);
    return first === last ? first : `${first} to ${last}`;
}

/** The case file as text a person can paste into a note or an assistant. */
export function caseFileMarkdown(file: CaseFile): string {
    const lines = [
        `# ${file.title}`,
        "",
        `${plural(file.stats.memories, "memory", "memories")} from ${file.stats.apps.join(", ")}, ${span(file)}. Compiled by FNDR on this Mac.`,
    ];
    const section = (title: string, rows: string[]) => {
        if (rows.length > 0) lines.push("", `## ${title}`, ...rows.map((row) => `- ${row}`));
    };
    section("Next steps on record", file.open.map((item) => item.text));
    section(
        "Recorded as done",
        file.done.map((item) => `${item.text} (later, ${shortDay(item.doneAtMs)}: "${item.clause}")`),
    );
    section("Decisions", file.decisions.map((item) => `${item.text} (${shortDay(item.atMs)})`));
    section("Problems seen", file.problems.map((item) => item.text));
    section("What happened", file.timeline.map((entry) => `${shortDay(entry.atMs)}, ${entry.app}: ${entry.line}`));
    section("Places", file.places.map((place) => `${place.label} (${place.appName})`));
    return lines.join("\n");
}

type RunState =
    | { kind: "running" }
    | { kind: "file"; file: CaseFile }
    | { kind: "error" };

type OpenState =
    | { kind: "idle" }
    | { kind: "confirm" }
    | { kind: "opening" }
    | { kind: "done"; outcomes: WorkItemOutcome[] }
    | { kind: "failed" };

interface InvestigationProps {
    goal: string;
    onOpenMemoryById: (id: string) => void;
    /** Back to the question box. */
    onDismiss: () => void;
}

let runCounter = 0;

/**
 * One investigation (ADR 039): runs the loop for `goal`, shows each step as
 * its event arrives, then the case file. The run only reads; places open and
 * to-dos are added on a tap.
 */
export function Investigation({ goal, onOpenMemoryById, onDismiss }: InvestigationProps) {
    const runId = useRef(`investigate-${Date.now()}-${++runCounter}`).current;
    const [state, setState] = useState<RunState>({ kind: "running" });
    const [trace, setTrace] = useState<ActivityTraceSnapshot>(() =>
        beginActivityTrace({ id: runId, title: "Investigation steps" }),
    );
    const [stopping, setStopping] = useState(false);
    const [opening, setOpening] = useState<OpenState>({ kind: "idle" });
    const [added, setAdded] = useState<Record<string, "saving" | "added" | "failed">>({});
    const [copied, setCopied] = useState<"idle" | "copied" | "failed">("idle");
    const [showAll, setShowAll] = useState(false);
    const [showAllOpen, setShowAllOpen] = useState(false);

    const record = (step: InvestigateStep) => {
        const copy = stepCopy(step);
        setTrace((current) =>
            recordActivityStep(current, {
                id: `step-${step.index}`,
                label: copy.label,
                actor: ACTOR,
                status: step.status,
                evidence: "backend-event",
                atMs: Date.now(),
                durationMs: step.status === "running" ? undefined : step.ms,
                detail: copy.detail,
            }),
        );
    };

    useTauriEvent<InvestigateStepEvent>(INVESTIGATE_STEP_EVENT, (event) => {
        if (event.runId === runId) record(event);
    });

    useEffect(() => {
        let live = true;
        investigate(goal, runId)
            .then((file) => {
                if (!live) return;
                // The file carries every step, so a step whose event was
                // missed (a listener that attached late) still shows.
                setTrace((current) => {
                    let next = current;
                    for (const step of file.steps) {
                        const copy = stepCopy(step);
                        next = recordActivityStep(next, {
                            id: `step-${step.index}`,
                            label: copy.label,
                            actor: ACTOR,
                            status: step.status,
                            evidence: "result-metadata",
                            atMs: Date.now(),
                            durationMs: step.ms,
                            detail: copy.detail,
                        });
                    }
                    return next;
                });
                setState({ kind: "file", file });
            })
            .catch(() => {
                if (live) setState({ kind: "error" });
            });
        return () => {
            live = false;
            void investigateStop(runId).catch(() => undefined);
        };
        // One run per mount: the panel remounts this component for a new goal.
    }, []);

    const stop = () => {
        setStopping(true);
        void investigateStop(runId).catch(() => undefined);
    };

    const openPlaces = async (file: CaseFile) => {
        setOpening({ kind: "opening" });
        try {
            const outcomes = await openWorkSet(file.places.map((place) => place.memoryId));
            setOpening({ kind: "done", outcomes });
        } catch {
            setOpening({ kind: "failed" });
        }
    };

    const addStep = async (text: string) => {
        setAdded((current) => ({ ...current, [text]: "saving" }));
        try {
            await addTodo(text, "Todo");
            setAdded((current) => ({ ...current, [text]: "added" }));
        } catch {
            setAdded((current) => ({ ...current, [text]: "failed" }));
        }
    };

    const copy = async (file: CaseFile) => {
        try {
            await navigator.clipboard.writeText(caseFileMarkdown(file));
            setCopied("copied");
        } catch {
            setCopied("failed");
        }
        window.setTimeout(() => setCopied("idle"), 2000);
    };

    const running = state.kind === "running";
    const file = state.kind === "file" ? state.file : null;
    const found = file?.outcome === "found" ? file : null;

    return (
        <section className="inv" aria-label="Investigation">
            <header className="inv-head">
                <div className="inv-head-copy">
                    <p className="inv-kicker">{running ? "Investigating on this Mac" : "Investigation"}</p>
                    <h3 className="inv-goal">{goal}</h3>
                </div>
                {running ? (
                    <button type="button" className="ui-action-btn inv-stop" onClick={stop} disabled={stopping}>
                        {stopping ? "Stopping" : "Stop"}
                    </button>
                ) : (
                    <button type="button" className="ui-action-btn inv-stop" onClick={onDismiss}>
                        New question
                    </button>
                )}
            </header>

            {trace.steps.length > 0 && (
                <ActivityTrace
                    // Open while the run works; folded once the case file is the news.
                    key={running ? "live" : "settled"}
                    trace={trace}
                    className="inv-trace"
                    defaultExpanded={running}
                    announce={running}
                />
            )}

            {running && trace.steps.length === 0 && (
                <PanelState kind="loading" title="Starting the investigation" description="Reading the request." announce={false} />
            )}

            {state.kind === "error" && (
                <PanelState
                    kind="error"
                    title="The investigation could not run"
                    description="FNDR could not read its memories just now. Try again in a moment."
                    action={{ label: "Back", onAction: onDismiss }}
                />
            )}

            {file?.outcome === "nothing_found" && (
                <PanelState
                    kind="empty"
                    title="Nothing in memory matches"
                    description={`FNDR ran ${plural(file.steps.length, "search", "searches")} and found no strong match. It only reports what it captured on this Mac.`}
                    action={{ label: "Ask something else", onAction: onDismiss }}
                />
            )}
            {file?.outcome === "needs_topic" && (
                <PanelState
                    kind="empty"
                    title="Name the work to investigate"
                    description="An investigation follows one piece of work: a project, a document or a ticket. For a day or a week, open FNDR Review."
                    action={{ label: "Try again", onAction: onDismiss }}
                />
            )}
            {file?.outcome === "private" && (
                <PanelState
                    kind="empty"
                    title="FNDR is private right now"
                    description="Private Mode is on, so FNDR does not look through memories."
                    action={{ label: "Back", onAction: onDismiss }}
                />
            )}
            {file?.outcome === "cancelled" && (
                <PanelState
                    kind="empty"
                    title="Stopped"
                    description="The investigation stopped before its next step. Nothing was changed."
                    action={{ label: "Back", onAction: onDismiss }}
                />
            )}

            {found && (
                <article className="case" aria-label={`Case file: ${found.title}`}>
                    <header className="case-head">
                        <p className="case-kicker">Case file</p>
                        <h3 className="case-title">{found.title}</h3>
                        <p className="case-stats">
                            {plural(found.stats.memories, "memory", "memories")} · {found.stats.apps.join(", ")} · {span(found)}
                        </p>
                        <dl className="case-counts">
                            <div><dt>Next steps</dt><dd>{found.open.length}</dd></div>
                            <div><dt>Recorded as done</dt><dd>{found.done.length}</dd></div>
                            <div><dt>Decisions</dt><dd>{found.decisions.length}</dd></div>
                            <div><dt>Problems</dt><dd>{found.problems.length}</dd></div>
                        </dl>
                    </header>

                    <div className="case-actions">
                        {found.places.length > 0 && opening.kind !== "confirm" && (
                            <button
                                type="button"
                                className="ui-action-btn btn-primary"
                                disabled={opening.kind === "opening"}
                                onClick={() =>
                                    found.places.length > OPENS_WITHOUT_ASKING
                                        ? setOpening({ kind: "confirm" })
                                        : void openPlaces(found)
                                }
                            >
                                {opening.kind === "opening" ? "Opening" : `Open ${plural(found.places.length, "place")}`}
                            </button>
                        )}
                        {opening.kind === "confirm" && (
                            <>
                                <button type="button" className="ui-action-btn btn-primary" onClick={() => void openPlaces(found)}>
                                    Open all {found.places.length}
                                </button>
                                <button type="button" className="ui-action-btn" onClick={() => setOpening({ kind: "idle" })}>
                                    Not now
                                </button>
                            </>
                        )}
                        <button type="button" className="ui-action-btn" onClick={() => void copy(found)}>
                            {copied === "copied" ? "Copied" : copied === "failed" ? "Could not copy" : "Copy case file"}
                        </button>
                    </div>
                    {opening.kind === "failed" && (
                        <p className="case-note" role="alert">FNDR could not open these places.</p>
                    )}
                    {opening.kind === "done" && (
                        <ul className="case-outcomes" aria-label="What opened">
                            {opening.outcomes.map((outcome) => (
                                <li key={outcome.memoryId} data-ok={outcome.ok}>
                                    <span>{outcome.label || "A place"}</span>
                                    <span>{outcome.ok ? "Opened" : outcome.detail}</span>
                                </li>
                            ))}
                        </ul>
                    )}

                    {found.open.length > 0 && (
                        <section className="case-section" aria-label="Next steps on record">
                            <h4>Next steps on record</h4>
                            <p className="case-explain">No later memory records these as done. Newest first.</p>
                            <ul>
                                {(showAllOpen ? found.open : found.open.slice(0, OPEN_PREVIEW)).map((item) => {
                                    const todoState = added[item.text];
                                    return (
                                        <li key={`${item.todoId ?? item.memoryId}-${item.text}`} className="case-row">
                                            {item.memoryId ? (
                                                <button type="button" className="case-line" onClick={() => onOpenMemoryById(item.memoryId!)}>
                                                    <span className="case-text">{item.text}</span>
                                                    <span className="case-meta">
                                                        {item.todoId ? "Your to-do · " : ""}{item.app} · {shortDay(item.atMs)}
                                                    </span>
                                                </button>
                                            ) : (
                                                <div className="case-line case-line--plain">
                                                    <span className="case-text">{item.text}</span>
                                                    <span className="case-meta">Your to-do</span>
                                                </div>
                                            )}
                                            {!item.todoId && (
                                                <button
                                                    type="button"
                                                    className="ui-action-btn case-add"
                                                    disabled={todoState === "saving" || todoState === "added"}
                                                    aria-label={`Add to To-dos: ${item.text}`}
                                                    onClick={() => void addStep(item.text)}
                                                >
                                                    {todoState === "added" ? "Added" : todoState === "failed" ? "Try again" : "Add to To-dos"}
                                                </button>
                                            )}
                                        </li>
                                    );
                                })}
                            </ul>
                            {found.open.length > OPEN_PREVIEW && (
                                <button type="button" className="ui-action-btn case-more" onClick={() => setShowAllOpen((value) => !value)}>
                                    {showAllOpen ? `Show the newest ${OPEN_PREVIEW}` : `Show all ${found.open.length}`}
                                </button>
                            )}
                        </section>
                    )}

                    {found.done.length > 0 && (
                        <section className="case-section" aria-label="Recorded as done">
                            <h4>Recorded as done</h4>
                            <p className="case-explain">
                                A later memory says each of these happened. FNDR shows the words it found; it does not decide for you.
                            </p>
                            <ul>
                                {found.done.map((item) => (
                                    <li key={`${item.memoryId}-${item.text}`} className="case-row case-row--done">
                                        <button type="button" className="case-line" onClick={() => onOpenMemoryById(item.doneMemoryId)}>
                                            <span className="case-text case-text--done">{item.text}</span>
                                            <span className="case-quote">“{item.clause}”</span>
                                            <span className="case-meta">{item.doneApp} · {dayAndTime(item.doneAtMs)}</span>
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}

                    {found.decisions.length > 0 && (
                        <section className="case-section" aria-label="Decisions">
                            <h4>Decisions</h4>
                            <ul>
                                {found.decisions.map((item) => (
                                    <li key={`${item.memoryId}-${item.text}`} className="case-row">
                                        <button type="button" className="case-line" onClick={() => onOpenMemoryById(item.memoryId)}>
                                            <span className="case-text">{item.text}</span>
                                            <span className="case-meta">
                                                {item.app} · {shortDay(item.atMs)}{item.also > 0 ? ` · in ${item.also + 1} memories` : ""}
                                            </span>
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}

                    {found.problems.length > 0 && (
                        <section className="case-section" aria-label="Problems seen">
                            <h4>Problems seen</h4>
                            <ul>
                                {found.problems.map((item) => (
                                    <li key={`${item.memoryId}-${item.text}`} className="case-row">
                                        <button type="button" className="case-line" onClick={() => onOpenMemoryById(item.memoryId)}>
                                            <span className="case-text">{item.text}</span>
                                            <span className="case-meta">{item.app} · {shortDay(item.atMs)}</span>
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}

                    {found.timeline.length > 0 && (
                        <section className="case-section" aria-label="What happened">
                            <h4>What happened</h4>
                            <ol className="case-timeline">
                                {(showAll ? found.timeline : found.timeline.slice(-TIMELINE_PREVIEW)).map((entry) => (
                                    <li key={entry.memoryId} className="case-row">
                                        <button type="button" className="case-line" onClick={() => onOpenMemoryById(entry.memoryId)}>
                                            <span className="case-meta">{dayAndTime(entry.atMs)} · {entry.app}</span>
                                            <span className="case-text">{entry.line}</span>
                                        </button>
                                    </li>
                                ))}
                            </ol>
                            {found.timeline.length > TIMELINE_PREVIEW && (
                                <button type="button" className="ui-action-btn case-more" onClick={() => setShowAll((value) => !value)}>
                                    {showAll ? `Show the latest ${TIMELINE_PREVIEW}` : `Show all ${found.timeline.length}`}
                                </button>
                            )}
                        </section>
                    )}

                    {found.places.length > 0 && (
                        <section className="case-section" aria-label="Places">
                            <h4>Places</h4>
                            <ul className="case-places">
                                {found.places.map((place) => (
                                    <li key={place.memoryId}>
                                        <span className="case-text">{place.label}</span>
                                        <span className="case-meta">{place.host ?? place.appName}</span>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}
                </article>
            )}
        </section>
    );
}

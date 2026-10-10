/**
 * Notch Do's state: listening, the plan card, the step list, approvals and the
 * result. A pure reducer over voice inputs and `computer-use://event`s, so the
 * notch only renders it (ADR-020 amendment, 2026-10-06).
 */

import type {
    ComputerUseEvent,
    ComputerUseStepAction,
    WorkItem,
    WorkItemOutcome,
    WorkSet,
    WorkSetResolution,
} from "@/shared/ipc/tauri";

/** Quiet after the last partial text that ends an utterance. */
export const ENDPOINT_MS = 1200;
/** No speech at all after listening starts. */
export const SILENCE_MS = 8000;
/** The plan card starts the run by itself after this long. */
export const AUTO_START_MS = 1500;

/** What was heard stays on screen this long before it is sent to be planned. */
export const HEARD_MS = 1200;

/** A work set of more than this many places waits for Start (ADR 027;
 *  `MAX_AUTO_REOPENS` in computer_use.rs). */
export const MAX_AUTO_REOPENS = 3;

export type DoPhase =
    | "idle"
    | "listening"
    | "heard"
    | "silence"
    | "mic_denied"
    | "voice_unavailable"
    | "planning"
    | "choose"
    | "plan"
    | "running"
    | "finished"
    | "failed"
    | "stopped";

export type StepStatus = "pending" | "running" | "done" | "failed";

export interface DoStep {
    label: string;
    action: ComputerUseStepAction;
    app: string;
    status: StepStatus;
    attempt: number;
    detail?: string;
    /** FNDR saw this step's result itself; false means the model reported it. */
    checked?: boolean;
    /** The place a `reopen_memory` step opens. */
    item?: WorkItem;
}

export interface DoAction {
    id: string;
    summary: string;
    state: "running" | "done" | "failed" | "blocked";
}

export interface DoState {
    phase: DoPhase;
    partial: string;
    transcript: string;
    runId: string | null;
    steps: DoStep[];
    current: number | null;
    /** The most recent actions of the current step, newest last. */
    actions: DoAction[];
    approval: { requestKey: string; summary: string } | null;
    /** Speech heard mid-run, waiting for "go" or a tap before it replaces the run. */
    redirect: string | null;
    result: { ok: boolean; summary: string } | null;
    error: string | null;
    /** The ChatGPT sign-in has to be redone. */
    reconnect: boolean;
    usedMemories: number;
    /** The plan on the card may start by itself; otherwise it waits for a tap or "go". */
    autoStart: boolean;
    /** Work sets that matched about equally, waiting for the person to pick one. */
    options: WorkSet[];
    /** A picked work set: FNDR opens it itself when the card starts. */
    workSet: WorkSet | null;
}

export const initialDoState: DoState = {
    phase: "idle",
    partial: "",
    transcript: "",
    runId: null,
    steps: [],
    autoStart: false,
    current: null,
    actions: [],
    approval: null,
    redirect: null,
    result: null,
    error: null,
    reconnect: false,
    usedMemories: 0,
    options: [],
    workSet: null,
};

export type DoInput =
    | { type: "listening" }
    | { type: "partial"; text: string }
    | { type: "silence" }
    | { type: "micDenied"; message: string }
    | { type: "voiceUnavailable"; message: string }
    | { type: "heard"; text: string }
    | { type: "planRequested"; runId: string; transcript: string }
    | { type: "redirectHeard"; text: string }
    | { type: "redirectDismissed" }
    | { type: "event"; event: ComputerUseEvent }
    | { type: "workSetChosen"; set: WorkSet }
    | { type: "workSetResolved"; resolution: WorkSetResolution }
    | { type: "workSetOpening" }
    | { type: "workSetOpened"; outcomes: WorkItemOutcome[] }
    | { type: "stopped" }
    | { type: "error"; message: string };

const MAX_ACTIONS = 4;

function updateStep(steps: DoStep[], index: number, patch: Partial<DoStep>): DoStep[] {
    return steps.map((step, i) => (i === index ? { ...step, ...patch } : step));
}

function pushAction(actions: DoAction[], action: DoAction): DoAction[] {
    return [...actions.filter((a) => a.id !== action.id), action].slice(-MAX_ACTIONS);
}

function applyEvent(state: DoState, event: ComputerUseEvent): DoState {
    if (event.runId !== state.runId) return state;
    switch (event.kind) {
        case "planning":
            return { ...state, phase: "planning", usedMemories: event.usedMemories };
        case "planned":
            return {
                ...state,
                phase: "plan",
                autoStart: event.autoStart === true,
                steps: event.steps.map((step) => ({ ...step, status: "pending", attempt: 0 })),
            };
        case "stepStarted":
            return {
                ...state,
                phase: "running",
                current: event.index,
                actions: event.attempt === 1 ? [] : state.actions,
                steps: updateStep(state.steps, event.index, { status: "running", attempt: event.attempt }),
            };
        case "action":
            return { ...state, actions: pushAction(state.actions, { id: event.itemId, summary: event.summary, state: "running" }) };
        case "actionDone":
            return {
                ...state,
                actions: state.actions.map((a) => (a.id === event.itemId ? { ...a, state: event.ok ? "done" : "failed" } : a)),
            };
        case "blocked":
            return {
                ...state,
                actions: pushAction(state.actions, {
                    id: `blocked-${state.actions.length}-${event.tool}`,
                    summary: event.summary,
                    state: "blocked",
                }),
            };
        case "approval":
            return { ...state, approval: { requestKey: event.requestKey, summary: event.summary } };
        case "approvalResolved":
            return state.approval?.requestKey === event.requestKey ? { ...state, approval: null } : state;
        case "stepDone":
            return {
                ...state,
                steps: updateStep(state.steps, event.index, {
                    status: event.ok ? "done" : "failed",
                    detail: event.detail,
                    checked: event.checked === true,
                }),
            };
        case "finished":
            return { ...state, phase: "finished", approval: null, current: null, result: { ok: event.ok, summary: event.summary } };
        case "stopped":
            return { ...state, phase: "stopped", approval: null, runId: null, current: null };
        case "choose":
            return { ...state, phase: "choose", options: event.options, steps: [], workSet: null };
        case "failed":
            return { ...state, phase: "failed", approval: null, error: event.error, reconnect: event.reconnect };
    }
}

/** The plan card for a picked work set: every place, listed. */
function chooseSet(state: DoState, set: WorkSet): DoState {
    return {
        ...state,
        phase: "plan",
        options: [],
        workSet: set,
        current: null,
        autoStart: set.items.length <= MAX_AUTO_REOPENS,
        steps: set.items.map((item) => ({
            label: `Open ${item.label}`,
            action: "reopen_memory",
            app: item.appName,
            status: "pending",
            attempt: 0,
            item,
        })),
    };
}

/** The same summary a work set run on the Mac ends with. */
export function workSetSummary(outcomes: WorkItemOutcome[]): string {
    const opened = outcomes.filter((o) => o.ok).map((o) => o.label);
    const missed = outcomes.filter((o) => !o.ok).map((o) => `${o.label} (${o.detail})`);
    if (missed.length === 0) return `Opened: ${opened.join(", ")}.`;
    if (opened.length === 0) return `Nothing opened: ${missed.join("; ")}.`;
    return `Opened: ${opened.join(", ")}. Not opened: ${missed.join("; ")}.`;
}

export function doRunReducer(state: DoState, input: DoInput): DoState {
    switch (input.type) {
        case "listening":
            return state.phase === "running" || state.phase === "plan" || state.phase === "planning" || state.phase === "choose"
                ? state
                : { ...initialDoState, phase: "listening" };
        case "partial":
            return { ...state, partial: input.text };
        case "silence":
            return { ...state, phase: "silence", partial: "" };
        case "micDenied":
            return { ...state, phase: "mic_denied", partial: "", error: input.message };
        case "voiceUnavailable":
            return { ...state, phase: "voice_unavailable", partial: "", error: input.message };
        case "heard":
            // Shown before anything leaves the Mac, so a misheard request can be stopped.
            return { ...initialDoState, phase: "heard", transcript: input.text };
        case "planRequested":
            return {
                ...initialDoState,
                phase: "planning",
                runId: input.runId,
                transcript: input.transcript,
            };
        case "redirectHeard":
            return { ...state, partial: "", redirect: input.text };
        case "redirectDismissed":
            return { ...state, redirect: null };
        case "event":
            return applyEvent(state, input.event);
        case "workSetChosen":
            return chooseSet(state, input.set);
        case "workSetResolved": {
            const resolution = input.resolution;
            if (resolution.kind === "best") return chooseSet(state, resolution.value);
            if (resolution.kind === "ambiguous") return { ...state, phase: "choose", partial: "", options: resolution.value };
            return { ...state, phase: "failed", options: [], error: resolution.value.why };
        }
        case "workSetOpening":
            return {
                ...state,
                phase: "running",
                steps: state.steps.map((step) => ({ ...step, status: "running", attempt: 1 })),
            };
        case "workSetOpened": {
            const steps = state.steps.map((step) => {
                const outcome = input.outcomes.find((o) => o.memoryId === step.item?.memoryId);
                if (!outcome) return { ...step, status: "pending" as const };
                return { ...step, status: outcome.ok ? ("done" as const) : ("failed" as const), detail: outcome.detail, checked: outcome.ok };
            });
            return {
                ...state,
                phase: "finished",
                steps,
                workSet: null,
                result: { ok: input.outcomes.every((o) => o.ok), summary: workSetSummary(input.outcomes) },
            };
        }
        case "stopped":
            return { ...state, phase: "stopped", approval: null, redirect: null, runId: null, current: null, partial: "", options: [], workSet: null };
        case "error":
            return { ...state, phase: "failed", error: input.message };
    }
}

/** What a finished run can honestly claim: how many steps FNDR saw for
 *  itself, and that nothing it did is undone automatically. */
export function resultNote(steps: DoStep[]): string {
    const done = steps.filter((step) => step.status === "done");
    if (done.length === 0) return "";
    const checked = done.filter((step) => step.checked).length;
    const reported = done.length - checked;
    const seen =
        reported === 0
            ? "FNDR checked every step."
            : checked === 0
              ? "FNDR could not check these steps; they are as the model reported."
              : `FNDR checked ${checked} of ${done.length} steps; the rest are as the model reported.`;
    return `${seen} Nothing here is undone automatically.`;
}

// MARK: - What a spoken phrase means

export type UtteranceIntent =
    | { kind: "choose"; index: number }
    | { kind: "refine"; text: string }
    | { kind: "stop" }
    | { kind: "go" }
    | { kind: "decline" }
    | { kind: "request"; text: string };

const STOP_PHRASES = ["stop", "cancel", "never mind", "nevermind", "wait", "hold on", "pause", "abort"];
const GO_PHRASES = ["go", "go ahead", "start", "do it", "yes", "yeah", "yep", "ok", "okay", "sure", "run it"];
const YES_PHRASES = ["yes", "yeah", "yep", "yup", "sure", "ok", "okay", "go ahead", "do it", "confirm", "allow", "please do"];
const NO_PHRASES = ["no", "nope", "don't", "do not", "deny", "skip", "not that", "no thanks"];

function normalize(text: string): string {
    return text
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s']/gu, " ")
        .replace(/\s+/g, " ")
        .trim();
}

function matchesShortPhrase(normalized: string, phrases: string[]): boolean {
    // Short commands only: "stop" or "okay do it", not a sentence that merely
    // contains the word ("stop at the second tab and open settings").
    if (normalized.split(" ").length > 4) return false;
    return phrases.some((phrase) => normalized === phrase || normalized.startsWith(`${phrase} `));
}

/** Words people put in front of "stop" without changing what they mean. */
const STOP_LEAD_INS = ["please", "ok", "okay", "hey", "no", "just", "now"];

function saysStop(normalized: string): boolean {
    const words = normalized.split(" ");
    while (words.length > 1 && STOP_LEAD_INS.includes(words[0])) words.shift();
    return matchesShortPhrase(normalized, STOP_PHRASES) || matchesShortPhrase(words.join(" "), STOP_PHRASES);
}

/** Whether partial text already says stop, so a run can be killed before the utterance ends. */
export function isStopPhrase(text: string): boolean {
    return saysStop(normalize(text));
}

const ORDINALS: string[][] = [
    ["1", "one", "first"],
    ["2", "two", "second", "to", "too"],
    ["3", "three", "third"],
];

const NAME_FILLER = new Set(["the", "one", "that", "this", "with", "open", "number", "option", "please", "about"]);

/** Which offered work set the person named: by number ("two", "the second
 *  one") or by a word of its title ("the biology one"). Null when unclear. */
export function matchChoice(text: string, options: { title: string }[]): number | null {
    const said = normalize(text).split(" ").filter(Boolean);
    // "the second one", "the biology one": a trailing "one" is not a number.
    const words = said.filter(
        (word, i) => !(word === "one" && i > 0 && i === said.length - 1 && !["number", "option"].includes(said[i - 1])),
    );
    if (words.length === 0) return null;
    if (words.length <= 4) {
        const byNumber = ORDINALS.findIndex(
            (names, index) => index < options.length && words.some((word) => names.includes(word) && !(word === "to" && words.length > 1)),
        );
        if (byNumber >= 0) return byNumber;
    }
    const named = words.filter((word) => word.length >= 4 && !NAME_FILLER.has(word));
    const scores = options.map((option) => {
        const title = new Set(normalize(option.title).split(" "));
        return named.filter((word) => title.has(word)).length;
    });
    const best = Math.max(0, ...scores);
    if (best === 0 || scores.filter((score) => score === best).length > 1) return null;
    return scores.indexOf(best);
}

export function classifyUtterance(
    text: string,
    context: { awaitingApproval?: boolean; awaitingStart?: boolean; running?: boolean; choices?: { title: string }[] },
): UtteranceIntent | null {
    const normalized = normalize(text);
    if (!normalized) return null;
    if (saysStop(normalized)) return { kind: "stop" };
    if (context.choices && context.choices.length > 0) {
        const index = matchChoice(text, context.choices);
        return index === null ? { kind: "refine", text: text.trim() } : { kind: "choose", index };
    }
    if (context.awaitingApproval) {
        if (matchesShortPhrase(normalized, NO_PHRASES)) return { kind: "decline" };
        // A yes must be a tap: the microphone hears music, video and other people too.
        if (matchesShortPhrase(normalized, YES_PHRASES)) return null;
    }
    if (context.awaitingStart && matchesShortPhrase(normalized, GO_PHRASES)) return { kind: "go" };
    return { kind: "request", text: text.trim() };
}

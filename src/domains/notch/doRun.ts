/**
 * Notch Do's state: listening, the plan card, the step list, approvals and the
 * result. A pure reducer over voice inputs and `computer-use://event`s, so the
 * notch only renders it (ADR-020 amendment, 2026-10-06).
 *
 * `turnOf` names the turn-taking state of docs/product/voice-ux.md, and
 * `micFor` what the microphone may do in it. From the end of a request until
 * FNDR has finished speaking the result the notch is deaf: it hears only the
 * stop word, and the reducer drops any request or partial text (ADR 020
 * amendment, 2026-10-09).
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

/** How long the "Working on it" cue shows, and how soon it may show again. */
export const CUE_MS = 2500;
export const CUE_REPEAT_MS = 6000;

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
    /** FNDR's own voice is playing. */
    speaking: boolean;
    /** The result has been read out (or there was nothing to read). */
    readoutDone: boolean;
    /** "Working on it. Say stop to interrupt." is showing. */
    cue: boolean;
    lastCueAt: number | null;
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
    result: null,
    error: null,
    reconnect: false,
    usedMemories: 0,
    options: [],
    workSet: null,
    speaking: false,
    readoutDone: false,
    cue: false,
    lastCueAt: null,
};

export type DoInput =
    | { type: "listening" }
    | { type: "partial"; text: string }
    | { type: "silence" }
    | { type: "micDenied"; message: string }
    | { type: "voiceUnavailable"; message: string }
    | { type: "heard"; text: string }
    | { type: "planRequested"; runId: string; transcript: string }
    | { type: "event"; event: ComputerUseEvent }
    | { type: "workSetChosen"; set: WorkSet }
    | { type: "workSetResolved"; resolution: WorkSetResolution }
    | { type: "workSetOpening" }
    | { type: "workSetOpened"; outcomes: WorkItemOutcome[] }
    | { type: "stopped" }
    | { type: "error"; message: string }
    | { type: "speechStarted" }
    | { type: "speechEnded" }
    | { type: "readoutDone" }
    | { type: "ignoredSpeech"; at: number }
    | { type: "cueEnded" };

/** The turn-taking states of docs/product/voice-ux.md. */
export type TurnState =
    | "idle"
    | "listening"
    | "hearing"
    | "thinking"
    | "awaiting_choice"
    | "awaiting_start"
    | "working"
    | "awaiting_approval"
    | "finishing"
    | "speaking"
    | "interrupted"
    | "error";

/** closed: nothing is captured. open: full recognition. stop_only: the
 *  helper's stop-word spotter, which lets no text out. */
export type MicMode = "closed" | "open" | "stop_only";

export function turnOf(state: DoState): TurnState {
    switch (state.phase) {
        case "idle":
            return "idle";
        case "listening":
            return state.partial ? "hearing" : "listening";
        case "heard":
        case "planning":
            return "thinking";
        case "choose":
            return "awaiting_choice";
        case "plan":
            return state.autoStart ? "working" : "awaiting_start";
        case "running":
            return state.approval ? "awaiting_approval" : "working";
        case "finished":
            if (state.readoutDone) return "idle";
            return state.speaking ? "speaking" : "finishing";
        case "stopped":
            return "interrupted";
        case "failed":
        case "silence":
        case "mic_denied":
        case "voice_unavailable":
            return "error";
    }
}

const DEAF: ReadonlySet<TurnState> = new Set([
    "thinking",
    "awaiting_choice",
    "awaiting_start",
    "working",
    "awaiting_approval",
    "finishing",
    "speaking",
]);

/** From the end of the request until the result has been spoken. */
export function isDeaf(turn: TurnState): boolean {
    return DEAF.has(turn);
}

export function micFor(turn: TurnState): MicMode {
    if (turn === "listening" || turn === "hearing") return "open";
    return isDeaf(turn) ? "stop_only" : "closed";
}

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
    const deaf = isDeaf(turnOf(state));
    switch (input.type) {
        case "listening":
            return state.phase === "running" || state.phase === "plan" || state.phase === "planning" || state.phase === "choose"
                ? state
                : { ...initialDoState, phase: "listening" };
        case "partial":
            // Nothing heard while deaf is shown, routed or kept.
            return deaf ? state : { ...state, partial: input.text };
        case "silence":
            return { ...state, phase: "silence", partial: "" };
        case "micDenied":
            return { ...state, phase: "mic_denied", partial: "", error: input.message };
        case "voiceUnavailable":
            return { ...state, phase: "voice_unavailable", partial: "", error: input.message };
        case "heard":
            if (deaf) return state;
            // Shown before anything leaves the Mac, so a misheard request can be stopped.
            return { ...initialDoState, phase: "heard", transcript: input.text };
        case "planRequested":
            return {
                ...initialDoState,
                phase: "planning",
                runId: input.runId,
                transcript: input.transcript,
            };
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
            return { ...state, phase: "stopped", approval: null, runId: null, current: null, partial: "", options: [], workSet: null, cue: false };
        case "error":
            return { ...state, phase: "failed", error: input.message, cue: false };
        case "speechStarted":
            return state.speaking ? state : { ...state, speaking: true };
        case "speechEnded":
            return state.speaking ? { ...state, speaking: false } : state;
        case "readoutDone":
            return state.phase === "finished" && !state.readoutDone ? { ...state, readoutDone: true, speaking: false } : state;
        case "ignoredSpeech":
            if (!deaf) return state;
            if (state.lastCueAt !== null && input.at - state.lastCueAt < CUE_REPEAT_MS) return state;
            return { ...state, cue: true, lastCueAt: input.at };
        case "cueEnded":
            return state.cue ? { ...state, cue: false } : state;
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

export type UtteranceIntent = { kind: "stop" } | { kind: "request"; text: string };

function normalize(text: string): string {
    return text
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s']/gu, " ")
        .replace(/\s+/g, " ")
        .trim();
}

/** The spotter's vocabulary (voice-ux.md, "Matching rule"); kept in step with
 *  `StopWordMatcher` in `fndr-speech` and `is_stop_word` in `voice/mod.rs`. */
const STOP_WORDS = ["stop", "cancel"];
const STOP_TAILS = ["it", "that", "now"];
const STOP_LEAD_INS = [["hey", "fndr"], ["fndr"], ["please"], ["okay"], ["ok"], ["no"]];
const MAX_STOP_WORDS = 4;

/** `[lead-in] (stop | cancel) [it | that | now] [please]`, at most four words.
 *  "Wait", "pause", "hold on", "never mind" and "abort" are not stop words. */
export function isStopWord(text: string): boolean {
    let words = normalize(text).split(" ").filter(Boolean);
    if (words.length === 0 || words.length > MAX_STOP_WORDS) return false;
    const lead = STOP_LEAD_INS.find((lead) => lead.every((word, i) => words[i] === word));
    if (lead) words = words.slice(lead.length);
    if (!STOP_WORDS.includes(words[0] ?? "")) return false;
    words = words.slice(1);
    if (STOP_TAILS.includes(words[0] ?? "")) words = words.slice(1);
    if (words[0] === "please") words = words.slice(1);
    return words.length === 0;
}

/** What a final transcript means while the microphone is open: the stop word,
 *  or a request. Spoken go, yes, no and option names mean nothing; Start, a
 *  choice and an approval are taps or keys (ADR 020 amendment, 2026-10-09). */
export function classifyUtterance(text: string): UtteranceIntent | null {
    const normalized = normalize(text);
    if (!normalized) return null;
    if (isStopWord(normalized)) return { kind: "stop" };
    if (IGNORED.has(normalized)) return null;
    return { kind: "request", text: text.trim() };
}

/** Short replies that once meant something and now must not become a request. */
const IGNORED = new Set([
    "go",
    "go ahead",
    "start",
    "do it",
    "okay do it",
    "yes",
    "yeah",
    "yep",
    "yup",
    "ok",
    "okay",
    "sure",
    "allow",
    "confirm",
    "no",
    "nope",
    "don't",
    "deny",
    "skip",
    "wait",
    "hold on",
    "pause",
    "never mind",
    "nevermind",
    "abort",
]);

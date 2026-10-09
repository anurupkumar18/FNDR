/**
 * What Notch Do says aloud, as a pure function of two consecutive states
 * (ADR-020 amendment, 2026-10-08). The lines are plain and short, and they
 * keep the difference between a step FNDR checked and one the model only
 * reported. Speech never decides anything: an approval is still a tap.
 */

import { isStopPhrase, type DoState } from "./doRun";

export interface Narration {
    text: string;
    /** Interrupts whatever is being said; otherwise it waits behind it. */
    urgent: boolean;
}

const MAX_DETAIL_CHARS = 140;

function short(text: string, max = MAX_DETAIL_CHARS): string {
    const flat = text.replace(/\s+/g, " ").trim();
    return flat.length > max ? `${flat.slice(0, max - 1).trimEnd()}…` : flat;
}

function sentence(text: string): string {
    return short(text).replace(/[.!?…\s]+$/u, "");
}

const NUMBER_WORDS = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];

/** Spelled out; `isEchoOfSpeech` reads a recognizer's digits as the same words. */
function num(count: number): string {
    return NUMBER_WORDS[count] ?? String(count);
}

function plural(count: number, noun: string): string {
    return `${num(count)} ${noun}${count === 1 ? "" : "s"}`;
}

function checkedNote(steps: DoState["steps"]): string {
    const done = steps.filter((step) => step.status === "done");
    if (done.length === 0) return "";
    const checked = done.filter((step) => step.checked).length;
    if (checked === done.length) return "I checked every step myself.";
    if (checked === 0) return "I could not check these steps; they are as the model reported.";
    return `I checked ${num(checked)} of ${num(done.length)} steps myself; the rest are as the model reported.`;
}

function leftOut(steps: DoState["steps"]): string {
    const pending = steps.filter((step) => step.status === "pending");
    return pending.length === 0 ? "" : `I left out: ${pending.map((step) => sentence(step.label)).join(", ")}.`;
}

function planned(next: DoState): Narration {
    const heard = sentence(next.transcript);
    const lead = `${heard ? `Understood: ${heard}. ` : ""}${plural(next.steps.length, "step")}.`;
    return {
        text: next.autoStart ? `${lead} Starting.` : `${lead} Some steps may ask first. Tap Start when you are ready.`,
        urgent: false,
    };
}

function finished(next: DoState): Narration {
    const result = next.result;
    const parts = [
        result?.ok ? "Done." : "I could not finish.",
        result?.summary ? `${sentence(result.summary)}.` : "",
        leftOut(next.steps),
        checkedNote(next.steps),
        next.steps.some((step) => step.status === "done") ? "Nothing is undone automatically." : "",
    ];
    return { text: parts.filter(Boolean).join(" "), urgent: true };
}

/** The line to say now that `next` follows `prev`, or null for silence. */
export function narrate(prev: DoState, next: DoState): Narration | null {
    if (next.phase === "plan" && prev.phase !== "plan" && next.steps.length > 0) return planned(next);

    if (next.approval && next.approval.requestKey !== prev.approval?.requestKey) {
        return { text: `I need your okay to ${sentence(next.approval.summary)}. Tap Allow, or say no.`, urgent: true };
    }

    const blocked = next.actions.find((a) => a.state === "blocked" && !prev.actions.some((p) => p.id === a.id));
    if (blocked) {
        return { text: `I left this out: ${sentence(blocked.summary)}. FNDR does not do that.`, urgent: true };
    }

    const current = next.current;
    if (next.phase === "running" && current !== null) {
        const attempt = next.steps[current]?.attempt ?? 0;
        if (current !== prev.current || attempt !== (prev.steps[current]?.attempt ?? 0)) {
            if (attempt > 1) return { text: "Trying that step again.", urgent: false };
            return { text: `Step ${num(current + 1)} of ${num(next.steps.length)}: ${sentence(next.steps[current].label)}.`, urgent: false };
        }
    }

    const failedStep = next.steps.findIndex((step, i) => step.status === "failed" && prev.steps[i]?.status !== "failed");
    if (failedStep >= 0) {
        const detail = next.steps[failedStep].detail;
        return { text: `That step did not work${detail ? `: ${sentence(detail)}` : ""}.`, urgent: false };
    }

    if (next.phase === "finished" && prev.phase !== "finished") return finished(next);

    if (next.phase === "failed" && (prev.phase !== "failed" || prev.error !== next.error)) {
        return { text: `I could not do that.${next.error ? ` ${short(next.error)}` : ""}`, urgent: true };
    }

    return null;
}

function words(text: string): string[] {
    return text
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s']/gu, " ")
        .split(/\s+/)
        .filter(Boolean)
        .map((word) => NUMBER_WORDS[Number(word)] ?? word);
}

/** Whether heard text is only the notch's own voice coming back through the
 *  microphone. A stop is never an echo: Stop must work while FNDR talks. */
export function isEchoOfSpeech(heard: string, spoken: string): boolean {
    const heardWords = words(heard);
    if (heardWords.length === 0 || isStopPhrase(heard)) return false;
    const spokenWords = new Set(words(spoken));
    return spokenWords.size > 0 && heardWords.every((word) => spokenWords.has(word));
}

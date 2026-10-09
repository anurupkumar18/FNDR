/**
 * What Notch Do says aloud, as a pure function of two consecutive states
 * (ADR-020 amendment, 2026-10-08). The lines are plain and short, and they
 * keep the difference between a step FNDR checked and one the model only
 * reported. Speech never decides anything: an approval is still a tap.
 */

import type { WorkItem } from "@/shared/ipc/tauri";
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

/** A site's name as people say it: `canvas.utah.edu` is Canvas. */
function siteName(host: string): string {
    const labels = host.toLowerCase().replace(/^www\./, "").split(".");
    const known = ["canvas", "notion", "github", "figma", "overleaf", "gradescope", "piazza", "youtube"];
    const name = labels.find((label) => known.includes(label)) ?? labels[Math.max(0, labels.length - 2)] ?? host;
    if (name === "github") return "GitHub";
    if (name === "youtube") return "YouTube";
    return name.charAt(0).toUpperCase() + name.slice(1);
}

/** How the notch names a place it opens: "your Canvas page", "the PDF on page 4", "the doc". */
export function spokenItem(item: WorkItem): string {
    switch (item.kind) {
        case "pdf_page":
            return item.page ? `the PDF on page ${item.page}` : "the PDF";
        case "url": {
            const host = item.host ?? "";
            if (host === "docs.google.com") return "the doc";
            return host ? `your ${siteName(host)} page` : "the page";
        }
        case "file":
            return `the file ${sentence(item.label)}`;
        case "folder":
            return `the ${sentence(item.label)} folder`;
        case "app":
            return sentence(item.label || item.appName);
    }
}

/** "a", "a and b", "a, b and c". */
function listed(parts: string[]): string {
    if (parts.length <= 1) return parts.join("");
    return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

function plannedReopens(next: DoState): Narration {
    const places = listed(next.steps.map((step) => (step.item ? spokenItem(step.item) : sentence(step.label))));
    return {
        text: next.autoStart
            ? `Opening ${places}.`
            : `${plural(next.steps.length, "place")} to open: ${places}. Tap Start when you are ready.`,
        urgent: false,
    };
}

function choices(next: DoState): Narration {
    const named = next.options.map((option, index) => `${num(index + 1)}, ${sentence(option.title)}`).join("; ");
    return {
        text: `That could be ${plural(next.options.length, "piece")} of work: ${named}. Tap one or say its number.`,
        urgent: false,
    };
}

function planned(next: DoState): Narration {
    if (next.steps.every((step) => step.action === "reopen_memory")) return plannedReopens(next);
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
    if (next.phase === "choose" && next.options.length > 0 && next.options !== prev.options) return choices(next);

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

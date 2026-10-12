import { useSyncExternalStore } from "react";
import { STORAGE_KEYS } from "@/shared/utils/config";

export type HomeSectionId = "routine" | "deadlines" | "threads" | "sets";

export interface HomeSectionInfo {
    id: HomeSectionId;
    label: string;
    hint: string;
}

/** What Home can show below the search, in the order it shows them. */
export const HOME_SECTIONS: readonly HomeSectionInfo[] = [
    { id: "routine", label: "Routine offers", hint: "A set FNDR offers to open when your usual time comes." },
    { id: "deadlines", label: "Due soon", hint: "Deadlines FNDR found in your work, with the memories behind them." },
    { id: "threads", label: "Pick up where you left off", hint: "Your recent work from the last 24 hours." },
    { id: "sets", label: "Your sets", hint: "Groups of places you saved to reopen together." },
];

export type HomeSectionChoice = Record<HomeSectionId, boolean>;

const CHANGE_EVENT = "fndr-home-sections-changed";
const ALL_ON: HomeSectionChoice = { routine: true, deadlines: true, threads: true, sets: true };

function storage(): Storage | null {
    try {
        return typeof window === "undefined" ? null : window.localStorage;
    } catch {
        return null;
    }
}

/** A section is on unless the person turned it off, so a new section appears for everyone. */
export function parseHomeSections(raw: string | null): HomeSectionChoice {
    if (!raw) return ALL_ON;
    try {
        const parsed = JSON.parse(raw) as Record<string, unknown>;
        return {
            routine: parsed.routine !== false,
            deadlines: parsed.deadlines !== false,
            threads: parsed.threads !== false,
            sets: parsed.sets !== false,
        };
    } catch {
        return ALL_ON;
    }
}

let cachedRaw: string | null | undefined;
let cachedChoice: HomeSectionChoice = ALL_ON;

function snapshot(): HomeSectionChoice {
    let raw: string | null = null;
    try {
        raw = storage()?.getItem(STORAGE_KEYS.homeSections) ?? null;
    } catch {
        raw = null;
    }
    if (raw !== cachedRaw) {
        cachedRaw = raw;
        cachedChoice = parseHomeSections(raw);
    }
    return cachedChoice;
}

function subscribe(listener: () => void): () => void {
    window.addEventListener(CHANGE_EVENT, listener);
    window.addEventListener("storage", listener);
    return () => {
        window.removeEventListener(CHANGE_EVENT, listener);
        window.removeEventListener("storage", listener);
    };
}

export function setHomeSection(id: HomeSectionId, shown: boolean): void {
    const next = { ...snapshot(), [id]: shown };
    try {
        storage()?.setItem(STORAGE_KEYS.homeSections, JSON.stringify(next));
    } catch {
        // Private windows can refuse storage; the choice then lasts until reload.
        cachedRaw = undefined;
        cachedChoice = next;
    }
    window.dispatchEvent(new Event(CHANGE_EVENT));
}

/** The person's choice of Home sections, live across the settings drawer and Home. */
export function useHomeSections(): HomeSectionChoice {
    return useSyncExternalStore(subscribe, snapshot, () => ALL_ON);
}

import { describe, expect, it } from "vitest";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { groupVaultMemories } from "./vaultGrouping";
import { linkSessions } from "./sessionLinks";

const NOW = new Date(2026, 9, 8, 15, 0).getTime();
const MINUTE = 60_000;

function card(id: string, app: string, title: string, minutesAgo: number, files: string[] = []): MemoryCard {
    return {
        id,
        title,
        summary: "",
        action: "",
        context: [],
        timestamp: NOW - minutesAgo * MINUTE,
        app_name: app,
        window_title: title,
        score: 0,
        source_count: 1,
        raw_snippets: [],
        files_touched: files,
    } as MemoryCard;
}

function linksFor(cards: MemoryCard[]) {
    const [day] = groupVaultMemories(cards, NOW);
    return linkSessions(day);
}

describe("linkSessions", () => {
    it("links sessions in two apps that touch the same file within minutes", () => {
        const links = linksFor([
            card("code", "Visual Studio Code", "retrieve.rs", 5, ["src-tauri/src/context_runtime/retrieve.rs"]),
            card("term", "Terminal", "cargo test", 8, ["retrieve.rs"]),
        ]);
        expect(links.get("code")).toEqual([{ leadId: "term", label: "Terminal", moments: 1 }]);
        expect(links.get("term")).toEqual([{ leadId: "code", label: "Visual Studio Code", moments: 1 }]);
    });

    it("links on two distinctive title words", () => {
        const links = linksFor([
            card("doc", "Pages", "Capstone retro notes", 4),
            card("web", "Google Chrome", "Capstone retro board - Google Chrome", 9),
        ]);
        expect(links.get("doc")?.[0].label).toBe("Google Chrome");
    });

    it("does not link apps that were only open at the same time", () => {
        const links = linksFor([
            card("doc", "Pages", "Capstone retro notes", 4),
            card("music", "Spotify", "Liked Songs", 5),
            card("web", "Google Chrome", "Capstone - Google Chrome", 6),
        ]);
        expect(links.size).toBe(0);
    });

    it("does not link the same file an hour apart", () => {
        const links = linksFor([
            card("code", "Visual Studio Code", "retrieve.rs", 5, ["retrieve.rs"]),
            card("term", "Terminal", "cargo test", 70, ["retrieve.rs"]),
        ]);
        expect(links.size).toBe(0);
    });

    it("never counts the app's own name as a shared title word", () => {
        const links = linksFor([
            card("a", "Google Chrome", "Flights to Denver - Google Chrome", 3),
            card("b", "Google Chrome Canary", "Chrome Canary release notes - Google Chrome Canary", 4),
        ]);
        expect(links.size).toBe(0);
    });
});

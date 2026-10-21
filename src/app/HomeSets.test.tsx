import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { NamedWorkSet, RoutineOffer, WorkItem } from "@/shared/ipc/tauri";
import { HomeSets } from "./HomeSets";
import { RoutineOfferCard } from "./RoutineOfferCard";
import { SaveAsSet } from "./SaveAsSet";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function item(memoryId: string): WorkItem {
    return { memoryId, label: `Item ${memoryId}`, kind: "url", reopenRank: 4, appName: "Safari", capturedAt: 1 };
}

function named(id: string, name: string, ids: string[]): NamedWorkSet {
    return { id, name, memoryIds: ids, items: ids.map(item), savedAt: 1 };
}

const offer: RoutineOffer = { id: "r1", label: "Start your usual morning set", setId: "s1", memoryIds: ["a", "b"], reason: "Opened on 4 weekdays around 9:00", dueNow: true };

let sets: NamedWorkSet[] = [];
let offers: RoutineOffer[] | Error = [];

function route() {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
        const payload = args as Record<string, unknown> | undefined;
        switch (command) {
            case "list_named_sets": return sets;
            case "delete_named_set":
                sets = sets.filter((set) => set.id !== payload?.id);
                return null;
            case "save_named_set": {
                if (payload?.name === "Taken") throw "A set is already called “Taken”.";
                const saved = named("new", String(payload?.name), payload?.memoryIds as string[]);
                sets = [saved, ...sets];
                return saved;
            }
            case "routine_offers":
                if (offers instanceof Error) throw offers;
                return offers;
            case "dismiss_routine_offer": return null;
            case "resolve_work_set":
                return { kind: "best", value: { id: "ws", title: "Parser", reason: "", score: 1, items: [item("a"), item("b")] } };
            case "open_work_set":
                return (payload?.memoryIds as string[]).map((memoryId) => ({ memoryId, label: `Item ${memoryId}`, ok: true, detail: "Opened", outcome: { kind: "opened" } }));
            default: throw new Error(`unexpected ${command}`);
        }
    });
}

beforeEach(() => {
    vi.mocked(invoke).mockReset();
    sets = [];
    offers = [];
    route();
});
afterEach(cleanup);

describe("Your sets", () => {
    it("teaches how to make one when there are none", async () => {
        render(<HomeSets refreshKey={0} />);
        expect(await screen.findByText(/No saved sets yet/)).toHaveTextContent("Save as set");
    });

    it("lists saved sets, opens one on a tap and deletes after a second tap", async () => {
        sets = [named("s1", "Thesis", ["a", "b"])];
        render(<HomeSets refreshKey={0} />);
        expect(await screen.findByRole("heading", { name: "Thesis" })).toBeInTheDocument();
        expect(screen.getByText("2 places")).toBeInTheDocument();
        expect(invoke).not.toHaveBeenCalledWith("open_work_set", expect.anything());

        fireEvent.click(screen.getByRole("button", { name: "Open for Thesis" }));
        expect(await screen.findByText("Opened 2 of 2")).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Delete Thesis" }));
        expect(invoke).not.toHaveBeenCalledWith("delete_named_set", expect.anything());
        fireEvent.click(screen.getByRole("button", { name: "Yes, delete Thesis" }));
        await waitFor(() => expect(screen.queryByRole("heading", { name: "Thesis" })).not.toBeInTheDocument());
        expect(invoke).toHaveBeenCalledWith("delete_named_set", { id: "s1" });
    });

    it("says when sets cannot be loaded", async () => {
        vi.mocked(invoke).mockRejectedValue(new Error("x"));
        render(<HomeSets refreshKey={0} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("Saved sets could not be loaded.");
    });
});

describe("Save as set", () => {
    it("names the thread's set and saves its memories", async () => {
        const onSaved = vi.fn();
        render(<SaveAsSet name="Parser" load={async () => [item("a"), item("b")]} onSaved={onSaved} />);
        fireEvent.click(screen.getByRole("button", { name: "Save Parser as a set" }));
        const field = screen.getByRole("textbox", { name: "Set name" });
        expect(field).toHaveValue("Parser");
        expect(field).toHaveFocus();
        fireEvent.change(field, { target: { value: "Parser work" } });
        fireEvent.click(screen.getByRole("button", { name: "Save" }));
        expect(await screen.findByText("Saved as “Parser work”")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("save_named_set", { name: "Parser work", memoryIds: ["a", "b"] });
        expect(onSaved).toHaveBeenCalledOnce();
    });

    it("shows the reason a name is refused", async () => {
        render(<SaveAsSet name="Parser" load={async () => [item("a")]} onSaved={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: "Save Parser as a set" }));
        fireEvent.change(screen.getByRole("textbox", { name: "Set name" }), { target: { value: "Taken" } });
        fireEvent.click(screen.getByRole("button", { name: "Save" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("A set is already called “Taken”.");
    });
});

describe("Routine offer", () => {
    it("offers the usual set as a card that opens only on a tap, side by side when asked", async () => {
        sets = [named("s1", "Morning", ["a", "b"])];
        offers = [offer];
        render(<RoutineOfferCard arrange />);
        expect(await screen.findByRole("heading", { name: "Start your usual morning set" })).toBeInTheDocument();
        expect(screen.getByText("Opened on 4 weekdays around 9:00")).toBeInTheDocument();
        expect(invoke).not.toHaveBeenCalledWith("open_work_set", expect.anything());
        fireEvent.click(screen.getByRole("button", { name: "Start for Start your usual morning set" }));
        expect(await screen.findByText(/Opened 2 of 2/)).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("open_work_set", { memoryIds: ["a", "b"], layout: "left_right_split" });
    });

    it("is dismissible", async () => {
        offers = [offer];
        render(<RoutineOfferCard />);
        fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
        expect(screen.queryByRole("heading", { name: "Start your usual morning set" })).not.toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("dismiss_routine_offer", { id: "r1" });
    });

    it("shows nothing when no offer is due or offers cannot be read", async () => {
        offers = [{ ...offer, dueNow: false }];
        const quiet = render(<RoutineOfferCard />);
        await waitFor(() => expect(invoke).toHaveBeenCalledWith("routine_offers"));
        expect(quiet.container).toBeEmptyDOMElement();
        quiet.unmount();
        offers = new Error("x");
        const failed = render(<RoutineOfferCard />);
        await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
        expect(failed.container).toBeEmptyDOMElement();
    });
});

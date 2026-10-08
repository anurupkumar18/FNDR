import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

const opened = vi.hoisted(() => ({ openExternalUrl: vi.fn() }));
vi.mock("@/shared/utils/openExternalUrl", () => opened);

import { AgentReply, replyBlocks } from "./AgentReply";

const deck = { id: "m9", title: "Quarterly review deck", appName: "Keynote", timestamp: 1 };

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("AgentReply", () => {
    it("shows lists, code and emphasis as such", () => {
        const answer = [
            "## Plan",
            "Do these **today**:",
            "- Email Sam",
            "- Run `make test`",
            "",
            "1. Draft",
            "2. Send",
            "```",
            "cargo test --lib",
            "```",
        ].join("\n");
        const { container } = render(<AgentReply content={answer} cited={[]} onOpenMemory={vi.fn()} />);

        expect(screen.getByText("Plan").tagName).toBe("STRONG");
        expect(screen.getByText("today").tagName).toBe("STRONG");
        expect(container.querySelectorAll("ul li")).toHaveLength(2);
        expect(container.querySelectorAll("ol li")).toHaveLength(2);
        expect(screen.getByText("make test").tagName).toBe("CODE");
        expect(container.querySelector("pre code")).toHaveTextContent("cargo test --lib");
    });

    it("opens web links outside the app and never renders markup from an answer", () => {
        const answer = 'See [the docs](https://example.com/a) and <img src=x onerror="alert(1)"> [bad](javascript:alert(1))';
        const { container } = render(<AgentReply content={answer} cited={[]} onOpenMemory={vi.fn()} />);

        fireEvent.click(screen.getByRole("link", { name: "the docs" }));
        expect(opened.openExternalUrl).toHaveBeenCalledWith("https://example.com/a");
        expect(container.querySelector("img")).toBeNull();
        expect(screen.getAllByRole("link")).toHaveLength(1);
        expect(container).toHaveTextContent('<img src=x onerror="alert(1)">');
    });

    it("turns a citation into a button only when a memory stands behind it", () => {
        const onOpenMemory = vi.fn();
        render(<AgentReply content="Q3 is in the deck [1]; nothing backs [4]." cited={[deck]} onOpenMemory={onOpenMemory} />);

        fireEvent.click(screen.getByRole("button", { name: "Open memory 1: Quarterly review deck" }));
        expect(onOpenMemory).toHaveBeenCalledWith(deck);
        expect(screen.queryByRole("button", { name: /Open memory 4/ })).not.toBeInTheDocument();
    });

    it("keeps an answer that was cut off inside a code block", () => {
        expect(replyBlocks("Run this:\n```\nnpm test")).toEqual([
            { kind: "paragraph", lines: ["Run this:"] },
            { kind: "code", text: "npm test" },
        ]);
    });
});

import { describe, expect, it } from "vitest";

import {
    MAX_THREAD_TURNS,
    type ConversationTurn,
    answeredQuestions,
    appendTurn,
    conversationQuery,
    isFollowUp,
    resolveTurn,
} from "./notchConversation";

function turn(id: string, question: string, status: ConversationTurn["status"]): ConversationTurn {
    return { id, question, status };
}

describe("notch conversation", () => {
    it("treats referring fragments as follow-ups", () => {
        expect(isFollowUp("what about yesterday?")).toBe(true);
        expect(isFollowUp("and the other one")).toBe(true);
        expect(isFollowUp("in Figma?")).toBe(true);
        expect(isFollowUp("why")).toBe(true);
    });

    it("leaves a self-contained question alone", () => {
        expect(isFollowUp("what was the postgres connection string I copied on Tuesday")).toBe(
            false
        );
        // Short but names its own subject and verb.
        expect(conversationQuery(["what did I read about vLLM"], "show me the SLURM script I wrote")).toBe(
            "show me the SLURM script I wrote"
        );
    });

    it("expands a follow-up with the questions before it", () => {
        const expanded = conversationQuery(
            ["what did I read about vLLM batching", "which repo was that in"],
            "what about yesterday?"
        );
        expect(expanded).toContain("vLLM batching");
        expect(expanded).toContain("which repo was that in");
        expect(expanded.endsWith("what about yesterday?")).toBe(true);
    });

    it("carries at most the two most recent questions", () => {
        const expanded = conversationQuery(["first topic", "second topic", "third topic"], "why");
        expect(expanded).not.toContain("first topic");
        expect(expanded).toContain("second topic");
        expect(expanded).toContain("third topic");
    });

    it("has nothing to expand on the opening question", () => {
        expect(conversationQuery([], "why")).toBe("why");
    });

    it("only carries questions that actually got answered", () => {
        const turns = [
            turn("1", "answered question", "answered"),
            turn("2", "failed question", "failed"),
            turn("3", "in flight", "thinking"),
        ];
        expect(answeredQuestions(turns)).toEqual(["answered question"]);
    });

    it("caps the thread and resolves turns in place", () => {
        let turns: ConversationTurn[] = [];
        for (let index = 0; index < MAX_THREAD_TURNS + 3; index += 1) {
            turns = appendTurn(turns, turn(`t${index}`, `question ${index}`, "thinking"));
        }
        expect(turns).toHaveLength(MAX_THREAD_TURNS);
        expect(turns[0].id).toBe("t3");

        turns = resolveTurn(turns, "t5", { status: "answered", answer: "because" });
        const resolved = turns.find((candidate) => candidate.id === "t5");
        expect(resolved?.status).toBe("answered");
        expect(resolved?.answer).toBe("because");
        // Other turns are untouched.
        expect(turns.find((candidate) => candidate.id === "t6")?.status).toBe("thinking");
    });
});

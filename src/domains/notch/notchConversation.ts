/**
 * Turning a notch conversation into something the retrieval pipeline can
 * answer.
 *
 * `fndr_answer` is single-shot: it plans, retrieves and composes from one
 * query string with no memory of what came before. A spoken follow-up
 * ("what about yesterday?") carries almost no retrievable terms on its own, so
 * follow-ups are expanded here with the questions that preceded them before
 * they reach the backend. The expansion is deliberately client-side and
 * keyword-only — no extra model round-trip, and the shared answer pipeline the
 * omnibar and MCP use stays untouched.
 */

import type { MemoryCard } from "@/shared/ipc/tauri";

export type TurnStatus = "thinking" | "answered" | "failed";

export interface ConversationTurn {
    id: string;
    question: string;
    status: TurnStatus;
    answer?: string;
    cards?: MemoryCard[];
    error?: string;
}

/** How many prior questions a follow-up is allowed to drag along. */
const CONTEXT_QUESTIONS = 2;
/** Past this many characters a question stands on its own. */
const STANDALONE_LENGTH = 48;
/** Longest expanded query handed to the planner. */
const MAX_QUERY_LENGTH = 320;
/** Turns kept in the thread; older ones scroll out of reach anyway. */
export const MAX_THREAD_TURNS = 12;

/**
 * Words that point at something already said. A question opening with one of
 * these is answering *about* the previous turn, not starting a new topic.
 */
const REFERRING_OPENERS = [
    "and",
    "also",
    "what about",
    "how about",
    "why",
    "who",
    "when",
    "where",
    "which",
    "that",
    "those",
    "them",
    "they",
    "it",
    "its",
    "this",
    "these",
    "then",
    "so",
    "but",
    "more",
    "any",
    "anything else",
    "same",
    "again",
    "tell me more",
];

export function isFollowUp(question: string): boolean {
    const trimmed = question.trim().toLowerCase();
    if (!trimmed) {
        return false;
    }
    if (trimmed.length <= STANDALONE_LENGTH) {
        // Short questions lean on context even when they name a topic.
        if (REFERRING_OPENERS.some((opener) => trimmed.startsWith(`${opener} `) || trimmed === opener)) {
            return true;
        }
        // A bare fragment with no verb of its own ("yesterday?", "in Figma?").
        return trimmed.split(/\s+/).length <= 4;
    }
    return false;
}

/**
 * The query to actually retrieve on: the question itself, or — for a
 * follow-up — the recent questions prepended so the planner sees the topic.
 */
export function conversationQuery(priorQuestions: string[], question: string): string {
    const asked = question.trim();
    if (!asked || priorQuestions.length === 0 || !isFollowUp(asked)) {
        return asked;
    }
    const context = priorQuestions
        .map((prior) => prior.trim())
        .filter(Boolean)
        .slice(-CONTEXT_QUESTIONS);
    if (context.length === 0) {
        return asked;
    }
    const expanded = [...context, asked].join(" ");
    return expanded.length <= MAX_QUERY_LENGTH
        ? expanded
        : expanded.slice(expanded.length - MAX_QUERY_LENGTH);
}

/** Questions from turns that actually got an answer, oldest first. */
export function answeredQuestions(turns: ConversationTurn[]): string[] {
    return turns.filter((turn) => turn.status === "answered").map((turn) => turn.question);
}

export function appendTurn(turns: ConversationTurn[], turn: ConversationTurn): ConversationTurn[] {
    return [...turns, turn].slice(-MAX_THREAD_TURNS);
}

export function resolveTurn(
    turns: ConversationTurn[],
    id: string,
    resolution: Partial<ConversationTurn>
): ConversationTurn[] {
    return turns.map((turn) => (turn.id === id ? { ...turn, ...resolution } : turn));
}

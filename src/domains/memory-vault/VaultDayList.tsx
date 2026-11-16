import { useState, type ReactNode } from "react";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { Icon, type IconName } from "@/shared/components/atoms";
import { MemoryCard as MemoryCardComponent, pickPreviewText, reopenDestinationLabel } from "./MemoryCard";
import { sessionDigest, sessionDigestFacts, sessionDigestLabel, type SessionDigest } from "./sessionDigest";
import { linkSessions, type SessionLink } from "./sessionLinks";
import { vaultSourceKind, type VaultDay, type VaultRow, type VaultSourceKind } from "./vaultGrouping";

const SOURCE_LABELS: Record<VaultSourceKind, string> = {
    page: "Web page",
    document: "Document",
    download: "Download",
    agent: "Agent note",
    screen: "Screen capture",
};

const SOURCE_ATOM_ICONS: Partial<Record<VaultSourceKind, IconName>> = {
    page: "globe",
    agent: "sparkles",
    screen: "monitor",
};

/** Shapes the shared Icon atom does not have yet (file-text, download). */
const SOURCE_PATHS: Partial<Record<VaultSourceKind, ReactNode>> = {
    document: (
        <>
            <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
            <path d="M14 2v4a2 2 0 0 0 2 2h4" />
            <path d="M16 13H8M16 17H8M10 9H8" />
        </>
    ),
    download: (
        <>
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <path d="m7 10 5 5 5-5" />
            <path d="M12 15V3" />
        </>
    ),
};

function SourceIcon({ kind }: { kind: VaultSourceKind }) {
    const atom = SOURCE_ATOM_ICONS[kind];
    return (
        <span className="vault-source-icon" role="img" aria-label={SOURCE_LABELS[kind]} title={SOURCE_LABELS[kind]}>
            {atom ? (
                <Icon name={atom} size={14} />
            ) : (
                <svg
                    viewBox="0 0 24 24"
                    width={14}
                    height={14}
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden="true"
                >
                    {SOURCE_PATHS[kind]}
                </svg>
            )}
        </span>
    );
}

/** "4:41 PM to 6:24 PM" for a session row. */
function sessionRange(row: VaultRow): string {
    const clock = (timestamp: number) =>
        new Date(timestamp).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    const oldest = Math.min(row.lead.timestamp, ...row.similar.map((card) => card.timestamp));
    return oldest === row.lead.timestamp ? clock(oldest) : `${clock(oldest)} to ${clock(row.lead.timestamp)}`;
}

function threadCountHint(card: MemoryCard): number | undefined {
    return (
        card.topic_categories?.length ||
        (card.insight_context_thread?.trim() ? 1 : 0) ||
        card.files_touched?.length ||
        undefined
    );
}

/** Where a row's source button is in its press: idle, waiting on the OS, or
 *  just landed. Anything that did not open is a note under the row instead. */
export type VaultReopenPhase = "opening" | "opened";

interface VaultDayListProps {
    days: VaultDay[];
    /** A memory to reveal even when it is folded under a near-duplicate. */
    focusMemoryId?: string | null;
    onOpen: (card: MemoryCard) => void;
    onReopen: (card: MemoryCard) => void;
    reopenPhaseById?: Record<string, VaultReopenPhase>;
    /** A one-line outcome to show under a row when its source did not open. */
    reopenNoteById?: Record<string, string>;
}

function ArrowUpRight() {
    return (
        <svg viewBox="0 0 24 24" width={11} height={11} fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M7 17 17 7M9 7h8v8" />
        </svg>
    );
}

function Check() {
    return (
        <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="m5 12 5 5 9-10" />
        </svg>
    );
}

/** The Vault list: day sections, thread headings, and one row per session. */
export function VaultDayList({
    days,
    focusMemoryId = null,
    onOpen,
    onReopen,
    reopenPhaseById = {},
    reopenNoteById = {},
}: VaultDayListProps) {
    const [openSimilarIds, setOpenSimilarIds] = useState<Set<string>>(new Set());

    const toggleSimilar = (leadId: string) => {
        setOpenSimilarIds((previous) => {
            const next = new Set(previous);
            if (next.has(leadId)) next.delete(leadId);
            else next.add(leadId);
            return next;
        });
    };

    const showRow = (leadId: string) => {
        const target = document.getElementById(`vault-row-${leadId}`);
        target?.scrollIntoView?.({ block: "center" });
        target?.querySelector<HTMLElement>('[aria-label^="Open memory"], button')?.focus();
    };

    const renderLine = (
        card: MemoryCard,
        row?: VaultRow,
        similarOpen = false,
        digest: SessionDigest | null = null,
        links: SessionLink[] = [],
    ) => {
        const phase = reopenPhaseById[card.id];
        const note = reopenNoteById[card.id];
        const hasSession = Boolean(row && row.similar.length > 0);
        return (
            <div className={`vault-row${card.reopen_target ? " vault-row--reopenable" : ""}${phase ? ` is-${phase}` : ""}`}>
                <MemoryCardComponent
                    card={card}
                    variant="compact"
                    onOpen={onOpen}
                    sourceIcon={<SourceIcon kind={vaultSourceKind(card)} />}
                    threadCountHint={threadCountHint(card)}
                />
                {card.reopen_target ? (
                    <button
                        type="button"
                        className="vault-row-open"
                        data-phase={phase ?? "idle"}
                        aria-label={`${reopenDestinationLabel(card)}: ${card.title}`}
                        aria-busy={phase === "opening" || undefined}
                        onClick={() => onReopen(card)}
                    >
                        {phase === "opening" ? (
                            <>
                                <span className="vault-row-open-spinner" aria-hidden="true" />
                                Opening…
                            </>
                        ) : phase === "opened" ? (
                            <>
                                <Check />
                                Opened
                            </>
                        ) : (
                            <>
                                {reopenDestinationLabel(card)}
                                <ArrowUpRight />
                            </>
                        )}
                    </button>
                ) : null}
                {note ? (
                    <p className="vault-row-note" role="status">
                        {note}
                    </p>
                ) : null}
                {hasSession && row && (
                    <button
                        type="button"
                        className="vault-row-similar"
                        aria-expanded={similarOpen}
                        aria-label={`${row.similar.length} more ${row.similar.length === 1 ? "moment" : "moments"} from this session: ${card.title}`}
                        onClick={() => toggleSimilar(card.id)}
                    >
                        <svg className="vault-row-similar-chevron" viewBox="0 0 24 24" width={11} height={11} fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden="true">
                            <path d="m9 6 6 6-6 6" />
                        </svg>
                        {digest ? sessionDigestLabel(digest) : `${row.similar.length + 1} moments`}
                        <span className="vault-row-session-range" aria-label="Session time range">
                            {sessionRange(row)}
                        </span>
                    </button>
                )}
                {digest && sessionDigestFacts(digest) && (
                    <p className="vault-row-earlier">
                        <span className="vault-row-earlier-label">In this session</span> {sessionDigestFacts(digest)}
                    </p>
                )}
                {digest?.earlier && (
                    <p className="vault-row-earlier">
                        <span className="vault-row-earlier-label">Earlier in this session</span> {digest.earlier}
                    </p>
                )}
                {links.length > 0 && (
                    <p className="vault-row-earlier">
                        <span className="vault-row-earlier-label">Same stretch of work</span>
                        {links.map((link) => (
                            <button
                                key={link.leadId}
                                type="button"
                                className="vault-row-link"
                                onClick={() => showRow(link.leadId)}
                            >
                                {link.label}
                                {link.moments > 1 ? ` (${link.moments} moments)` : ""}
                            </button>
                        ))}
                    </p>
                )}
            </div>
        );
    };

    const linksByDay = new Map(days.map((day) => [day.key, linkSessions(day)]));

    return (
        <>
            {days.map((day) => (
                <section key={day.key} className="vault-day" aria-label={day.label}>
                    <h3 className="vault-day-heading">
                        {day.label}
                        <span className="vault-day-count">
                            {day.count.toLocaleString()} {day.count === 1 ? "memory" : "memories"}
                        </span>
                    </h3>
                    {day.threads.map((thread) => (
                        <div key={thread.key} className="vault-thread">
                            <h4 className="vault-thread-heading">
                                {thread.label}
                                {thread.count > 1 && <span className="vault-thread-count"> · {thread.count}</span>}
                            </h4>
                            <ul className="vault-rows">
                                {thread.rows.map((row) => {
                                    // A focused memory unfolds its group; the toggle still flips it.
                                    const similarOpen =
                                        openSimilarIds.has(row.lead.id) !==
                                        row.similar.some((card) => card.id === focusMemoryId);
                                    return (
                                        <li key={row.lead.id} id={`vault-row-${row.lead.id}`}>
                                            {renderLine(
                                                row.lead,
                                                row,
                                                similarOpen,
                                                sessionDigest(row, pickPreviewText),
                                                linksByDay.get(day.key)?.get(row.lead.id),
                                            )}
                                            {similarOpen && (
                                                <ul className="vault-rows vault-rows--similar">
                                                    {row.similar.map((card) => (
                                                        <li key={card.id}>{renderLine(card)}</li>
                                                    ))}
                                                </ul>
                                            )}
                                        </li>
                                    );
                                })}
                            </ul>
                        </div>
                    ))}
                </section>
            ))}
        </>
    );
}

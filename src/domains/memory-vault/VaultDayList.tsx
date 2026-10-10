import { useState, type ReactNode } from "react";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { Icon, type IconName } from "@/shared/components/atoms";
import { MemoryCard as MemoryCardComponent, pickPreviewText, reopenButtonLabel } from "./MemoryCard";
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

interface VaultDayListProps {
    days: VaultDay[];
    /** A memory to reveal even when it is folded under a near-duplicate. */
    focusMemoryId?: string | null;
    onOpen: (card: MemoryCard) => void;
    onReopen: (card: MemoryCard) => void;
}

/** The Vault list: day sections, thread headings, and one row per session. */
export function VaultDayList({ days, focusMemoryId = null, onOpen, onReopen }: VaultDayListProps) {
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
    ) => (
        <div className="vault-row">
            <MemoryCardComponent
                card={card}
                variant="compact"
                onOpen={onOpen}
                sourceIcon={<SourceIcon kind={vaultSourceKind(card)} />}
                threadCountHint={threadCountHint(card)}
            />
            <div className="vault-row-actions">
                {row && row.similar.length > 0 && (
                    <span className="vault-row-session-range" aria-label="Session time range">
                        {sessionRange(row)}
                    </span>
                )}
                {card.reopen_target ? (
                    <button
                        type="button"
                        className="ui-action-btn vault-row-action"
                        aria-label={`${reopenButtonLabel(card)}: ${card.title}`}
                        onClick={() => onReopen(card)}
                    >
                        Open source
                    </button>
                ) : null}
                {row && row.similar.length > 0 && (
                    <button
                        type="button"
                        className="ui-action-btn vault-row-action vault-row-similar"
                        aria-expanded={similarOpen}
                        aria-label={`${row.similar.length} more ${row.similar.length === 1 ? "moment" : "moments"} from this session: ${card.title}`}
                        onClick={() => toggleSimilar(card.id)}
                    >
                        {digest ? sessionDigestLabel(digest) : `${row.similar.length + 1} moments`}
                    </button>
                )}
            </div>
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

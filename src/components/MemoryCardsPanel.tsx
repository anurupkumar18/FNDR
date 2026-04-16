import { useEffect, useMemo, useState } from "react";
import { MemoryCard, deleteMemory, listMemoryCards } from "../api/tauri";
import "./MemoryCardsPanel.css";

interface MemoryCardsPanelProps {
    isVisible: boolean;
    onClose: () => void;
    appNames: string[];
    onMemoryDeleted?: (memoryId: string) => void;
}

const APP_FILTER_ALL = "__all__";
const TIME_FILTER_ALL = "__time_all__";
const PERSPECTIVE_FILTER_ALL = "__perspective_all__";

type TimeFilter = 
    | typeof TIME_FILTER_ALL 
    | "last_hour" 
    | "today" 
    | "last_24h" 
    | "last_7d";

type PerspectiveFilter =
    | typeof PERSPECTIVE_FILTER_ALL
    | "web"
    | "coding"
    | "meetings"
    | "communication"
    | "docs";

const TIME_FILTER_OPTIONS: Array<{ value: TimeFilter; label: string }> = [
    { value: TIME_FILTER_ALL, label: "All history" },
    { value: "last_hour", label: "Last hour" },
    { value: "today", label: "Today" },
    { value: "last_24h", label: "Last 24 hours" },
    { value: "last_7d", label: "Last 7 days" },
];

const PERSPECTIVE_FILTER_OPTIONS: Array<{ value: PerspectiveFilter; label: string }> = [
    { value: PERSPECTIVE_FILTER_ALL, label: "All perspectives" },
    { value: "web", label: "Web pages" },
    { value: "coding", label: "Coding sessions" },
    { value: "meetings", label: "Meetings" },
    { value: "communication", label: "Communication" },
    { value: "docs", label: "Docs & writing" },
];

function normalizeText(value: string | undefined | null): string {
    if (!value) {
        return "";
    }
    return value
        .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
        .replace(/\s*Sources:\s*[A-Za-z0-9,\-\s]+\.?$/i, "")
        .replace(/\s+/g, " ")
        .trim();
}

function hasReadableCharacters(value: string): boolean {
    return /[\p{L}\p{N}]/u.test(value);
}

function pickReadable(...candidates: Array<string | undefined | null>): string {
    for (const candidate of candidates) {
        const cleaned = normalizeText(candidate);
        if (cleaned && hasReadableCharacters(cleaned)) {
            return cleaned;
        }
    }
    return "";
}

function fallbackTitle(card: MemoryCard): string {
    const windowTitle = normalizeText(card.window_title);
    const title = normalizeText(card.title);
    const lowerWindow = windowTitle.toLowerCase();
    const lowerApp = card.app_name.toLowerCase();
    const genericWindow = !windowTitle
        || lowerWindow === lowerApp
        || includesAny(lowerWindow, ["new tab", "dashboard", "home", "settings"]);

    if (!genericWindow && (title.endsWith("...") || !title)) {
        return windowTitle;
    }

    return pickReadable(card.title, card.window_title)
        || `Memory in ${card.app_name}`;
}

function fallbackSummary(card: MemoryCard): string {
    return pickReadable(card.summary, card.raw_snippets[0], card.window_title)
        || `Captured context in ${card.app_name}.`;
}

function parseHost(rawUrl: string): string {
    try {
        const normalized = rawUrl.startsWith("http") ? rawUrl : `https://${rawUrl}`;
        return new URL(normalized).host;
    } catch {
        return rawUrl.replace(/^https?:\/\//i, "").split("/")[0].trim();
    }
}

function extractUrlPath(rawUrl: string): string {
    try {
        const normalized = rawUrl.startsWith("http") ? rawUrl : `https://${rawUrl}`;
        const pathname = new URL(normalized).pathname;
        const segments = pathname.split("/").filter((segment) => segment.trim().length > 0);
        if (segments.length === 0) {
            return "";
        }
        return segments.slice(0, 3).join("/");
    } catch {
        return "";
    }
}

function extractPathHint(...candidates: Array<string | undefined | null>): string {
    const pathRegex = /([A-Za-z0-9._-]+\/[A-Za-z0-9._/\-]+)/;
    for (const candidate of candidates) {
        const cleaned = normalizeText(candidate);
        if (!cleaned) {
            continue;
        }
        const match = cleaned.match(pathRegex);
        if (match && match[1]) {
            return match[1].slice(0, 56);
        }
    }
    return "";
}

function extractSite(card: MemoryCard, titleHint?: string): string {
    if (!card.url) {
        return "";
    }
    const host = parseHost(card.url);
    if (!host) {
        return "";
    }

    const urlPath = extractUrlPath(card.url);
    const titlePath = extractPathHint(titleHint, card.window_title);
    const details = [urlPath, titlePath]
        .map((value) => normalizeText(value))
        .filter((value, index, arr) => value.length > 0 && arr.indexOf(value) === index)
        .slice(0, 2);

    if (details.length === 0) {
        return host;
    }

    return `${host} · ${details.join(" · ")}`;
}

function includesAny(haystack: string, needles: string[]): boolean {
    return needles.some((needle) => haystack.includes(needle));
}

function matchesFilters(
    card: MemoryCard, 
    timeFilter: TimeFilter, 
    perspectiveFilter: PerspectiveFilter
): boolean {
    const now = Date.now();
    const timestamp = Number(card.timestamp) || 0;

    // 1. Time Filtering
    if (timeFilter !== TIME_FILTER_ALL && timestamp > 0) {
        if (timeFilter === "last_hour" && timestamp < now - 60 * 60 * 1000) return false;
        if (timeFilter === "today" && new Date(timestamp).toDateString() !== new Date(now).toDateString()) return false;
        if (timeFilter === "last_24h" && timestamp < now - 24 * 60 * 60 * 1000) return false;
        if (timeFilter === "last_7d" && timestamp < now - 7 * 24 * 60 * 60 * 1000) return false;
    }

    // 2. Perspective Filtering
    if (perspectiveFilter === PERSPECTIVE_FILTER_ALL) {
        return true;
    }

    const app = card.app_name.toLowerCase();
    const windowTitle = (card.window_title || "").toLowerCase();
    const context = card.context.join(" ").toLowerCase();
    const summary = (card.summary || "").toLowerCase();
    const haystack = `${app} ${windowTitle} ${context} ${summary}`;

    if (perspectiveFilter === "web") {
        return Boolean(card.url) || includesAny(haystack, [
            "chrome", "safari", "firefox", "brave", "arc browser", "edge", "url:", "site:",
        ]);
    }

    if (perspectiveFilter === "coding") {
        return includesAny(haystack, [
            "code", "codex", "cursor", "terminal", "iterm", "xcode", "intellij", "pycharm", "webstorm", "android studio", "git",
        ]);
    }

    if (perspectiveFilter === "meetings") {
        return includesAny(haystack, [
            "meeting", "zoom", "teams", "meet.google", "call", "transcript", "fndr meetings",
        ]);
    }

    if (perspectiveFilter === "communication") {
        return includesAny(haystack, [
            "slack", "mail", "gmail", "messages", "discord", "inbox", "outlook", "chat",
        ]);
    }

    if (perspectiveFilter === "docs") {
        return includesAny(haystack, [
            "notion", "docs", "word", "pages", "pdf", "preview", "obsidian", "confluence", "readme", "document",
        ]);
    }

    return true;
}

function isStoryStyleApp(card: MemoryCard): boolean {
    const app = card.app_name.toLowerCase();
    const title = (card.window_title || "").toLowerCase();
    const haystack = `${app} ${title}`;
    return includesAny(haystack, [
        "codex",
        "antigravity",
        "chatgpt",
        "gemini",
        "claude",
        "cursor",
        "visual studio code",
        "vscode",
        "vs code",
        "terminal",
        "iterm",
        "zed",
        "xcode",
        "intellij",
    ]);
}

function isContinuityCard(card: MemoryCard): boolean {
    return Boolean(card.continuity) || card.source_count > 1;
}

function buildStoryText(
    title: string,
    summary: string,
    windowTitle: string
): string {
    return (
        normalizeText(summary)
        || normalizeText(title)
        || normalizeText(windowTitle)
    );
}

function cardCopy(
    card: MemoryCard
): {
    title: string;
    summary: string;
    site: string;
    storyMode: boolean;
    story: string;
    continuity: boolean;
} {
    const title = fallbackTitle(card);
    const site = extractSite(card, title);
    const storyMode = isStoryStyleApp(card);
    const continuity = isContinuityCard(card);
    const summary = fallbackSummary(card);
    const story = buildStoryText(title, summary, card.window_title);

    return { title, summary, site, storyMode, story, continuity };
}

function formatDay(timestamp: number): string {
    const date = new Date(timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date.toDateString() === today.toDateString()) {
        return "Today";
    }
    if (date.toDateString() === yesterday.toDateString()) {
        return "Yesterday";
    }
    return date.toLocaleDateString(undefined, {
        weekday: "short",
        month: "short",
        day: "numeric",
    });
}

export function MemoryCardsPanel({ isVisible, onClose, appNames, onMemoryDeleted }: MemoryCardsPanelProps) {
    const [cards, setCards] = useState<MemoryCard[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string>(APP_FILTER_ALL);
    const [timeFilter, setTimeFilter] = useState<TimeFilter>(TIME_FILTER_ALL);
    const [perspectiveFilter, setPerspectiveFilter] = useState<PerspectiveFilter>(PERSPECTIVE_FILTER_ALL);
    const [deletingId, setDeletingId] = useState<string | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);

    const selectableApps = useMemo(() => {
        return appNames
            .map((name) => name.trim())
            .filter((name) => name.length > 0)
            .sort((a, b) => a.localeCompare(b));
    }, [appNames]);

    const filteredCards = useMemo(
        () => cards.filter((card) => matchesFilters(card, timeFilter, perspectiveFilter)),
        [cards, timeFilter, perspectiveFilter]
    );

    useEffect(() => {
        if (!isVisible) {
            return;
        }

        let cancelled = false;
        const selectedApp = appFilter === APP_FILTER_ALL ? undefined : appFilter;

        setLoading(true);
        setError(null);

        void listMemoryCards(1500, selectedApp)
            .then((items) => {
                if (cancelled) {
                    return;
                }
                setCards(items);
            })
            .catch((err) => {
                if (cancelled) {
                    return;
                }
                setCards([]);
                setError(err instanceof Error ? err.message : "Unable to load memory cards.");
            })
            .finally(() => {
                if (!cancelled) {
                    setLoading(false);
                }
            });

        return () => {
            cancelled = true;
        };
    }, [isVisible, appFilter]);

    if (!isVisible) {
        return null;
    }

    const handleDeleteCard = async (memoryId: string) => {
        if (deletingId) {
            return;
        }

        setDeletingId(memoryId);
        try {
            const deleted = await deleteMemory(memoryId);
            if (deleted) {
                setCards((previous) => previous.filter((card) => card.id !== memoryId));
                onMemoryDeleted?.(memoryId);
            }
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to delete memory.");
        } finally {
            setDeletingId(null);
        }
    };

    const toggleExpanded = (memoryId: string) => {
        setExpandedCardIds((previous) => {
            const next = new Set(previous);
            if (next.has(memoryId)) {
                next.delete(memoryId);
            } else {
                next.add(memoryId);
            }
            return next;
        });
    };

    return (
        <div className="memory-cards-panel">
            <div className="memory-cards-header">
                <div className="memory-cards-heading">
                    <h2>All Memory Cards</h2>
                    <p>Newest to oldest</p>
                </div>
                <button className="ui-action-btn memory-cards-close-btn" onClick={onClose}>
                    ✕ Close
                </button>
            </div>

            <div className="memory-cards-toolbar">
                <div className="memory-cards-filters">
                    <label className="memory-cards-filter">
                        Universe
                        <div className="memory-cards-filter-control">
                            <select
                                value={appFilter}
                                onChange={(event) => setAppFilter(event.target.value)}
                            >
                                <option value={APP_FILTER_ALL}>All Apps</option>
                                {selectableApps.map((name) => (
                                    <option key={name} value={name}>
                                        {name}
                                    </option>
                                ))}
                            </select>
                            <svg className="memory-cards-filter-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>
                    </label>

                    <label className="memory-cards-filter">
                        History
                        <div className="memory-cards-filter-control">
                            <select
                                value={timeFilter}
                                onChange={(event) => setTimeFilter(event.target.value as TimeFilter)}
                            >
                                {TIME_FILTER_OPTIONS.map((option) => (
                                    <option key={option.value} value={option.value}>
                                        {option.label}
                                    </option>
                                ))}
                            </select>
                            <svg className="memory-cards-filter-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>
                    </label>

                    <label className="memory-cards-filter">
                        Perspective
                        <div className="memory-cards-filter-control">
                            <select
                                value={perspectiveFilter}
                                onChange={(event) => setPerspectiveFilter(event.target.value as PerspectiveFilter)}
                            >
                                {PERSPECTIVE_FILTER_OPTIONS.map((option) => (
                                    <option key={option.value} value={option.value}>
                                        {option.label}
                                    </option>
                                ))}
                            </select>
                            <svg className="memory-cards-filter-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>
                    </label>
                </div>
                <div className="memory-cards-count">{filteredCards.length} cards</div>
            </div>

            <div className="memory-cards-body">
                {loading && (
                    <div className="memory-cards-state">
                        <div className="thinking-loader thinking-loader-lg" aria-hidden="true" />
                        <p>Loading memory cards...</p>
                    </div>
                )}

                {!loading && error && (
                    <div className="memory-cards-state">
                        <p>{error}</p>
                    </div>
                )}

                {!loading && !error && filteredCards.length === 0 && (
                    <div className="memory-cards-state">
                        <p>No memory cards yet for this filter.</p>
                    </div>
                )}

                {!loading && !error && filteredCards.length > 0 && (
                    <div className="memory-cards-stream">
                        {filteredCards.map((card) => {
                            const { title, summary, site } = cardCopy(card);
                            const allChips = card.context
                                .map((item) => normalizeText(item))
                                .filter((item) => {
                                    const lower = item.toLowerCase();
                                    return (
                                        item.length > 0
                                        && !lower.startsWith("app:")
                                        && !lower.startsWith("type:")
                                        && !lower.startsWith("site:")
                                        && !lower.startsWith("sources:")
                                    );
                                });
                            const chips = allChips.slice(0, 4);
                            const isExpanded = expandedId === card.id;

                            const validSnippets = card.raw_snippets
                                .map((s) => normalizeText(s))
                                .filter((s) => s.length > 20 && hasReadableCharacters(s))
                                .filter((s) => !looksTooSimilar(s, summary) && !looksTooSimilar(s, title));

                            return (
                                <article
                                    key={card.id}
                                    className={`result-card memory-browse-card${isExpanded ? " expanded" : ""}`}
                                    onClick={() => setExpandedId(isExpanded ? null : card.id)}
                                    tabIndex={0}
                                    onKeyDown={(e) => e.key === "Enter" && setExpandedId(isExpanded ? null : card.id)}
                                >
                                    <div className="result-meta memory-browse-meta">
                                        <div className="memory-browse-meta-main">
                                            <span className="result-app">{card.app_name}</span>
                                            <span className="result-time">
                                                {formatDay(card.timestamp)} ·{" "}
                                                {new Date(card.timestamp).toLocaleTimeString(undefined, {
                                                    hour: "2-digit",
                                                    minute: "2-digit",
                                                })}
                                            </span>
                                            {card.source_count > 1 && (
                                                <span className="memory-source-count" title={`Composed from ${card.source_count} captures`}>
                                                    {card.source_count} captures
                                                </span>
                                            )}
                                        </div>
                                        <button
                                            className="ui-action-btn memory-delete-btn"
                                            onClick={(e) => { e.stopPropagation(); void handleDeleteCard(card.id); }}
                                            disabled={deletingId === card.id}
                                            aria-label="Delete memory card"
                                            title="Delete this memory"
                                        >
                                            {deletingId === card.id ? "Deleting..." : "Delete"}
                                        </button>
                                    </div>
                                    <div className="memory-browse-content">
                                        {storyMode ? (
                                            <div className={`memory-browse-summary memory-browse-summary-primary ${collapseState}`}>
                                                {story}
                                            </div>
                                        ) : (
                                            <div className={`memory-browse-summary memory-browse-summary-primary ${collapseState}`}>
                                                {summary}
                                            </div>
                                        )}
                                        {(site || canExpand) && (
                                            <div className="memory-browse-footer-row">
                                                {site ? (
                                                    <div className="memory-browse-site">
                                                        {`Site: ${site}`}
                                                    </div>
                                                ) : (
                                                    <span className="memory-browse-footer-spacer" aria-hidden="true" />
                                                )}
                                                {canExpand && (
                                                    <button
                                                        type="button"
                                                        className="memory-browse-expand"
                                                        onClick={() => toggleExpanded(card.id)}
                                                    >
                                                        {isExpanded ? "Show less" : "See more"}
                                                    </button>
                                                )}
                                            </div>
                                        )}
                                    </div>
                                    {chips.length > 0 && (
                                        <div className="result-context-chips">
                                            {chips.map((item, index) => (
                                                <span key={`${card.id}-ctx-${index}`} className="result-chip">
                                                    {item}
                                                </span>
                                            ))}
                                        </div>
                                    )}
                                    {isExpanded && (
                                        <div className="memory-expand-details" onClick={(e) => e.stopPropagation()}>
                                            {card.url && (
                                                <a
                                                    className="memory-browse-url"
                                                    href={card.url}
                                                    target="_blank"
                                                    rel="noopener noreferrer"
                                                    title={card.url}
                                                >
                                                    🔗 {card.url}
                                                </a>
                                            )}
                                            {allChips.length > 4 && (
                                                <div className="result-context-chips memory-expand-extra-chips">
                                                    {allChips.slice(4).map((item, index) => (
                                                        <span key={`${card.id}-extra-${index}`} className="result-chip">
                                                            {item}
                                                        </span>
                                                    ))}
                                                </div>
                                            )}
                                            {validSnippets.length > 0 && (
                                                <div className="memory-snippets">
                                                    <div className="memory-snippets-label">
                                                        Captured text · {validSnippets.length} segment{validSnippets.length !== 1 ? "s" : ""}
                                                    </div>
                                                    {validSnippets.slice(0, 6).map((snippet, index) => (
                                                        <div key={`${card.id}-snip-${index}`} className="memory-snippet-item">
                                                            {snippet}
                                                        </div>
                                                    ))}
                                                </div>
                                            )}
                                        </div>
                                    )}
                                </article>
                            );
                        })}
                    </div>
                )}
            </div>
        </div>
    );
}

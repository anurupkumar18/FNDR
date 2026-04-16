import { CSSProperties, useEffect, useRef, useState } from "react";
import { MemoryCard, generateDailyBriefing, listMemoryCards } from "../api/tauri";
import "./DailyBriefing.css";

interface DailyBriefingProps {
    onCardClick: (query: string) => void;
}

function timeAgo(ts: number): string {
    const diff = Date.now() - ts;
    const m = Math.floor(diff / 60_000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    return `${Math.floor(h / 24)}d ago`;
}

function getCategory(appName: string): string {
    const n = appName.toLowerCase();
    if (n.includes("code") || n.includes("cursor") || n.includes("xcode") || n.includes("terminal") || n.includes("iterm")) return "CODE";
    if (n.includes("chrome") || n.includes("safari") || n.includes("firefox") || n.includes("arc")) return "WEB";
    if (n.includes("slack") || n.includes("discord") || n.includes("teams") || n.includes("mail") || n.includes("messages")) return "COMMS";
    if (n.includes("figma") || n.includes("sketch") || n.includes("framer") || n.includes("photoshop")) return "DESIGN";
    if (n.includes("notion") || n.includes("notes") || n.includes("bear") || n.includes("obsidian") || n.includes("docs")) return "NOTES";
    if (n.includes("spotify") || n.includes("music") || n.includes("youtube")) return "MEDIA";
    if (n.includes("linear") || n.includes("jira") || n.includes("asana")) return "TASKS";
    return "APP";
}

function getAppIcon(appName: string): string {
    const n = appName.toLowerCase();
    if (n.includes("chrome") || n.includes("safari") || n.includes("firefox") || n.includes("arc")) return "◉";
    if (n.includes("code") || n.includes("cursor") || n.includes("xcode")) return "⌥";
    if (n.includes("slack") || n.includes("discord")) return "◈";
    if (n.includes("mail") || n.includes("outlook")) return "✉";
    if (n.includes("notes") || n.includes("notion") || n.includes("obsidian")) return "◻";
    if (n.includes("terminal") || n.includes("iterm")) return "▸";
    if (n.includes("figma") || n.includes("sketch")) return "◇";
    if (n.includes("linear") || n.includes("jira")) return "◆";
    return "○";
}

function todayStart(): number {
    const d = new Date();
    d.setHours(0, 0, 0, 0);
    return d.getTime();
}

function briefingMode(): "morning" | "evening" {
    return new Date().getHours() >= 17 ? "evening" : "morning";
}

export function DailyBriefing({ onCardClick }: DailyBriefingProps) {
    const [cards, setCards] = useState<MemoryCard[]>([]);
    const [loading, setLoading] = useState(true);
    const [briefing, setBriefing] = useState<string | null>(null);
    const [briefingLoading, setBriefingLoading] = useState(false);
    const [open, setOpen] = useState(false);
    const scrollRef = useRef<HTMLDivElement>(null);
    const [canScrollLeft, setCanScrollLeft] = useState(false);
    const [canScrollRight, setCanScrollRight] = useState(false);

    useEffect(() => {
        let mounted = true;
        listMemoryCards(12)
            .then((all) => {
                if (!mounted) return;
                const todayTs = todayStart();
                const todays = all.filter((c) => c.timestamp >= todayTs);
                setCards(todays.length >= 2 ? todays : all.slice(0, 6));
                setLoading(false);
            })
            .catch(() => {
                if (!mounted) return;
                setLoading(false);
            });
        return () => { mounted = false; };
    }, []);

    // Fetch LLM briefing once when popup first opens
    useEffect(() => {
        if (!open || briefing !== null || briefingLoading) return;
        let mounted = true;
        setBriefingLoading(true);
        generateDailyBriefing(briefingMode())
            .then((text) => {
                if (!mounted) return;
                setBriefing(text || null);
                setBriefingLoading(false);
            })
            .catch(() => {
                if (!mounted) return;
                setBriefingLoading(false);
            });
        return () => { mounted = false; };
    }, [open]);

    // Escape to close
    useEffect(() => {
        if (!open) return;
        const handle = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
        window.addEventListener("keydown", handle);
        return () => window.removeEventListener("keydown", handle);
    }, [open]);

    // Scroll arrows state
    useEffect(() => {
        if (!open) return;
        const el = scrollRef.current;
        if (!el) return;
        const update = () => {
            setCanScrollLeft(el.scrollLeft > 8);
            setCanScrollRight(el.scrollLeft < el.scrollWidth - el.clientWidth - 8);
        };
        // Small delay so DOM has settled after popup mounts
        const t = setTimeout(update, 50);
        el.addEventListener("scroll", update, { passive: true });
        return () => {
            clearTimeout(t);
            el.removeEventListener("scroll", update);
        };
    }, [open, cards]);

    if (loading || cards.length === 0) return null;

    const scroll = (dir: "left" | "right") => {
        scrollRef.current?.scrollBy({ left: dir === "left" ? -248 : 248, behavior: "smooth" });
    };

    const handleCardClick = (title: string) => {
        setOpen(false);
        onCardClick(title);
    };

    return (
        <>
            {/* ── Pill trigger button ── */}
            <button
                className="briefing-pill"
                onClick={() => setOpen(true)}
                aria-label="Open today's briefing"
            >
                <span className="briefing-pill-dot" />
                <span className="briefing-pill-label">Today</span>
                <span className="briefing-pill-count">{cards.length}</span>
            </button>

            {/* ── Overlay ── */}
            {open && (
                <div
                    className="briefing-overlay"
                    onClick={(e) => { if (e.target === e.currentTarget) setOpen(false); }}
                    aria-modal="true"
                    role="dialog"
                    aria-label="Daily briefing"
                >
                    <div className="briefing-popup">
                        <div className="briefing-popup-header">
                            <div className="briefing-popup-meta">
                                <span className="briefing-popup-label">TODAY'S BRIEFING</span>
                                <span className="briefing-popup-sub">{cards.length} highlight{cards.length !== 1 ? "s" : ""}</span>
                            </div>
                            <div className="briefing-popup-nav">
                                <button
                                    className={`briefing-nav-btn ${canScrollLeft ? "visible" : ""}`}
                                    onClick={() => scroll("left")}
                                    aria-label="Scroll left"
                                >←</button>
                                <button
                                    className={`briefing-nav-btn ${canScrollRight ? "visible" : ""}`}
                                    onClick={() => scroll("right")}
                                    aria-label="Scroll right"
                                >→</button>
                                <button className="briefing-close-btn" onClick={() => setOpen(false)} aria-label="Close">×</button>
                            </div>
                        </div>

                        {(briefingLoading || briefing) && (
                            <p className={`briefing-paragraph${briefingLoading ? " briefing-paragraph--loading" : ""}`}>
                                {briefingLoading ? "Thinking…" : briefing}
                            </p>
                        )}

                        <div className="briefing-scroll-container">
                            {canScrollLeft && <div className="briefing-fade briefing-fade--left" />}
                            {canScrollRight && <div className="briefing-fade briefing-fade--right" />}

                            <div className="briefing-scroll" ref={scrollRef}>
                                {cards.map((card, i) => (
                                    <button
                                        key={card.id}
                                        className="briefing-card"
                                        style={{ "--i": i } as CSSProperties}
                                        onClick={() => handleCardClick(card.title)}
                                        aria-label={`Open: ${card.title}`}
                                    >
                                        <div className="briefing-card-top">
                                            <span className="briefing-icon">{getAppIcon(card.app_name)}</span>
                                            <span className="briefing-category">{getCategory(card.app_name)}</span>
                                            <span className="briefing-time">{timeAgo(card.timestamp)}</span>
                                        </div>
                                        <p className="briefing-title">{card.title}</p>
                                        <p className="briefing-summary">{card.summary}</p>
                                        {card.source_count > 1 && (
                                            <div className="briefing-card-footer">
                                                <span className="briefing-source-count">{card.source_count} memories</span>
                                            </div>
                                        )}
                                    </button>
                                ))}
                            </div>
                        </div>
                    </div>
                </div>
            )}
        </>
    );
}

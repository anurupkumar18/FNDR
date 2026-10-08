/**
 * HomeHero — cinematic home screen hero with mouse parallax, hero search pill,
 * and voice input.
 *
 * Design spec: docs/superpowers/specs/2026-05-18-hero-parallax-design.md
 *
 * Wires into the existing search flow via `onHeroSearch(query)` callback —
 * no duplicate state. Voice input uses the shared native voice session.
 */

import { useEffect, useRef, useState } from "react";
import {
    motion,
    useMotionValue,
    useSpring,
    useTransform,
} from "framer-motion";
import { useReducedMotionSafe } from "@/shared/motion/useReducedMotionSafe";
import { VoiceButton } from "@/shared/voice/VoiceButton";
import { VoiceStatus } from "@/shared/voice/VoiceStatus";
import { useVoice } from "@/shared/voice/useVoice";
import { Liquid } from "liquid-gooey";
import "./HomeHero.css";

/** Submit button width (44) plus its gap (14) — how far Speak moves to make room. */
const ACTION_SLOT_PX = 58;

// ─── Greeting helpers ─────────────────────────────────────────────────────────

function getGreeting(name: string, now: Date): { salutation: string; subtitle: string } {
    const h = now.getHours();
    const salutation =
        h < 12
            ? `Good Morning, ${name}!`
            : h < 17
              ? `Good Afternoon, ${name}!`
              : h < 21
                ? `Good Evening, ${name}!`
                : `Good Night, ${name}!`;
    const subtitle =
        h < 12
            ? "Let's see what the morning holds."
            : h < 17
              ? "Let's pick up where you left off."
              : h < 21
                ? "Let's revisit your day."
                : "Let's dive into your memories.";
    return { salutation, subtitle };
}

function formatHeroDate(now: Date): string {
    const weekday = now
        .toLocaleDateString("en-US", { weekday: "long" })
        .toUpperCase();
    const month = now
        .toLocaleDateString("en-US", { month: "long" })
        .toUpperCase();
    const day = now.toLocaleDateString("en-US", { day: "numeric" });
    return `${weekday} • ${month} ${day}`;
}

function getTimePlaceholder(now: Date): string {
    const h = now.getHours();
    if (h < 12) return "What did you work on this morning?";
    if (h < 17) return "What shall we uncover this afternoon?";
    if (h < 21) return "What happened today?";
    return "What shall we uncover tonight?";
}

// ─── Component ────────────────────────────────────────────────────────────────

interface HomeHeroProps {
    /** Display name for greeting. Fallback: "there". */
    userName?: string | null;
    /** Current timestamp for date chip (refresh externally to update). */
    now: Date;
    /** Greeting string from the IPC layer (getFunGreeting). */
    greeting?: string;
    /** Called when the user submits a query from the hero search pill. */
    onHeroSearch: (query: string) => void;
}

export function HomeHero({
    userName,
    now,
    greeting,
    onHeroSearch,
}: HomeHeroProps) {
    const { reduced } = useReducedMotionSafe();

    // Greeting logic — prefer the IPC greeting if available.
    const name = userName?.trim() || "there";
    const localGreeting = getGreeting(name, now);
    const salutation = greeting
        ? (() => {
              // Extract just the "Good *, Name!" part from the IPC greeting if present.
              const excl = greeting.indexOf("!");
              return excl >= 0 ? greeting.slice(0, excl + 1).trim() : greeting.trim();
          })()
        : localGreeting.salutation;
    const subtitle = localGreeting.subtitle;
    const dateLabel = formatHeroDate(now);

    // Search state (local, hands off via onHeroSearch).
    const [draft, setDraft] = useState("");
    const hasDraft = draft.trim().length > 0;
    const actionTransition = reduced ? { duration: 0 } : "smooth";
    const inputRef = useRef<HTMLInputElement>(null);

    // Hero visibility — pause parallax when offscreen.
    const heroRef = useRef<HTMLDivElement>(null);
    const [visible, setVisible] = useState(true);
    useEffect(() => {
        if (!heroRef.current) return;
        const io = new IntersectionObserver(
            ([entry]) => setVisible(entry.isIntersecting),
            { threshold: 0.1 }
        );
        io.observe(heroRef.current);
        return () => io.disconnect();
    }, []);

    // Mouse parallax — normalized -1..1.
    const mx = useMotionValue(0);
    const my = useMotionValue(0);
    const sx = useSpring(mx, { stiffness: 80, damping: 22, mass: 1 });
    const sy = useSpring(my, { stiffness: 80, damping: 22, mass: 1 });

    function onMouseMove(e: React.MouseEvent<HTMLDivElement>) {
        if (reduced || !visible) return;
        const rect = e.currentTarget.getBoundingClientRect();
        mx.set(((e.clientX - rect.left) / rect.width) * 2 - 1);
        my.set(((e.clientY - rect.top) / rect.height) * 2 - 1);
    }

    // Per-layer parallax transforms — each pair is a separate hook call
    // (Rules of Hooks: no hooks inside helper functions).
    const dateX = useTransform(sx, [-1, 1], reduced ? [0, 0] : [-6, 6]);
    const dateY = useTransform(sy, [-1, 1], reduced ? [0, 0] : [-6, 6]);
    const titleX = useTransform(sx, [-1, 1], reduced ? [0, 0] : [-30, 30]);
    const titleY = useTransform(sy, [-1, 1], reduced ? [0, 0] : [-30, 30]);
    const subtitleX = useTransform(sx, [-1, 1], reduced ? [0, 0] : [-16.5, 16.5]);
    const subtitleY = useTransform(sy, [-1, 1], reduced ? [0, 0] : [-16.5, 16.5]);
    const searchX = useTransform(sx, [-1, 1], reduced ? [0, 0] : [-10.5, 10.5]);
    const searchY = useTransform(sy, [-1, 1], reduced ? [0, 0] : [-10.5, 10.5]);

    // Voice.
    const voice = useVoice({
        surface: "home_search",
        mode: "toggle",
        onPartial: updateVoiceDraft,
        onFinal: updateVoiceDraft,
    });

    function updateVoiceDraft(text: string) {
        setDraft(text);
        window.requestAnimationFrame(() => inputRef.current?.focus());
    }

    function handleSubmit(value?: string) {
        const q = (value ?? draft).replace(/\r?\n/g, " ").replace(/\s+/g, " ").trim();
        if (q) {
            onHeroSearch(q);
            setDraft("");
        }
    }

    return (
        <div
            ref={heroRef}
            className="home-hero"
            onMouseMove={onMouseMove}
            data-wallpaper-ignore
        >
            {/* Date chip */}
            <motion.p
                className="home-hero__date"
                style={{ x: dateX, y: dateY }}
            >
                {dateLabel}
            </motion.p>

            {/* Greeting */}
            <motion.h1
                className="home-hero__title"
                style={{ x: titleX, y: titleY }}
            >
                {salutation}
            </motion.h1>

            {/* Subtitle */}
            <motion.p
                className="home-hero__subtitle"
                style={{ x: subtitleX, y: subtitleY }}
            >
                {subtitle}
            </motion.p>

            {/* Hero search pill */}
            <motion.div
                className="home-hero__search-wrap"
                style={{ x: searchX, y: searchY }}
            >
                <div
                    className={`home-hero__search-pill${voice.isActive ? " is-recording" : ""}`}
                    role="search"
                >
                    {/* Search icon */}
                    <svg
                        className="home-hero__search-icon"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2"
                        aria-hidden="true"
                    >
                        <circle cx="11" cy="11" r="8" />
                        <path d="M21 21l-4.35-4.35" />
                    </svg>

                    {/* Input */}
                    <input
                        ref={inputRef}
                        type="text"
                        className="home-hero__search-input"
                        placeholder={getTimePlaceholder(now)}
                        value={draft}
                        onChange={(e) => setDraft(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault();
                                handleSubmit();
                            }
                        }}
                        aria-label="Search your memories"
                        aria-describedby={voice.state.kind !== "idle"
                            ? "home-search-help home-voice-status"
                            : "home-search-help"}
                        autoComplete="off"
                        enterKeyHint="search"
                        spellCheck={false}
                    />

                    {/* Speak and Submit share one liquid surface: with an empty field
                        Speak holds the trailing slot and Submit waits inside it; typing
                        slides Speak over and Submit swells out of it. */}
                    <Liquid
                        className="home-hero__actions"
                        blur={4}
                        fill="var(--surface)"
                        shadow="inset 0 0 0 1px var(--border-strong)"
                    >
                    <Liquid.Item
                        className="home-hero__action home-hero__action--voice"
                        x={hasDraft ? 0 : ACTION_SLOT_PX}
                        transition={actionTransition}
                    >
                    <VoiceButton
                        mode={voice.mode}
                        state={voice.state}
                        isActive={voice.isActive}
                        onStart={voice.start}
                        onStop={voice.stop}
                        className={`home-hero__voice-btn${voice.isActive ? " is-recording" : ""}`}
                    />
                    </Liquid.Item>

                    <Liquid.Item
                        className="home-hero__action"
                        x={hasDraft ? 0 : -ACTION_SLOT_PX / 2}
                        scale={hasDraft ? 1 : 0.4}
                        transition={actionTransition}
                    >
                    <button
                        type="button"
                        className="home-hero__search-submit"
                        onClick={() => handleSubmit()}
                        aria-label="Submit search"
                        disabled={!draft.trim()}
                    >
                        <svg
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            strokeWidth="2"
                            strokeLinecap="round"
                            aria-hidden="true"
                        >
                            <path d="M5 12h14M13 6l6 6-6 6" />
                        </svg>
                    </button>
                    </Liquid.Item>
                    </Liquid>
                </div>

                {/* Voice status */}
                {voice.state.kind !== "idle" && (
                    <div
                        id="home-voice-status"
                        className="home-hero__voice-status"
                    >
                        <VoiceStatus
                            state={voice.state}
                            level={voice.level}
                            onRetry={voice.retry}
                            onCancel={voice.cancel}
                        />
                    </div>
                )}
                <p id="home-search-help" className="home-hero__search-help">
                    Search saved memories by topic, app, person, or time.
                </p>
            </motion.div>

        </div>
    );
}

export default HomeHero;

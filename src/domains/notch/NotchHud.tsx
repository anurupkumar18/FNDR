import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { animate, motion, useMotionValue, useTransform } from "framer-motion";
import { ThinkingOrb } from "thinking-orbs";
import { VoiceBeam } from "voice-glow";

import {
    type MemoryCard,
    NOTCH_HUD_GEOMETRY_EVENT,
    NOTCH_HUD_HOVER_EVENT,
    type NotchHudGeometry,
    fndrAnswer,
    getNotchHudGeometry,
    listMemoryCards,
    notchHudOpenMemory,
    searchMemoryCards,
    setNotchHudHitRect,
    setNotchHudKeyboard,
    transcribeVoiceInput,
} from "@/shared/ipc/tauri";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";

import {
    type NotchStage,
    bodySizeFor,
    bottomRadiusFor,
    fallbackGeometry,
    flareAllowance,
    notchMetrics,
    stageSpring,
    styleFor,
    topRadiusFor,
} from "./notchMetrics";
import { notchClipPath } from "./notchShape";
import {
    type ConversationTurn,
    answeredQuestions,
    appendTurn,
    conversationQuery,
    resolveTurn,
} from "./notchConversation";
import { VoiceCapture, isVoiceCaptureAvailable } from "./notchVoice";

const SEARCH_DEBOUNCE_MS = 200;
/** No row highlighted — Enter asks FNDR instead of opening a memory. */
const NO_SELECTION = -1;

/**
 * FNDR in the notch: a panel that lives on the display's camera housing,
 * widens under the pointer, and opens into a place to ask FNDR things in plain
 * language — typed or spoken — without a window ever appearing.
 *
 * The window it lives in is fixed at its largest footprint and never resized —
 * everything here animates the *content* inside it, exactly as the macOS
 * original does, because resizing a window per frame jitters against the
 * compositor. The silhouette is a clip path regenerated from the animating
 * width/height (see `notchShape.ts`), and the drawn frame is reported back to
 * Rust so the transparent margin stays click-through.
 */
export function NotchHud() {
    const [geometry, setGeometry] = useState<NotchHudGeometry>(fallbackGeometry);
    const [stage, setStage] = useState<NotchStage>("closed");
    const [hovering, setHovering] = useState(false);
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<MemoryCard[]>([]);
    const [selectedIndex, setSelectedIndex] = useState(NO_SELECTION);
    const [searching, setSearching] = useState(false);
    const [turns, setTurns] = useState<ConversationTurn[]>([]);
    const [recording, setRecording] = useState(false);
    const [voiceStream, setVoiceStream] = useState<MediaStream | null>(null);
    const [transcribing, setTranscribing] = useState(false);
    const [voiceError, setVoiceError] = useState<string | null>(null);
    const [contentHeight, setContentHeight] = useState<number>(notchMetrics.openHeaderHeight);

    const inputRef = useRef<HTMLInputElement>(null);
    const contentRef = useRef<HTMLDivElement>(null);
    const threadRef = useRef<HTMLDivElement>(null);
    const hoveringRef = useRef(false);
    const searchSeq = useRef(0);
    const askSeq = useRef(0);
    const previousStage = useRef<NotchStage>("closed");
    const voiceRef = useRef<VoiceCapture | null>(null);
    // Read by `ask`, which runs from async callbacks (a finished recording)
    // where the render's own `turns` may already be a generation behind.
    const turnsRef = useRef<ConversationTurn[]>([]);
    turnsRef.current = turns;

    const style = styleFor(geometry.is_physical_notch);
    const closedSize = useMemo(
        () => ({ width: geometry.closed_width, height: geometry.closed_height }),
        [geometry.closed_width, geometry.closed_height]
    );
    const topOffset = style === "pill" ? notchMetrics.pillTopGap : 0;
    const conversing = turns.length > 0;
    const voiceAvailable = useMemo(() => isVoiceCaptureAvailable(), []);

    // MARK: - Target silhouette

    const target = useMemo(() => {
        const body = bodySizeFor(stage, style, closedSize, contentHeight);
        const topRadius = topRadiusFor(stage, style, closedSize.height);
        const bottomRadius = bottomRadiusFor(stage, style, closedSize.height);
        // Hover grows the panel a touch; the open panel is already at its size.
        const bump = hovering && stage !== "open" ? notchMetrics.hoverBump : 0;
        return {
            width: body.width + flareAllowance(topRadius, style) + bump,
            height: body.height + bump,
            topRadius,
            bottomRadius,
        };
    }, [stage, style, closedSize, contentHeight, hovering]);

    const width = useMotionValue(target.width);
    const height = useMotionValue(target.height);
    const topRadius = useMotionValue(target.topRadius);
    const bottomRadius = useMotionValue(target.bottomRadius);

    useEffect(() => {
        const transition = stageSpring(previousStage.current, stage);
        previousStage.current = stage;
        const controls = [
            animate(width, target.width, transition),
            animate(height, target.height, transition),
            animate(topRadius, target.topRadius, transition),
            animate(bottomRadius, target.bottomRadius, transition),
        ];
        return () => controls.forEach((control) => control.stop());
    }, [target, stage, width, height, topRadius, bottomRadius]);

    const clipPath = useTransform(
        [width, height, topRadius, bottomRadius],
        ([currentWidth, currentHeight, currentTop, currentBottom]: number[]) =>
            notchClipPath({
                width: currentWidth,
                height: currentHeight,
                topRadius: currentTop,
                bottomRadius: currentBottom,
                style,
            })
    );

    // An answer is as long as it is, so the panel follows what is actually
    // rendered rather than a row count.
    useEffect(() => {
        const element = contentRef.current;
        if (!element || typeof ResizeObserver === "undefined") {
            return;
        }
        // `offsetHeight` rather than the entry's content box: the panel has to
        // grow by the content's padding too, which `contentRect` leaves out.
        const observer = new ResizeObserver(() => setContentHeight(element.offsetHeight));
        observer.observe(element);
        return () => observer.disconnect();
    }, []);

    // Newest turn in view as answers land. `scrollTo` is optional-called: it
    // does not exist in jsdom, and a missing scroll must not break the panel.
    useEffect(() => {
        const thread = threadRef.current;
        thread?.scrollTo?.({ top: thread.scrollHeight, behavior: "smooth" });
    }, [turns]);

    // MARK: - Click-through tracking
    //
    // macOS routes a click by window frame, not by what is drawn, so the window
    // only stops ignoring the mouse while the cursor is inside this rectangle.

    useEffect(() => {
        let pending: number | null = null;
        const report = () => {
            pending = null;
            const currentWidth = width.get();
            void setNotchHudHitRect({
                x: (geometry.window_width - currentWidth) / 2,
                y: topOffset,
                width: currentWidth,
                height: height.get(),
            }).catch(() => undefined);
        };
        const schedule = () => {
            if (pending !== null) {
                return;
            }
            pending = window.setTimeout(report, notchMetrics.hitRectThrottleMs);
        };
        report();
        const unsubscribe = [width.on("change", schedule), height.on("change", schedule)];
        return () => {
            if (pending !== null) {
                window.clearTimeout(pending);
            }
            unsubscribe.forEach((stop) => stop());
        };
    }, [width, height, geometry.window_width, topOffset]);

    // MARK: - Backend wiring

    useEffect(() => {
        getNotchHudGeometry()
            .then(setGeometry)
            .catch(() => undefined);
    }, []);

    useTauriEvent<NotchHudGeometry>(NOTCH_HUD_GEOMETRY_EVENT, setGeometry);

    useTauriEvent<boolean>(NOTCH_HUD_HOVER_EVENT, (inside) => {
        hoveringRef.current = inside;
        setHovering(inside);
    });

    useEffect(() => () => voiceRef.current?.cancel(), []);

    const closePanel = useCallback(() => {
        voiceRef.current?.cancel();
        voiceRef.current = null;
        setStage(hoveringRef.current ? "peek" : "closed");
        setQuery("");
        setResults([]);
        setTurns([]);
        setSelectedIndex(NO_SELECTION);
        setSearching(false);
        setRecording(false);
        setVoiceStream(null);
        setTranscribing(false);
        setVoiceError(null);
        setContentHeight(notchMetrics.openHeaderHeight);
        void setNotchHudKeyboard(false).catch(() => undefined);
    }, []);

    const openPanel = useCallback(() => {
        setStage("open");
        void setNotchHudKeyboard(true).catch(() => undefined);
        window.requestAnimationFrame(() => inputRef.current?.focus());
    }, []);

    // Peek follows the pointer; the open panel outlives it, since the cursor
    // leaves the moment the user starts typing.
    useEffect(() => {
        if (stage === "open") {
            return;
        }
        if (hovering) {
            setStage("peek");
            return;
        }
        const timer = window.setTimeout(() => setStage("closed"), notchMetrics.hoverExitGraceMs);
        return () => window.clearTimeout(timer);
    }, [hovering, stage]);

    // MARK: - Browse

    // Live results while typing, so the panel is useful before the model has
    // said anything. Suspended during a conversation — the thread owns the
    // panel then.
    useEffect(() => {
        if (stage !== "open" || conversing) {
            return;
        }
        const trimmed = query.trim();
        const seq = ++searchSeq.current;
        setSearching(true);
        const timer = window.setTimeout(
            () => {
                const request = trimmed
                    ? searchMemoryCards(trimmed, undefined, undefined, notchMetrics.openMaxRows)
                    : listMemoryCards(notchMetrics.openMaxRows);
                request
                    .then((cards) => {
                        if (searchSeq.current !== seq) {
                            return;
                        }
                        setResults(cards.slice(0, notchMetrics.openMaxRows));
                        setSelectedIndex(NO_SELECTION);
                        setSearching(false);
                    })
                    .catch(() => {
                        if (searchSeq.current !== seq) {
                            return;
                        }
                        setResults([]);
                        setSearching(false);
                    });
            },
            trimmed ? SEARCH_DEBOUNCE_MS : 0
        );
        return () => window.clearTimeout(timer);
    }, [query, stage, conversing]);

    const openMemory = useCallback(
        (card: MemoryCard | undefined) => {
            if (!card) {
                return;
            }
            void notchHudOpenMemory(card.id).catch(() => undefined);
            closePanel();
        },
        [closePanel]
    );

    // MARK: - Ask

    const ask = useCallback(
        (question: string) => {
            const asked = question.trim();
            if (!asked) {
                return;
            }
            const id = `turn-${++askSeq.current}`;
            const priorQuestions = answeredQuestions(turnsRef.current);
            setTurns((current) => appendTurn(current, { id, question: asked, status: "thinking" }));
            setQuery("");
            setResults([]);
            setSelectedIndex(NO_SELECTION);
            setSearching(false);

            fndrAnswer(conversationQuery(priorQuestions, asked), notchMetrics.answerCardLimit)
                .then((composed) => {
                    setTurns((current) =>
                        resolveTurn(current, id, {
                            status: "answered",
                            answer: composed.answer,
                            cards: composed.cards.slice(0, notchMetrics.answerCardLimit),
                        })
                    );
                })
                .catch((error: unknown) => {
                    setTurns((current) =>
                        resolveTurn(current, id, {
                            status: "failed",
                            error: error instanceof Error ? error.message : String(error),
                        })
                    );
                });
        },
        []
    );

    // MARK: - Voice

    const toggleVoice = useCallback(async () => {
        setVoiceError(null);
        const active = voiceRef.current;
        if (active?.isRecording) {
            setRecording(false);
            setVoiceStream(null);
            setTranscribing(true);
            try {
                const clip = await active.stop();
                if (!clip) {
                    setVoiceError("Too short — hold the mic a little longer.");
                    return;
                }
                const result = await transcribeVoiceInput(clip.audioBytes, clip.mimeType);
                const spoken = result.text?.trim();
                if (!spoken) {
                    setVoiceError("Nothing came through.");
                    return;
                }
                ask(spoken);
            } catch (error: unknown) {
                setVoiceError(error instanceof Error ? error.message : "Transcription failed.");
            } finally {
                setTranscribing(false);
            }
            return;
        }

        const capture = new VoiceCapture();
        voiceRef.current = capture;
        try {
            await capture.start();
            setRecording(true);
            setVoiceStream(capture.mediaStream);
        } catch {
            voiceRef.current = null;
            setVoiceError("Microphone access failed.");
        }
    }, [ask]);

    // MARK: - Keyboard

    const onKeyDown = useCallback(
        (event: React.KeyboardEvent) => {
            if (event.key === "Escape") {
                event.preventDefault();
                // Esc backs out one layer: first the thread, then the panel.
                if (conversing) {
                    setTurns([]);
                    setContentHeight(notchMetrics.openHeaderHeight);
                    return;
                }
                closePanel();
                return;
            }
            if (event.key === "ArrowDown") {
                event.preventDefault();
                setSelectedIndex((index) => Math.min(index + 1, results.length - 1));
                return;
            }
            if (event.key === "ArrowUp") {
                event.preventDefault();
                setSelectedIndex((index) => Math.max(index - 1, NO_SELECTION));
                return;
            }
            if (event.key === "Enter") {
                event.preventDefault();
                // A highlighted row opens; otherwise the question goes to FNDR.
                if (selectedIndex !== NO_SELECTION && results[selectedIndex]) {
                    openMemory(results[selectedIndex]);
                    return;
                }
                ask(query);
            }
        },
        [ask, closePanel, conversing, openMemory, query, results, selectedIndex]
    );

    const thinking = turns.some((turn) => turn.status === "thinking");
    const busy = searching || thinking || transcribing;

    return (
        <div
            className="notch-root"
            style={{ width: geometry.window_width, height: geometry.window_height }}
        >
            <motion.div
                className="notch-shadow"
                style={{ marginTop: topOffset }}
                animate={{ opacity: stage === "closed" ? 0 : 1 }}
                transition={{ duration: 0.18 }}
            >
                <motion.div
                    className="notch-panel"
                    data-stage={stage}
                    style={{ width, height, clipPath }}
                    onMouseDown={(event) => {
                        if (stage !== "open") {
                            event.preventDefault();
                            openPanel();
                        }
                    }}
                    // Keys are handled once, here: a handler on the input as
                    // well would run twice for every keystroke that bubbles.
                    onKeyDown={onKeyDown}
                >
                    {/* Content dissolves as the silhouette collapses instead of
                        being abruptly clipped by it. */}
                    <motion.div
                        className="notch-peek"
                        aria-hidden={stage !== "peek"}
                        animate={{
                            opacity: stage === "peek" ? 1 : 0,
                            filter: stage === "peek" ? "blur(0px)" : "blur(12px)",
                            scale: stage === "peek" ? 1 : 0.86,
                        }}
                        transition={{ duration: 0.2 }}
                    >
                        <span className="notch-mark" aria-hidden="true" />
                        <span className="notch-peek-label">Ask FNDR</span>
                        <span className="notch-peek-hint">click</span>
                    </motion.div>

                    <motion.div
                        className="notch-open"
                        aria-hidden={stage !== "open"}
                        style={{
                            width: notchMetrics.openWidth,
                            paddingLeft: notchMetrics.openContentInset + target.topRadius,
                            paddingRight: notchMetrics.openContentInset + target.topRadius,
                            pointerEvents: stage === "open" ? "auto" : "none",
                        }}
                        animate={{
                            opacity: stage === "open" ? 1 : 0,
                            filter: stage === "open" ? "blur(0px)" : "blur(30px)",
                            scale: stage === "open" ? 1 : 0.4,
                        }}
                        transition={{ duration: 0.22 }}
                    >
                        <div ref={contentRef} className="notch-content">
                            <VoiceBeam
                                stream={voiceStream ?? undefined}
                                processing={busy}
                                theme="dark"
                                active={stage === "open"}
                            >
                                <div className="notch-input-row">
                                    <span
                                        className={
                                            busy ? "notch-mark notch-mark-busy" : "notch-mark"
                                        }
                                        aria-hidden="true"
                                    />
                                    <input
                                        ref={inputRef}
                                        className="notch-input"
                                        value={query}
                                        placeholder={
                                            conversing ? "Ask a follow-up" : "Ask FNDR anything"
                                        }
                                        aria-label="Ask FNDR"
                                        spellCheck={false}
                                        onChange={(event) => setQuery(event.target.value)}
                                    />
                                    {voiceAvailable ? (
                                        <button
                                            type="button"
                                            className={
                                                recording
                                                    ? "notch-mic notch-mic-recording"
                                                    : "notch-mic"
                                            }
                                            aria-label={
                                                recording ? "Stop and send" : "Speak to FNDR"
                                            }
                                            aria-pressed={recording}
                                            onClick={() => void toggleVoice()}
                                        >
                                            <svg viewBox="0 0 24 24" aria-hidden="true">
                                                <rect x="9" y="3" width="6" height="11" rx="3" />
                                                <path d="M5 11a7 7 0 0 0 14 0" />
                                                <path d="M12 18v3" />
                                            </svg>
                                        </button>
                                    ) : null}
                                </div>
                            </VoiceBeam>

                            {voiceError ? (
                                <p className="notch-voice-error" role="status">
                                    {voiceError}
                                </p>
                            ) : null}

                            {conversing ? (
                                <div ref={threadRef} className="notch-thread">
                                    {turns.map((turn) => (
                                        <div key={turn.id} className="notch-turn">
                                            <p className="notch-question">{turn.question}</p>
                                            {turn.status === "thinking" ? (
                                                <p className="notch-thinking" role="status">
                                                    <ThinkingOrb
                                                        state="searching"
                                                        size={20}
                                                        theme="dark"
                                                    />
                                                    Reading your memory
                                                </p>
                                            ) : null}
                                            {turn.status === "answered" ? (
                                                <p className="notch-answer">{turn.answer}</p>
                                            ) : null}
                                            {turn.status === "failed" ? (
                                                <p className="notch-answer notch-answer-failed">
                                                    {turn.error}
                                                </p>
                                            ) : null}
                                            {turn.cards && turn.cards.length > 0 ? (
                                                <div className="notch-cites">
                                                    {turn.cards.map((card) => (
                                                        <button
                                                            key={card.id}
                                                            type="button"
                                                            className="notch-cite"
                                                            onClick={() => openMemory(card)}
                                                        >
                                                            {card.app_name} · {card.title}
                                                        </button>
                                                    ))}
                                                </div>
                                            ) : null}
                                        </div>
                                    ))}
                                </div>
                            ) : null}

                            {!conversing && results.length > 0 ? (
                                <div className="notch-results" role="listbox">
                                    {results.map((card, index) => (
                                        <button
                                            key={card.id}
                                            type="button"
                                            role="option"
                                            aria-selected={index === selectedIndex}
                                            className={
                                                index === selectedIndex
                                                    ? "notch-row notch-row-selected"
                                                    : "notch-row"
                                            }
                                            onMouseEnter={() => setSelectedIndex(index)}
                                            onClick={() => openMemory(card)}
                                        >
                                            <span className="notch-row-title">{card.title}</span>
                                            <span className="notch-row-meta">{card.app_name}</span>
                                        </button>
                                    ))}
                                </div>
                            ) : null}

                            <p className="notch-footer">
                                <span>⏎ ask</span>
                                {!conversing && results.length > 0 ? <span>↓ browse</span> : null}
                                <span>esc {conversing ? "clear" : "close"}</span>
                            </p>
                        </div>
                    </motion.div>
                </motion.div>
            </motion.div>
        </div>
    );
}

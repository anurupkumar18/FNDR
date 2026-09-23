import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
    type AutofillCandidate,
    type AutofillOverlayEvent,
    type AutofillOverlayPayload,
    type AutofillResolution,
    type FieldContext,
    dismissAutofill,
    injectText,
    resolveAutofill,
    setAutofillOverlayReady,
    takePendingAutofillPayload,
} from "@/shared/ipc/tauri";

const SUCCESS_TOAST_MS = 900;
const ERROR_TOAST_MS = 6000;

type Phase =
    | { kind: "idle" }
    | {
        kind: "searching";
        label: string;
        appName: string;
        windowTitle: string;
        contextHint: string;
    }
    | {
        kind: "manual";
        appName: string;
        windowTitle: string;
        contextHint: string;
        message?: string;
    }
    | { kind: "waiting" }
    | {
        kind: "preview";
        label: string;
        resolution: AutofillResolution;
        selectedIndex: number;
        appName: string;
        windowTitle: string;
        contextHint: string;
    }
    | { kind: "injecting"; label: string; candidate: AutofillCandidate }
    | { kind: "done"; label: string; candidate: AutofillCandidate }
    | { kind: "error"; title: string; message: string };

interface AutofillErrorCopy {
    title: string;
    message: string;
}

function autofillErrorCopy(error: unknown): AutofillErrorCopy {
    const raw = error instanceof Error ? error.message : String(error);
    const normalized = raw.toLowerCase();

    if (normalized.includes("accessibility") || normalized.includes("permission denied")) {
        return {
            title: "Autofill needs Accessibility permission",
            message: "Allow FNDR in System Settings, then focus the field and try again.",
        };
    }

    if (
        normalized.includes("no autofill target")
        || normalized.includes("target app")
        || normalized.includes("activate target")
        || normalized.includes("focused field")
    ) {
        return {
            title: "The focused field changed",
            message: "Focus the destination field and run Autofill again.",
        };
    }

    return {
        title: "Autofill couldn’t finish",
        message: "Nothing was inserted. Focus the destination field and try again.",
    };
}

function normalizePhrase(input: string): string {
    return input
        .toLowerCase()
        .replace(/[^a-z0-9#]+/g, " ")
        .trim()
        .replace(/\s+/g, " ");
}

function confidenceLabel(confidence: number): string {
    return `${Math.round(confidence * 100)}% match`;
}

function confidenceTone(confidence: number): "high" | "medium" | "low" {
    if (confidence >= 0.94) return "high";
    if (confidence >= 0.84) return "medium";
    return "low";
}

function timeAgo(timestampMs: number): string {
    const delta = Date.now() - timestampMs;
    const days = Math.floor(delta / 86_400_000);
    if (days <= 0) return "today";
    if (days === 1) return "yesterday";
    if (days < 7) return `${days}d ago`;
    if (days < 30) return `${Math.floor(days / 7)}w ago`;
    return `${Math.floor(days / 30)}mo ago`;
}

function labelFromContext(context: FieldContext): string {
    return context.label || context.inferred_label || context.placeholder || "";
}

function contextHintFromContext(context: FieldContext): string {
    return context.screen_context || context.window_title || context.app_name || "";
}

function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) {
        return false;
    }
    return (
        target.tagName === "INPUT"
        || target.tagName === "TEXTAREA"
        || target.isContentEditable
    );
}

function isErrorPayload(payload: AutofillOverlayPayload): payload is { error: string } {
    return "error" in payload;
}

function isScanningPayload(
    payload: AutofillOverlayPayload,
): payload is { scanning: true; message?: string } {
    return "scanning" in payload;
}

export function AutofillOverlay() {
    const [phase, setPhase] = useState<Phase>({ kind: "idle" });
    const [query, setQuery] = useState("");
    const [overlayVisible, setOverlayVisible] = useState(false);
    const contextRef = useRef<FieldContext | null>(null);
    const queryInputRef = useRef<HTMLInputElement>(null);
    const dismissTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const resolveTokenRef = useRef(0);
    const activeRequestRef = useRef<number | null>(null);

    function clearDismissTimer() {
        if (dismissTimer.current) {
            clearTimeout(dismissTimer.current);
            dismissTimer.current = null;
        }
    }

    function hideOverlay(requestId: number | null) {
        void dismissAutofill(requestId).catch(() => {});
    }

    function resetAndHide(expectedRequest = activeRequestRef.current) {
        if (
            expectedRequest !== null
            && activeRequestRef.current !== expectedRequest
        ) {
            return;
        }
        clearDismissTimer();
        resolveTokenRef.current += 1;
        activeRequestRef.current = null;
        setOverlayVisible(false);
        setPhase({ kind: "idle" });
        setQuery("");
        hideOverlay(expectedRequest);
    }

    function scheduleDismiss(delayMs: number) {
        clearDismissTimer();
        const requestId = activeRequestRef.current;
        dismissTimer.current = setTimeout(() => {
            resetAndHide(requestId);
        }, delayMs);
    }

    function showManual(context: FieldContext, message?: string) {
        setOverlayVisible(true);
        setPhase({
            kind: "manual",
            appName: context.app_name,
            windowTitle: context.window_title,
            contextHint: contextHintFromContext(context),
            message,
        });
    }

    async function acceptCandidate(label: string, candidate: AutofillCandidate) {
        const requestId = activeRequestRef.current;
        if (requestId === null) {
            return;
        }
        clearDismissTimer();
        resolveTokenRef.current += 1;
        setOverlayVisible(true);

        try {
            setPhase({ kind: "injecting", label, candidate });
            await injectText(candidate.value, requestId);
            if (activeRequestRef.current !== requestId) {
                return;
            }
            setPhase({ kind: "done", label, candidate });
            scheduleDismiss(SUCCESS_TOAST_MS);
        } catch (error) {
            if (activeRequestRef.current !== requestId) {
                return;
            }
            setPhase({ kind: "error", ...autofillErrorCopy(error) });
            scheduleDismiss(ERROR_TOAST_MS);
        }
    }

    async function runResolution(context: FieldContext, nextQuery?: string) {
        const pendingLabel = (nextQuery ?? query ?? labelFromContext(context)).trim();
        if (!pendingLabel) {
            showManual(context, "No field label was detected yet. Type what you want FNDR to find.");
            return;
        }

        clearDismissTimer();
        setQuery(pendingLabel);
        setOverlayVisible(true);

        const token = resolveTokenRef.current + 1;
        resolveTokenRef.current = token;

        setPhase({
            kind: "searching",
            label: pendingLabel,
            appName: context.app_name,
            windowTitle: context.window_title,
            contextHint: contextHintFromContext(context),
        });

        try {
            const resolution = await resolveAutofill(context, pendingLabel);
            if (token !== resolveTokenRef.current) {
                return;
            }

            const label = resolution.query || pendingLabel;
            setQuery(label);

            if (resolution.candidates.length === 0) {
                showManual(context, `No strong matches for "${pendingLabel}" yet. Refine the search and press Enter again.`);
                return;
            }

            const topCandidate = resolution.candidates[0];
            if (
                !resolution.requires_confirmation
                && topCandidate.confidence >= resolution.auto_inject_threshold
            ) {
                await acceptCandidate(label, topCandidate);
                return;
            }

            setPhase({
                kind: "preview",
                label,
                resolution,
                selectedIndex: 0,
                appName: context.app_name,
                windowTitle: context.window_title,
                contextHint: contextHintFromContext(context),
            });
        } catch (error) {
            if (token !== resolveTokenRef.current) {
                return;
            }
            setPhase({ kind: "error", ...autofillErrorCopy(error) });
            scheduleDismiss(ERROR_TOAST_MS);
        }
    }

    async function syncPendingPayload(showFallback = false) {
        setOverlayVisible(true);
        try {
            const pending = await takePendingAutofillPayload();
            if (pending) {
                await handlePayload(pending);
                return true;
            }
        } catch {
            // Keep the overlay visible even if the payload fetch fails once.
        }

        if (showFallback) {
            setPhase((current) =>
                current.kind === "idle"
                    ? { kind: "waiting" }
                    : current,
            );
        }

        return false;
    }

    async function handlePayload(event: AutofillOverlayEvent) {
        const activeRequest = activeRequestRef.current;
        if (activeRequest !== null && event.requestId < activeRequest) {
            return;
        }
        if (activeRequest !== event.requestId) {
            resolveTokenRef.current += 1;
            contextRef.current = null;
            setPhase({ kind: "idle" });
            setQuery("");
        }
        activeRequestRef.current = event.requestId;
        clearDismissTimer();
        setOverlayVisible(true);
        const payload: AutofillOverlayPayload = event.payload;

        if (isScanningPayload(payload)) {
            setPhase({
                kind: "searching",
                label: payload.message || "Searching memories",
                appName: "FNDR",
                windowTitle: "",
                contextHint: "",
            });
            void syncPendingPayload(false);
            return;
        }

        if (isErrorPayload(payload)) {
            setPhase({ kind: "error", ...autofillErrorCopy(payload.error) });
            scheduleDismiss(ERROR_TOAST_MS);
            return;
        }

        const context = payload;
        contextRef.current = context;

        const seededQuery = labelFromContext(context);
        setQuery(seededQuery);

        if (!seededQuery && !context.screen_context.trim()) {
            showManual(context);
            return;
        }

        await runResolution(context, seededQuery || undefined);
    }

    useEffect(() => {
        let unlisten: UnlistenFn | null = null;
        let isMounted = true;

        void setAutofillOverlayReady(true)
            .then((pending) => {
                if (isMounted && pending) {
                    void handlePayload(pending);
                }
            })
            .catch(() => {
                // The native window may still be booting. A focus event retries the handoff.
            });

        listen<AutofillOverlayEvent>("autofill-triggered", (event) => {
            void handlePayload(event.payload);
        }).then((fn) => {
            unlisten = fn;
        }).catch(() => {
            // Tauri event registration is retried with the next auxiliary-window load.
        });

        function handleWindowVisible() {
            const visible = document.visibilityState === "visible" || document.hasFocus();
            setOverlayVisible(visible);
            if (visible) {
                void syncPendingPayload(true);
            }
        }

        window.addEventListener("focus", handleWindowVisible);
        document.addEventListener("visibilitychange", handleWindowVisible);

        return () => {
            isMounted = false;
            clearDismissTimer();
            resolveTokenRef.current += 1;
            unlisten?.();
            window.removeEventListener("focus", handleWindowVisible);
            document.removeEventListener("visibilitychange", handleWindowVisible);
            void setAutofillOverlayReady(false).catch(() => {});
        };
    }, []);

    // Only auto-focus the search input in manual mode. In preview mode the input
    // being focused would intercept ArrowUp/Down/1-9 keyboard shortcuts (isEditableTarget
    // guard in the key handler ignores those keys when an input is focused).
    useEffect(() => {
        if (phase.kind !== "manual") {
            return;
        }

        const timer = window.setTimeout(() => {
            queryInputRef.current?.focus();
            queryInputRef.current?.select();
        }, 40);

        return () => window.clearTimeout(timer);
    }, [phase.kind]);

    const selectedCandidate =
        phase.kind === "preview"
            ? phase.resolution.candidates[phase.selectedIndex] ?? phase.resolution.candidates[0]
            : null;

    const queryMatchesSelection = useMemo(() => {
        if (phase.kind !== "preview") {
            return false;
        }
        return normalizePhrase(query) === normalizePhrase(phase.label);
    }, [phase, query]);

    useEffect(() => {
        function handleKey(event: KeyboardEvent) {
            if (phase.kind === "idle") {
                return;
            }

            if (event.key === "Escape") {
                event.preventDefault();
                resetAndHide();
                return;
            }

            if (phase.kind !== "preview" || isEditableTarget(event.target)) {
                return;
            }

            if (event.key === "Enter") {
                event.preventDefault();
                const candidate = phase.resolution.candidates[phase.selectedIndex];
                if (candidate) {
                    void acceptCandidate(phase.label, candidate);
                }
                return;
            }

            if (event.key === "ArrowDown") {
                event.preventDefault();
                setPhase((current) =>
                    current.kind !== "preview"
                        ? current
                        : {
                            ...current,
                            selectedIndex: Math.min(
                                current.selectedIndex + 1,
                                current.resolution.candidates.length - 1,
                            ),
                        },
                );
                return;
            }

            if (event.key === "ArrowUp") {
                event.preventDefault();
                setPhase((current) =>
                    current.kind !== "preview"
                        ? current
                        : {
                            ...current,
                            selectedIndex: Math.max(current.selectedIndex - 1, 0),
                        },
                );
                return;
            }

            if (/^[1-9]$/.test(event.key)) {
                const index = Number(event.key) - 1;
                const candidate = phase.resolution.candidates[index];
                if (candidate) {
                    event.preventDefault();
                    void acceptCandidate(phase.label, candidate);
                }
            }
        }

        window.addEventListener("keydown", handleKey);
        return () => window.removeEventListener("keydown", handleKey);
    }, [phase]);

    const showBootstrapSurface = phase.kind === "idle" && overlayVisible;

    if (phase.kind === "idle" && !showBootstrapSurface) {
        return null;
    }

    const showSearchSurface =
        showBootstrapSurface
        || phase.kind === "searching"
        || phase.kind === "waiting"
        || phase.kind === "manual"
        || phase.kind === "preview";
    const searchButtonLabel =
        phase.kind === "preview" && queryMatchesSelection
            ? "Insert"
            : phase.kind === "searching" || showBootstrapSurface
                ? "Searching..."
                : phase.kind === "waiting"
                    ? "Waiting"
                : "Search";
    const contextAppName =
        phase.kind === "manual" || phase.kind === "preview" || phase.kind === "searching"
            ? phase.appName
            : contextRef.current?.app_name ?? "FNDR";
    const contextWindowTitle =
        phase.kind === "manual" || phase.kind === "preview" || phase.kind === "searching"
            ? phase.windowTitle
            : contextRef.current?.window_title ?? "";
    const contextHint =
        phase.kind === "manual" || phase.kind === "preview" || phase.kind === "searching"
            ? phase.contextHint
            : contextRef.current?.screen_context ?? "";

    async function submitQuery(event: FormEvent) {
        event.preventDefault();
        const context = contextRef.current;
        if (!context) {
            return;
        }

        const nextQuery = query.trim();
        if (phase.kind === "preview" && selectedCandidate && queryMatchesSelection) {
            await acceptCandidate(phase.label, selectedCandidate);
            return;
        }

        await runResolution(context, nextQuery);
    }

    return (
        <div
            className="af-overlay"
            role="dialog"
            aria-modal="false"
            aria-label="FNDR Autofill"
            aria-busy={phase.kind === "searching" || phase.kind === "injecting"}
        >
            {showSearchSurface && (
                <div className={`af-card af-main-card ${phase.kind}`}>
                    <div className="af-header">
                        <div className="af-brand">
                            <span className="af-brand-mark">FNDR</span>
                            <span className="af-brand-state">
                                {phase.kind === "preview"
                                    ? "Review match"
                                    : phase.kind === "manual"
                                        ? "Search Memory"
                                        : phase.kind === "waiting"
                                            ? "Waiting for field"
                                            : "Searching"}
                            </span>
                        </div>
                        <button
                            className="af-close"
                            onClick={() => resetAndHide()}
                            aria-label="Dismiss Autofill"
                            type="button"
                        >
                            ×
                        </button>
                    </div>

                    <form className="af-search-row" onSubmit={(event) => void submitQuery(event)}>
                        <input
                            ref={queryInputRef}
                            className="af-search-input"
                            type="search"
                            aria-label="Memory search for this field"
                            value={query}
                            onChange={(event) => setQuery(event.target.value)}
                            onKeyDown={(event) => {
                                if (event.key === "Escape") {
                                    event.preventDefault();
                                    resetAndHide();
                                }
                            }}
                            placeholder="Search for policy number, EIN, member ID..."
                            autoComplete="off"
                            spellCheck={false}
                            disabled={showBootstrapSurface || phase.kind === "waiting"}
                        />
                        <button
                            className="af-search-btn"
                            type="submit"
                            disabled={
                                phase.kind === "searching"
                                || phase.kind === "waiting"
                                || showBootstrapSurface
                            }
                        >
                            {searchButtonLabel}
                        </button>
                    </form>

                    <div className="af-context-row">
                        <span className="af-context-app" aria-label="Focused app">
                            {contextAppName || "FNDR"}
                        </span>
                        <span className="af-context-window" aria-label="Focused window">
                            {contextWindowTitle || "Waiting for a focused window"}
                        </span>
                    </div>

                    {(showBootstrapSurface || phase.kind === "searching" || phase.kind === "waiting") && (
                        <>
                            <div className="af-searching-panel">
                                {phase.kind !== "waiting" && <span className="af-spinner" aria-hidden />}
                                <div className="af-searching-copy">
                                    <span className="af-searching-title">
                                        {phase.kind === "waiting"
                                            ? "No field context available"
                                            : "Searching memories"}
                                    </span>
                                    <span className="af-searching-value">
                                        {phase.kind === "waiting"
                                            ? "Focus a text field, then run Autofill again."
                                            : showBootstrapSurface
                                            ? "Preparing the focused field context"
                                            : phase.kind === "searching"
                                                ? phase.label
                                                : "Using visible form context"}
                                    </span>
                                </div>
                            </div>
                            {contextHint && (
                                <div className="af-context-box">
                                    <span className="af-context-label">Visible form context</span>
                                    <span className="af-context-text">{contextHint}</span>
                                </div>
                            )}
                            <div className="af-footer-hint">
                                {phase.kind === "waiting"
                                    ? "Nothing will be inserted until a new field request is available."
                                    : "FNDR ranks local memories using the active field and nearby visible context."}
                            </div>
                        </>
                    )}

                    {phase.kind === "manual" && (
                        <>
                            <div className="af-empty-state">
                                <span className="af-empty-title">Search memory for this field</span>
                                <span className="af-empty-copy">
                                    FNDR needs a better search phrase for this form input. Edit the query and press Enter.
                                </span>
                            </div>
                            {phase.message && (
                                <div className="af-banner af-banner-soft">{phase.message}</div>
                            )}
                            {contextHint && (
                                <div className="af-context-box">
                                    <span className="af-context-label">Visible form context</span>
                                    <span className="af-context-text">{contextHint}</span>
                                </div>
                            )}
                            <div className="af-footer-hint">Enter searches again. Esc closes.</div>
                        </>
                    )}

                    {phase.kind === "preview" && selectedCandidate && (
                        <>
                            <div className="af-selection-card">
                                <div className="af-selection-top">
                                    <span className="af-field-chip">{phase.label}</span>
                                    <span
                                        className={`af-confidence-badge ${confidenceTone(selectedCandidate.confidence)}`}
                                    >
                                        {confidenceLabel(selectedCandidate.confidence)}
                                    </span>
                                </div>
                                <span className="af-selection-value">{selectedCandidate.value}</span>
                                <span className="af-selection-reason">
                                    {selectedCandidate.match_reason}
                                </span>
                                <div className="af-selection-source">
                                    <span>
                                        {selectedCandidate.source_window_title || selectedCandidate.source_app}
                                    </span>
                                    <span>{timeAgo(selectedCandidate.timestamp)}</span>
                                </div>
                            </div>

                            {selectedCandidate.confidence < phase.resolution.auto_inject_threshold && (
                                <div className="af-banner af-banner-warn">
                                    Context was strong enough to rank this memory first, but FNDR wants confirmation before inserting it.
                                </div>
                            )}

                            {phase.resolution.used_ocr_fallback && (
                                <div className="af-banner af-banner-soft">
                                    FNDR used nearby visible text because the field label was not available.
                                </div>
                            )}

                            {selectedCandidate.source_snippet && (
                                <div className="af-context-box">
                                    <span className="af-context-label">Why this memory</span>
                                    <span className="af-context-text">{selectedCandidate.source_snippet}</span>
                                </div>
                            )}

                            {phase.resolution.candidates.length > 1 && (
                                <div className="af-candidate-list" role="listbox" aria-label="Autofill matches">
                                    {phase.resolution.candidates.map((candidate, index) => (
                                        <button
                                            key={`${candidate.memory_id}-${candidate.value}-${index}`}
                                            type="button"
                                            role="option"
                                            aria-selected={index === phase.selectedIndex}
                                            className={`af-candidate ${index === phase.selectedIndex ? "selected" : ""}`}
                                            onClick={() =>
                                                setPhase((current) =>
                                                    current.kind !== "preview"
                                                        ? current
                                                        : { ...current, selectedIndex: index },
                                                )
                                            }
                                        >
                                            <span className="af-candidate-rank">{index + 1}</span>
                                            <div className="af-candidate-copy">
                                                <span className="af-candidate-value">{candidate.value}</span>
                                                <span className="af-candidate-meta">
                                                    {confidenceLabel(candidate.confidence)} · {timeAgo(candidate.timestamp)}
                                                </span>
                                            </div>
                                        </button>
                                    ))}
                                </div>
                            )}

                            <div className="af-actions">
                                <button
                                    type="button"
                                    className="af-primary"
                                    onClick={() => void acceptCandidate(phase.label, selectedCandidate)}
                                >
                                    Insert Selected
                                    <kbd>↵</kbd>
                                </button>
                                <button type="button" className="af-secondary" onClick={() => resetAndHide()}>
                                    Dismiss
                                    <kbd>Esc</kbd>
                                </button>
                            </div>
                            <div className="af-footer-hint">
                                Press Enter to insert the selected value. Up, Down, or 1-9 switches candidates.
                            </div>
                        </>
                    )}
                </div>
            )}

            {(phase.kind === "injecting" || phase.kind === "done" || phase.kind === "error") && (
                <div
                    className={`af-card af-inline-card ${phase.kind}`}
                    role={phase.kind === "error" ? "alert" : "status"}
                    aria-live={phase.kind === "error" ? "assertive" : "polite"}
                >
                    {phase.kind === "injecting" ? (
                        <span className="af-spinner" aria-hidden />
                    ) : (
                        <span className={`af-inline-icon ${phase.kind}`}>
                            {phase.kind === "done" ? "✓" : "!"}
                        </span>
                    )}
                    <div className="af-inline-copy">
                        <span className="af-inline-label">
                            {phase.kind === "injecting" && "Inserting into active field"}
                            {phase.kind === "done" && "Filled field"}
                            {phase.kind === "error" && phase.title}
                        </span>
                        <span className="af-inline-value">
                            {phase.kind === "injecting" && phase.candidate.value}
                            {phase.kind === "done"
                                && `${phase.label} from ${phase.candidate.source_window_title || phase.candidate.source_app}`}
                            {phase.kind === "error" && phase.message}
                        </span>
                    </div>
                    <button className="af-close" onClick={() => resetAndHide()} aria-label="Dismiss Autofill" type="button">
                        ×
                    </button>
                </div>
            )}

            <style>{`
                .af-overlay {
                    --af-text: var(--fg, #e8dfc8);
                    --af-text-secondary: var(--fg-2, #c4a878);
                    --af-text-muted: var(--fg-3, #8a7758);
                    --af-surface: var(--bg-2, #221915);
                    --af-raised: var(--bg-3, #2a2018);
                    --af-border: var(--hairline-2, rgba(232, 223, 200, 0.14));
                    --af-border-strong: var(--hairline-strong, rgba(232, 223, 200, 0.22));
                    --af-accent: var(--accent, #d4a04a);
                    --af-info: #78cdff;
                    --af-success: #91efae;
                    --af-danger: #ffb6a5;
                    position: fixed;
                    inset: 0;
                    display: flex;
                    align-items: stretch;
                    justify-content: stretch;
                    padding: 0;
                    pointer-events: none;
                    background:
                        radial-gradient(circle at top right, color-mix(in srgb, var(--af-accent) 14%, transparent), transparent 32%),
                        linear-gradient(155deg, var(--af-surface), var(--bg, #1a1410));
                    color: var(--af-text);
                    font-family: var(--film-font-ui, "SF Pro Text", "Helvetica Neue", system-ui, sans-serif);
                    -webkit-font-smoothing: antialiased;
                }

                .af-card {
                    pointer-events: all;
                    color: var(--af-text);
                    background: transparent;
                    animation: af-slide-in 0.18s cubic-bezier(0.25, 1, 0.5, 1) both;
                }

                .af-main-card {
                    display: flex;
                    flex-direction: column;
                    gap: 12px;
                    width: 100%;
                    min-height: 100%;
                    padding: 18px 18px 16px;
                    overflow-y: auto;
                    border: 1px solid var(--af-border);
                    box-shadow:
                        inset 0 1px 0 color-mix(in srgb, var(--af-text) 5%, transparent),
                        var(--shadow-medium, 0 18px 44px rgba(0, 0, 0, 0.28));
                }

                .af-inline-card {
                    display: flex;
                    align-items: center;
                    gap: 10px;
                    width: min(448px, calc(100vw - 24px));
                    height: auto;
                    max-height: calc(100vh - 24px);
                    overflow-y: auto;
                    padding: 12px 14px;
                    margin: auto 12px 12px auto;
                    border-radius: 22px;
                    border: 1px solid var(--af-border-strong);
                    background:
                        radial-gradient(circle at top right, color-mix(in srgb, var(--af-accent) 14%, transparent), transparent 30%),
                        linear-gradient(155deg, var(--af-surface), var(--bg, #1a1410));
                    box-shadow:
                        0 22px 56px rgba(0, 0, 0, 0.52),
                        inset 0 1px 0 rgba(255, 255, 255, 0.04);
                }

                .af-inline-card.done {
                    border-color: color-mix(in srgb, var(--af-success) 40%, transparent);
                    background:
                        radial-gradient(circle at top right, color-mix(in srgb, var(--af-success) 11%, transparent), transparent 30%),
                        var(--af-surface);
                }

                .af-inline-card.error {
                    border-color: color-mix(in srgb, var(--alarm, #c4521e) 42%, transparent);
                    background:
                        radial-gradient(circle at top right, color-mix(in srgb, var(--alarm, #c4521e) 12%, transparent), transparent 30%),
                        var(--af-surface);
                }

                .af-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 10px;
                }

                .af-brand {
                    display: flex;
                    flex-direction: column;
                    gap: 2px;
                }

                .af-brand-mark {
                    font-size: 10px;
                    font-weight: 900;
                    letter-spacing: 0.12em;
                    text-transform: uppercase;
                    color: var(--bg, #1a1410);
                    background: linear-gradient(135deg, var(--accent-2, #e8b85a), var(--af-accent));
                    padding: 3px 8px;
                    border-radius: 8px;
                    width: fit-content;
                    margin-bottom: 2px;
                    box-shadow: 0 4px 12px color-mix(in srgb, var(--af-accent) 22%, transparent);
                }

                .af-brand-state {
                    font-size: 12px;
                    color: var(--af-text-muted);
                }

                .af-close {
                    width: 44px;
                    height: 44px;
                    flex: 0 0 44px;
                    border-radius: 999px;
                    border: 1px solid var(--af-border);
                    background: color-mix(in srgb, var(--af-text) 5%, transparent);
                    color: var(--af-text-secondary);
                    font-size: 18px;
                    line-height: 1;
                    cursor: pointer;
                }

                .af-close:hover {
                    background: color-mix(in srgb, var(--af-text) 9%, transparent);
                    color: var(--af-text);
                }

                .af-close:focus-visible,
                .af-search-input:focus-visible,
                .af-search-btn:focus-visible,
                .af-candidate:focus-visible,
                .af-primary:focus-visible,
                .af-secondary:focus-visible {
                    outline: 2px solid var(--af-accent);
                    outline-offset: 2px;
                }

                .af-search-row {
                    display: grid;
                    grid-template-columns: 1fr auto;
                    gap: 8px;
                }

                .af-search-input {
                    width: 100%;
                    min-width: 0;
                    border-radius: 14px;
                    min-height: 44px;
                    border: 1px solid var(--af-border);
                    background:
                        linear-gradient(180deg, color-mix(in srgb, var(--af-text) 7%, transparent), color-mix(in srgb, var(--af-text) 3%, transparent));
                    padding: 12px 14px;
                    color: var(--af-text);
                    font-size: 14px;
                    outline: none;
                    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
                    caret-color: var(--af-info);
                }

                .af-search-input:focus {
                    border-color: color-mix(in srgb, var(--af-info) 60%, transparent);
                    box-shadow:
                        0 0 0 3px color-mix(in srgb, var(--af-info) 14%, transparent),
                        inset 0 1px 0 color-mix(in srgb, var(--af-text) 6%, transparent);
                }

                .af-search-input::placeholder {
                    color: var(--af-text-muted);
                }

                .af-search-input:disabled {
                    opacity: 0.72;
                    cursor: default;
                }

                .af-search-btn,
                .af-primary,
                .af-secondary {
                    border: 1px solid transparent;
                    border-radius: 14px;
                    font-family: inherit;
                    font-size: 13px;
                    font-weight: 600;
                    min-height: 44px;
                    cursor: pointer;
                    transition: transform 0.12s ease, background 0.12s ease, border-color 0.12s ease;
                }

                .af-search-btn {
                    padding: 0 14px;
                    background: color-mix(in srgb, var(--af-accent) 15%, var(--af-raised));
                    border-color: color-mix(in srgb, var(--af-accent) 38%, transparent);
                    color: var(--af-text);
                }

                .af-search-btn:disabled {
                    cursor: default;
                    opacity: 0.7;
                    transform: none;
                }

                .af-search-btn:hover:not(:disabled),
                .af-primary:hover,
                .af-secondary:hover {
                    transform: translateY(-1px);
                }

                .af-context-row {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    min-width: 0;
                    color: var(--af-text-muted);
                    font-size: 11px;
                }

                .af-context-app {
                    flex-shrink: 0;
                    padding: 3px 8px;
                    border-radius: 999px;
                    background: color-mix(in srgb, var(--af-text) 6%, transparent);
                    border: 1px solid var(--af-border);
                }

                .af-context-window {
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                }

                .af-searching-panel,
                .af-empty-state {
                    display: flex;
                    flex-direction: column;
                    gap: 6px;
                }

                .af-searching-panel {
                    flex-direction: row;
                    align-items: center;
                    gap: 12px;
                    padding: 4px 0;
                }

                .af-searching-copy {
                    display: flex;
                    flex-direction: column;
                    gap: 3px;
                    min-width: 0;
                }

                .af-searching-title,
                .af-empty-title {
                    font-size: 16px;
                    font-weight: 700;
                    color: var(--af-text);
                }

                .af-searching-value,
                .af-empty-copy {
                    font-size: 12px;
                    line-height: 1.45;
                    color: var(--af-text-secondary);
                }

                .af-banner {
                    border-radius: 14px;
                    padding: 10px 12px;
                    font-size: 12px;
                    line-height: 1.45;
                }

                .af-banner-soft {
                    border: 1px solid color-mix(in srgb, var(--af-info) 28%, transparent);
                    background: color-mix(in srgb, var(--af-info) 9%, transparent);
                    color: var(--af-text);
                }

                .af-banner-warn {
                    border: 1px solid color-mix(in srgb, var(--af-accent) 34%, transparent);
                    background: color-mix(in srgb, var(--af-accent) 10%, transparent);
                    color: var(--af-text);
                }

                .af-context-box {
                    display: flex;
                    flex-direction: column;
                    gap: 5px;
                    padding: 12px;
                    border-radius: 16px;
                    border: 1px solid var(--af-border);
                    background: color-mix(in srgb, var(--af-text) 4%, transparent);
                }

                .af-context-label {
                    font-size: 10px;
                    font-weight: 700;
                    letter-spacing: 0.10em;
                    text-transform: uppercase;
                    color: var(--af-text-muted);
                }

                .af-context-text {
                    font-size: 12px;
                    line-height: 1.5;
                    color: var(--af-text-secondary);
                    white-space: pre-line;
                    overflow-wrap: anywhere;
                }

                .af-selection-card {
                    display: flex;
                    flex-direction: column;
                    gap: 8px;
                    padding: 14px;
                    border-radius: 18px;
                    border: 1px solid var(--af-border);
                    background:
                        radial-gradient(circle at top left, color-mix(in srgb, var(--af-accent) 12%, transparent), transparent 35%),
                        color-mix(in srgb, var(--af-text) 4%, transparent);
                }

                .af-selection-top {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    justify-content: space-between;
                }

                .af-field-chip {
                    min-width: 0;
                    max-width: 60%;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    border-radius: 999px;
                    padding: 4px 10px;
                    background: color-mix(in srgb, var(--af-text) 7%, transparent);
                    border: 1px solid var(--af-border);
                    font-size: 11px;
                    color: var(--af-text-secondary);
                }

                .af-confidence-badge {
                    border-radius: 999px;
                    padding: 4px 10px;
                    font-size: 11px;
                    font-weight: 700;
                    flex-shrink: 0;
                }

                .af-confidence-badge.high {
                    background: color-mix(in srgb, var(--af-success) 14%, transparent);
                    color: var(--af-success);
                }

                .af-confidence-badge.medium {
                    background: color-mix(in srgb, var(--af-accent) 14%, transparent);
                    color: var(--af-text);
                }

                .af-confidence-badge.low {
                    background: color-mix(in srgb, var(--alarm, #c4521e) 14%, transparent);
                    color: var(--af-danger);
                }

                .af-selection-value {
                    font-size: 24px;
                    line-height: 1.15;
                    letter-spacing: -0.02em;
                    font-weight: 800;
                    color: var(--af-text);
                    word-break: break-word;
                }

                .af-selection-reason {
                    font-size: 12px;
                    line-height: 1.45;
                    color: var(--af-text-secondary);
                }

                .af-selection-source {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 10px;
                    font-size: 11px;
                    color: var(--af-text-muted);
                }

                .af-selection-source > span:first-child {
                    min-width: 0;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                }

                .af-candidate-list {
                    display: flex;
                    flex-direction: column;
                    gap: 8px;
                    max-height: 188px;
                    overflow-y: auto;
                }

                .af-candidate {
                    display: grid;
                    grid-template-columns: 28px 1fr;
                    gap: 10px;
                    align-items: start;
                    width: 100%;
                    min-height: 48px;
                    padding: 10px;
                    border-radius: 16px;
                    border: 1px solid var(--af-border);
                    background: color-mix(in srgb, var(--af-text) 3%, transparent);
                    color: inherit;
                    text-align: left;
                }

                .af-candidate.selected {
                    border-color: color-mix(in srgb, var(--af-info) 42%, transparent);
                    background: color-mix(in srgb, var(--af-info) 10%, transparent);
                }

                .af-candidate-rank {
                    width: 28px;
                    height: 28px;
                    border-radius: 999px;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    background: color-mix(in srgb, var(--af-text) 8%, transparent);
                    color: var(--af-text-secondary);
                    font-size: 11px;
                    font-weight: 700;
                }

                .af-candidate.selected .af-candidate-rank {
                    background: color-mix(in srgb, var(--af-info) 18%, transparent);
                    color: var(--af-text);
                }

                .af-candidate-copy {
                    display: flex;
                    flex-direction: column;
                    gap: 4px;
                    min-width: 0;
                }

                .af-candidate-value {
                    font-size: 13px;
                    font-weight: 700;
                    color: var(--af-text);
                    word-break: break-word;
                }

                .af-candidate-meta {
                    font-size: 11px;
                    color: var(--af-text-muted);
                }

                .af-actions {
                    display: grid;
                    grid-template-columns: 1fr 1fr;
                    gap: 8px;
                }

                .af-primary,
                .af-secondary {
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    gap: 6px;
                    padding: 12px 14px;
                }

                .af-primary {
                    background: color-mix(in srgb, var(--af-accent) 16%, var(--af-raised));
                    border-color: color-mix(in srgb, var(--af-accent) 38%, transparent);
                    color: var(--af-text);
                }

                .af-secondary {
                    background: color-mix(in srgb, var(--af-text) 5%, transparent);
                    border-color: var(--af-border);
                    color: var(--af-text-secondary);
                }

                .af-primary kbd,
                .af-secondary kbd {
                    font-size: 11px;
                    opacity: 0.7;
                }

                .af-footer-hint {
                    font-size: 11px;
                    color: var(--af-text-muted);
                    line-height: 1.4;
                }

                .af-spinner {
                    width: 18px;
                    height: 18px;
                    border-radius: 999px;
                    border: 2px solid var(--af-border);
                    border-top-color: var(--af-accent);
                    animation: af-spin 0.7s linear infinite;
                    flex-shrink: 0;
                }

                .af-inline-icon {
                    width: 22px;
                    height: 22px;
                    border-radius: 999px;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    font-size: 12px;
                    font-weight: 800;
                    flex-shrink: 0;
                }

                .af-inline-icon.done {
                    background: color-mix(in srgb, var(--af-success) 16%, transparent);
                    color: var(--af-success);
                }

                .af-inline-icon.error {
                    background: color-mix(in srgb, var(--alarm, #c4521e) 16%, transparent);
                    color: var(--af-danger);
                }

                .af-inline-copy {
                    display: flex;
                    flex-direction: column;
                    gap: 2px;
                    min-width: 0;
                    flex: 1;
                }

                .af-inline-label {
                    font-size: 11px;
                    color: var(--af-text-muted);
                }

                .af-inline-value {
                    font-size: 13px;
                    color: var(--af-text);
                    line-height: 1.4;
                    word-break: break-word;
                }

                @keyframes af-slide-in {
                    from {
                        opacity: 0;
                        transform: translateY(10px) scale(0.98);
                    }
                    to {
                        opacity: 1;
                        transform: translateY(0) scale(1);
                    }
                }

                @keyframes af-spin {
                    to {
                        transform: rotate(360deg);
                    }
                }

                :root[data-theme="light"] .af-overlay {
                    --af-info: #216589;
                    --af-success: #21683c;
                    --af-danger: #9b3425;
                }

                @media (max-width: 380px) {
                    .af-main-card {
                        padding: 14px 12px;
                    }

                    .af-search-row,
                    .af-actions {
                        grid-template-columns: 1fr;
                    }

                    .af-search-btn {
                        padding-block: 10px;
                    }

                    .af-selection-top,
                    .af-selection-source {
                        align-items: flex-start;
                        flex-direction: column;
                    }

                    .af-field-chip {
                        max-width: 100%;
                    }
                }

                @media (prefers-reduced-motion: reduce) {
                    .af-card {
                        animation: none;
                    }

                    .af-spinner {
                        animation: none;
                    }

                    .af-search-btn,
                    .af-primary,
                    .af-secondary {
                        transition: none;
                    }

                    .af-search-btn:hover:not(:disabled),
                    .af-primary:hover,
                    .af-secondary:hover {
                        transform: none;
                    }
                }
            `}</style>
        </div>
    );
}

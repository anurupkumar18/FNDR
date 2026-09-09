import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import {
    type ScreenGuideCursorPosition,
    type ScreenGuidePhase,
    type ScreenGuideSettings,
    acknowledgeScreenGuideMicrophoneStopped,
    askScreenGuide,
    finishScreenGuideVisual,
    getScreenGuideCursorPosition,
    getScreenGuideSettings,
    onScreenGuideShortcut,
    onScreenGuideSubmit,
    reportScreenGuideState,
    screenGuideMicrophoneStarted,
    setScreenGuideOverlayReady,
    transcribeScreenGuideVoiceInput,
} from "@/shared/ipc/tauri";
import { VOICE_RECORDING } from "@/shared/utils/config";
import {
    initialScreenGuideState,
    isLongEnoughVoiceClip,
    screenGuideReducer,
    screenGuideErrorMessage,
    toScreenGuideHistory,
} from "./screenGuideState";
import "./ScreenGuideOverlay.css";

const MIN_ANSWER_VISIBLE_MS = 9_000;
const MAX_ANSWER_VISIBLE_MS = 20_000;
const READING_MS_PER_WORD = 300;
const ERROR_VISIBLE_MS = 3_500;
const MAX_RECORDING_MS = 60_000;
const VISUAL_FINISH_ATTEMPTS = 3;
const VISUAL_FINISH_RETRY_MS = 150;

function answerVisibleMs(answer: string): number {
    const wordCount = answer.trim().split(/\s+/u).filter(Boolean).length;
    return Math.max(
        MIN_ANSWER_VISIBLE_MS,
        Math.min(MAX_ANSWER_VISIBLE_MS, wordCount * READING_MS_PER_WORD),
    );
}

function recorderOptions(): MediaRecorderOptions | undefined {
    const candidates = [
        "audio/webm;codecs=opus",
        "audio/mp4",
        "audio/ogg;codecs=opus",
        "audio/webm",
    ];
    for (const mimeType of candidates) {
        if (MediaRecorder.isTypeSupported(mimeType)) {
            return { mimeType, audioBitsPerSecond: VOICE_RECORDING.audioBitsPerSecond };
        }
    }
    return undefined;
}

function notchStatus(phase: ScreenGuidePhase): { label: string; copy: string } {
    switch (phase) {
        case "listening":
            return { label: "FNDR is listening", copy: "Listening" };
        case "transcribing":
            return { label: "FNDR is transcribing", copy: "On-device" };
        case "thinking":
            return { label: "FNDR is thinking", copy: "Finding" };
        case "answer":
            return { label: "FNDR has an answer", copy: "Found it" };
        case "error":
            return { label: "FNDR needs attention", copy: "Try again" };
        case "idle":
            return { label: "FNDR is ready", copy: "FNDR" };
    }
}

export function ScreenGuideOverlay() {
    const [state, dispatch] = useReducer(screenGuideReducer, initialScreenGuideState);
    const [cursorOrigin, setCursorOrigin] = useState<ScreenGuideCursorPosition | null>(null);
    const stateRef = useRef(state);
    const settingsRef = useRef<ScreenGuideSettings | null>(null);
    const recorderRef = useRef<MediaRecorder | null>(null);
    const streamRef = useRef<MediaStream | null>(null);
    const pendingMediaAcquisitionRef = useRef<{
        requestGeneration: number;
        stopGeneration: number | null;
    } | null>(null);
    const chunksRef = useRef<Blob[]>([]);
    const mimeTypeRef = useRef("audio/webm");
    const recordingStartedAtRef = useRef(0);
    const pressActiveRef = useRef(false);
    const cursorAtPressRef = useRef<ScreenGuideCursorPosition | null>(null);
    const dismissTimerRef = useRef<number | null>(null);
    const recordingTimerRef = useRef<number | null>(null);
    const requestIdRef = useRef(0);
    const interactionEpochRef = useRef(0);
    const mountedRef = useRef(true);
    stateRef.current = state;

    const clearDismissTimer = useCallback(() => {
        if (dismissTimerRef.current !== null) {
            window.clearTimeout(dismissTimerRef.current);
            dismissTimerRef.current = null;
        }
    }, []);

    const clearRecordingTimer = useCallback(() => {
        if (recordingTimerRef.current !== null) {
            window.clearTimeout(recordingTimerRef.current);
            recordingTimerRef.current = null;
        }
    }, []);

    const stopActiveStream = useCallback(() => {
        const stream = streamRef.current;
        if (!stream) return;
        streamRef.current = null;
        for (const track of stream.getTracks()) track.stop();
    }, []);

    const stopRecordingWithoutTranscription = useCallback(() => {
        pressActiveRef.current = false;
        chunksRef.current = [];
        clearRecordingTimer();
        const recorder = recorderRef.current;
        recorderRef.current = null;
        if (recorder) {
            recorder.ondataavailable = null;
            recorder.onstop = null;
            if (recorder.state !== "inactive") recorder.stop();
        }
        stopActiveStream();
    }, [clearRecordingTimer, stopActiveStream]);

    const acknowledgeStoppedOrDefer = useCallback((generation: number) => {
        const pendingAcquisition = pendingMediaAcquisitionRef.current;
        if (
            pendingAcquisition &&
            pendingAcquisition.requestGeneration <= generation
        ) {
            pendingAcquisition.stopGeneration = generation;
            return;
        }
        void acknowledgeScreenGuideMicrophoneStopped(generation).catch(() => undefined);
    }, []);

    const finishVisual = useCallback(async (requestId: number): Promise<boolean> => {
        for (let attempt = 0; attempt < VISUAL_FINISH_ATTEMPTS; attempt += 1) {
            if (!mountedRef.current || requestId !== requestIdRef.current) return false;
            try {
                if (await finishScreenGuideVisual(requestId)) return true;
            } catch {
                // A transient native window error is retried below.
            }
            if (attempt === VISUAL_FINISH_ATTEMPTS - 1) return false;
            await new Promise<void>((resolve) => {
                window.setTimeout(resolve, VISUAL_FINISH_RETRY_MS);
            });
        }
        return false;
    }, []);

    const hideAfter = useCallback((delayMs: number, requestId: number) => {
        clearDismissTimer();
        dismissTimerRef.current = window.setTimeout(() => {
            dismissTimerRef.current = null;
            if (requestId !== requestIdRef.current) return;
            void finishVisual(requestId).then((finished) => {
                if (!finished || !mountedRef.current || requestId !== requestIdRef.current) return;
                dispatch({ type: "idle" });
                setCursorOrigin(null);
            });
        }, delayMs);
    }, [clearDismissTimer, finishVisual]);

    const fail = useCallback((message: string) => {
        if (!mountedRef.current) return;
        dispatch({ type: "failed", message });
        hideAfter(ERROR_VISIBLE_MS, requestIdRef.current);
    }, [hideAfter]);

    const refreshSettings = useCallback(async () => {
        try {
            const settings = await getScreenGuideSettings();
            settingsRef.current = settings;
            return settings;
        } catch {
            return settingsRef.current;
        }
    }, []);

    const askQuestion = useCallback(async (
        rawQuestion: string,
        requestId: number,
        preferredCursor: ScreenGuideCursorPosition | null = null,
        interactionEpoch = interactionEpochRef.current,
    ) => {
        if (
            requestId !== requestIdRef.current ||
            interactionEpoch !== interactionEpochRef.current
        ) return;
        const question = rawQuestion.trim();
        if (!question) {
            fail("I didn’t catch a question. Try again.");
            return;
        }

        clearDismissTimer();
        dispatch({ type: "thinking", question });

        const settings = await refreshSettings();
        if (
            requestId !== requestIdRef.current ||
            interactionEpoch !== interactionEpochRef.current
        ) return;
        if (settings && !settings.enabled) {
            fail("Screen Guide is turned off.");
            return;
        }

        const cursorPromise = settings?.show_cursor
            ? preferredCursor
                ? Promise.resolve(preferredCursor)
                : getScreenGuideCursorPosition().catch(() => null)
            : Promise.resolve(null);

        try {
            const [response, origin] = await Promise.all([
                askScreenGuide(
                    question,
                    requestId,
                    toScreenGuideHistory(stateRef.current.exchanges),
                ),
                cursorPromise,
            ]);
            if (
                !mountedRef.current ||
                requestId !== requestIdRef.current ||
                interactionEpoch !== interactionEpochRef.current
            ) return;
            setCursorOrigin(origin);
            dispatch({ type: "answered", question, response });
            hideAfter(answerVisibleMs(response.answer), requestId);
        } catch (reason) {
            if (
                requestId === requestIdRef.current &&
                interactionEpoch === interactionEpochRef.current
            ) {
                fail(screenGuideErrorMessage(reason, "Screen Guide could not answer that."));
            }
        }
    }, [clearDismissTimer, fail, hideAfter, refreshSettings]);

    const transcribeRecording = useCallback(async (
        chunks: Blob[],
        mimeType: string,
        preferredCursor: ScreenGuideCursorPosition | null,
        requestId: number,
        interactionEpoch: number,
    ) => {
        if (
            requestId !== requestIdRef.current ||
            interactionEpoch !== interactionEpochRef.current
        ) return;
        if (chunks.length === 0) {
            fail("No voice input was captured.");
            return;
        }
        try {
            const blob = new Blob(chunks, { type: mimeType });
            const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
            const result = await transcribeScreenGuideVoiceInput(bytes, mimeType, requestId);
            if (
                requestId !== requestIdRef.current ||
                interactionEpoch !== interactionEpochRef.current
            ) return;
            await askQuestion(result.text, requestId, preferredCursor, interactionEpoch);
        } catch (reason) {
            if (
                requestId === requestIdRef.current &&
                interactionEpoch === interactionEpochRef.current
            ) {
                fail(screenGuideErrorMessage(reason, "Voice transcription failed."));
            }
        }
    }, [askQuestion, fail]);

    const beginRecording = useCallback(async (requestId: number, interactionEpoch: number) => {
        if (recorderRef.current || pressActiveRef.current) return;
        if (pendingMediaAcquisitionRef.current) {
            acknowledgeStoppedOrDefer(requestId);
            fail("The microphone is still stopping. Try the shortcut again.");
            return;
        }
        pressActiveRef.current = true;
        clearDismissTimer();
        dispatch({ type: "listening" });

        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
            pressActiveRef.current = false;
            acknowledgeStoppedOrDefer(requestId);
            fail("Microphone recording is not available in this build.");
            return;
        }

        const settings = await refreshSettings();
        if (
            requestId !== requestIdRef.current ||
            interactionEpoch !== interactionEpochRef.current
        ) return;
        if (!pressActiveRef.current) {
            fail("Hold the shortcut a little longer, then try again.");
            return;
        }
        if (settings && !settings.enabled) {
            pressActiveRef.current = false;
            acknowledgeStoppedOrDefer(requestId);
            fail("Screen Guide is turned off.");
            return;
        }

        cursorAtPressRef.current = null;
        if (settings?.show_cursor) {
            void getScreenGuideCursorPosition()
                .then((position) => {
                    if (requestId === requestIdRef.current) {
                        if (interactionEpoch === interactionEpochRef.current) {
                            cursorAtPressRef.current = position;
                        }
                    }
                })
                .catch(() => undefined);
        }

        const acquisition = {
            requestGeneration: requestId,
            stopGeneration: null as number | null,
        };
        pendingMediaAcquisitionRef.current = acquisition;
        try {
            // Microphone access is deliberately requested only after the press event.
            const stream = await navigator.mediaDevices.getUserMedia({
                audio: {
                    echoCancellation: true,
                    noiseSuppression: true,
                    autoGainControl: true,
                    channelCount: VOICE_RECORDING.channelCount,
                    sampleRate: VOICE_RECORDING.sampleRate,
                },
            });
            if (pendingMediaAcquisitionRef.current === acquisition) {
                pendingMediaAcquisitionRef.current = null;
            }
            if (
                acquisition.stopGeneration !== null ||
                !mountedRef.current ||
                !pressActiveRef.current ||
                requestId !== requestIdRef.current ||
                interactionEpoch !== interactionEpochRef.current
            ) {
                for (const track of stream.getTracks()) track.stop();
                if (acquisition.stopGeneration !== null) {
                    void acknowledgeScreenGuideMicrophoneStopped(
                        acquisition.stopGeneration,
                    ).catch(() => undefined);
                }
                if (
                    mountedRef.current &&
                    requestId === requestIdRef.current &&
                    interactionEpoch === interactionEpochRef.current
                ) {
                    fail("Hold the shortcut a little longer, then try again.");
                }
                return;
            }

            streamRef.current = stream;
            const options = recorderOptions();
            const recorder = options
                ? new MediaRecorder(stream, options)
                : new MediaRecorder(stream);
            recorderRef.current = recorder;
            chunksRef.current = [];
            mimeTypeRef.current = recorder.mimeType || options?.mimeType || "audio/webm";
            recordingStartedAtRef.current = Date.now();

            recorder.ondataavailable = (event) => {
                if (event.data.size > 0) chunksRef.current.push(event.data);
            };
            recorder.onstop = () => {
                clearRecordingTimer();
                const chunks = [...chunksRef.current];
                chunksRef.current = [];
                const stoppedAt = Date.now();
                if (
                    requestId === requestIdRef.current &&
                    interactionEpoch === interactionEpochRef.current
                ) pressActiveRef.current = false;
                recorderRef.current = null;
                stopActiveStream();
                void acknowledgeScreenGuideMicrophoneStopped(requestId).catch(() => undefined);
                if (!mountedRef.current) return;
                if (
                    requestId !== requestIdRef.current ||
                    interactionEpoch !== interactionEpochRef.current
                ) return;
                if (!isLongEnoughVoiceClip(recordingStartedAtRef.current, stoppedAt)) {
                    fail("Hold the shortcut a little longer, then try again.");
                    return;
                }
                dispatch({ type: "transcribing" });
                void transcribeRecording(
                    chunks,
                    mimeTypeRef.current,
                    cursorAtPressRef.current,
                    requestId,
                    interactionEpoch,
                );
            };

            recorder.start(VOICE_RECORDING.timesliceMs);
            try {
                await screenGuideMicrophoneStarted(requestId);
            } catch {
                if (
                    recorderRef.current === recorder &&
                    pressActiveRef.current &&
                    requestId === requestIdRef.current &&
                    interactionEpoch === interactionEpochRef.current
                ) {
                    stopRecordingWithoutTranscription();
                    acknowledgeStoppedOrDefer(requestId);
                    fail("Microphone recording was cancelled for safety.");
                }
                return;
            }
            if (
                recorderRef.current !== recorder ||
                !pressActiveRef.current ||
                requestId !== requestIdRef.current ||
                interactionEpoch !== interactionEpochRef.current
            ) return;
            recordingTimerRef.current = window.setTimeout(() => {
                recordingTimerRef.current = null;
                pressActiveRef.current = false;
                if (recorder.state !== "inactive") recorder.stop();
            }, MAX_RECORDING_MS);
            dispatch({ type: "listening" });
        } catch (reason) {
            if (pendingMediaAcquisitionRef.current === acquisition) {
                pendingMediaAcquisitionRef.current = null;
            }
            if (acquisition.stopGeneration !== null) {
                void acknowledgeScreenGuideMicrophoneStopped(
                    acquisition.stopGeneration,
                ).catch(() => undefined);
                return;
            }
            if (
                requestId !== requestIdRef.current ||
                interactionEpoch !== interactionEpochRef.current
            ) return;
            pressActiveRef.current = false;
            recorderRef.current = null;
            stopActiveStream();
            acknowledgeStoppedOrDefer(requestId);
            fail(screenGuideErrorMessage(reason, "Microphone access failed."));
        }
    }, [acknowledgeStoppedOrDefer, clearDismissTimer, clearRecordingTimer, fail, refreshSettings, stopActiveStream, stopRecordingWithoutTranscription, transcribeRecording]);

    const endRecording = useCallback((generation: number) => {
        pressActiveRef.current = false;
        clearRecordingTimer();
        const recorder = recorderRef.current;
        if (recorder && recorder.state !== "inactive") {
            recorder.stop();
            return;
        }
        stopActiveStream();
        acknowledgeStoppedOrDefer(generation);
    }, [acknowledgeStoppedOrDefer, clearRecordingTimer, stopActiveStream]);

    const cancelInteraction = useCallback((
        nextRequestId: number | null = null,
        stoppedGeneration: number | null = null,
    ) => {
        interactionEpochRef.current += 1;
        requestIdRef.current = nextRequestId ?? -1;
        cursorAtPressRef.current = null;
        clearDismissTimer();
        stopRecordingWithoutTranscription();
        if (stoppedGeneration !== null) {
            acknowledgeStoppedOrDefer(stoppedGeneration);
        }
        if (mountedRef.current) {
            dispatch({ type: "idle" });
            setCursorOrigin(null);
        }
    }, [acknowledgeStoppedOrDefer, clearDismissTimer, stopRecordingWithoutTranscription]);

    useEffect(() => {
        mountedRef.current = true;
        void refreshSettings();
        return () => {
            mountedRef.current = false;
        };
    }, [refreshSettings]);

    useEffect(() => {
        let active = true;
        const disposers: Array<() => void> = [];
        let readyAttempt: Promise<void> | null = null;

        void (async () => {
            try {
                const disposeShortcut = await onScreenGuideShortcut(({ action, generation }) => {
                    if (!active) return;
                    switch (action) {
                        case "press":
                            if (generation <= requestIdRef.current) return;
                            cancelInteraction(generation);
                            void beginRecording(generation, interactionEpochRef.current);
                            break;
                        case "release":
                            if (generation === requestIdRef.current) endRecording(generation);
                            break;
                        case "cancel":
                            if (generation >= requestIdRef.current) {
                                cancelInteraction(generation, generation);
                            }
                            break;
                    }
                });
                if (!active) {
                    disposeShortcut();
                    return;
                }
                disposers.push(disposeShortcut);

                const disposeSubmit = await onScreenGuideSubmit(({ text, generation }) => {
                    if (active && generation > requestIdRef.current) {
                        cancelInteraction(generation, generation);
                        void askQuestion(text, generation, null, interactionEpochRef.current);
                    }
                });
                if (!active) {
                    disposeSubmit();
                    return;
                }
                disposers.push(disposeSubmit);
                readyAttempt = setScreenGuideOverlayReady(true);
                await readyAttempt;
            } catch {
                if (active) {
                    void setScreenGuideOverlayReady(false).catch(() => undefined);
                }
            }
        })();

        return () => {
            active = false;
            cancelInteraction();
            for (const dispose of disposers) dispose();
            const markNotReady = readyAttempt
                ? readyAttempt.catch(() => undefined).then(() => setScreenGuideOverlayReady(false))
                : setScreenGuideOverlayReady(false);
            void markNotReady.catch(() => undefined);
        };
    }, [askQuestion, beginRecording, cancelInteraction, endRecording]);

    useEffect(() => {
        const message = state.phase === "answer" ? "Answer ready" : state.message;
        void reportScreenGuideState({
            phase: state.phase,
            message,
            generation: requestIdRef.current,
        }).catch(() => undefined);
    }, [state.message, state.phase]);

    const cue = state.pointCue;
    const notch = notchStatus(state.phase);
    const target = cue
        ? {
              x: cue.x * window.innerWidth,
              y: cue.y * window.innerHeight,
              labelOnLeft: cue.x >= 0.7,
              labelAbove: cue.y >= 0.78,
          }
        : null;

    return (
        <div className={`sg-overlay sg-overlay--${state.phase}`} aria-live="polite" aria-atomic="true">
            <div className="sg-notch" aria-label={notch.label} role="status">
                <span className="sg-notch-mark" aria-hidden="true">
                    <i /><i /><i />
                </span>
                <span className="sg-notch-copy">{notch.copy}</span>
            </div>

            {state.phase !== "idle" && (
                <div className="sg-overlay-card" role="status">
                    {state.phase === "listening" && (
                        <span className="sg-overlay-wave" aria-hidden="true">
                            <i /><i /><i /><i />
                        </span>
                    )}
                    {(state.phase === "transcribing" || state.phase === "thinking") && (
                        <span className="sg-overlay-spinner" aria-hidden="true" />
                    )}
                    <div className="sg-overlay-copy">
                        {state.phase === "answer" ? (
                            <>
                                <span className="sg-overlay-eyebrow">FNDR</span>
                                <p>{state.answer}</p>
                            </>
                        ) : (
                            <strong>{state.message}</strong>
                        )}
                    </div>
                </div>
            )}

            {target && (
                <div
                    className={`sg-guidance-cursor${target.labelOnLeft ? " is-right-edge" : ""}${target.labelAbove ? " is-bottom-edge" : ""}`}
                    data-label-placement={`${target.labelOnLeft ? "left" : "right"}-${target.labelAbove ? "above" : "below"}`}
                    aria-hidden="true"
                    style={{
                        left: target.x,
                        top: target.y,
                        "--sg-cursor-from-x": `${(cursorOrigin?.x ?? target.x) - target.x}px`,
                        "--sg-cursor-from-y": `${(cursorOrigin?.y ?? target.y) - target.y}px`,
                    } as React.CSSProperties}
                >
                    <span className="sg-guidance-triangle" />
                    {cue?.label && <span className="sg-guidance-label">{cue.label}</span>}
                </div>
            )}
        </div>
    );
}

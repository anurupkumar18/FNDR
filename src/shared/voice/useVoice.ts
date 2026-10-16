import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";

export type VoiceSurface = "home_search" | "screen_guide" | "notch_ask" | "notch_do";
/** `stop_words`: Notch Do while it works. The helper hears only "stop" or
 *  "cancel" and sends no text (ADR 020 amendment, 2026-10-09). */
export type VoiceMode = "toggle" | "push_to_talk" | "stop_words";
export type VoicePermission = "microphone" | "speech_recognition";
export type VoiceSettingsPane = "microphone" | "speech-recognition";
export type VoiceErrorCode =
    | "permission_denied"
    | "recording_failed"
    | "recognition_failed"
    | "helper_crashed"
    | "cancelled";
export type VoiceUnavailableReason =
    | "private_context"
    | "speech_recognition_unavailable"
    | "language_asset_missing"
    | "platform_unsupported"
    | "policy_not_enabled"
    | "permission_denied"
    | "permission_restricted";

export type VoiceState =
    | { kind: "idle" }
    | { kind: "requesting_permission"; permission: VoicePermission }
    | { kind: "preparing_model" }
    | { kind: "listening"; level: number }
    | { kind: "partial"; text: string }
    | { kind: "final"; text: string }
    | { kind: "stop_word" }
    | { kind: "speech_ignored" }
    | { kind: "error"; code: VoiceErrorCode; message: string }
    | {
          kind: "unavailable";
          reason: VoiceUnavailableReason;
          message: string;
          permission?: VoicePermission;
          settingsPane?: VoiceSettingsPane;
      };

export interface VoiceStateEvent {
    version: 1;
    sessionId: string | null;
    surface: VoiceSurface | null;
    state: VoiceState;
}

interface VoiceSession {
    sessionId: string;
}

export interface UseVoiceOptions {
    surface: VoiceSurface;
    mode: VoiceMode;
    onPartial?: (text: string) => void;
    onFinal?: (text: string) => void;
    /** Stop-only sessions: the spotter heard the stop word. */
    onStopWord?: () => void;
    /** Stop-only sessions: speech that was not the stop word was dropped. */
    onIgnoredSpeech?: () => void;
}

export interface VoiceController {
    state: VoiceState;
    level: number;
    sessionId: string | null;
    surface: VoiceSurface;
    mode: VoiceMode;
    isActive: boolean;
    start: () => Promise<void>;
    stop: () => Promise<void>;
    cancel: () => Promise<void>;
    retry: () => Promise<void>;
}

const idleState: VoiceState = { kind: "idle" };

function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

export function useVoice({ surface, mode, onPartial, onFinal, onStopWord, onIgnoredSpeech }: UseVoiceOptions): VoiceController {
    const [state, setState] = useState<VoiceState>(idleState);
    const [level, setLevel] = useState(0);
    const [sessionId, setSessionId] = useState<string | null>(null);
    const sessionRef = useRef<string | null>(null);
    const startingRef = useRef(false);
    const partialRef = useRef(onPartial);
    const finalRef = useRef(onFinal);
    const stopWordRef = useRef(onStopWord);
    const ignoredRef = useRef(onIgnoredSpeech);
    partialRef.current = onPartial;
    finalRef.current = onFinal;
    stopWordRef.current = onStopWord;
    ignoredRef.current = onIgnoredSpeech;

    useTauriEvent<VoiceStateEvent>("voice://state", (event) => {
        if (event.version !== 1 || event.sessionId !== sessionRef.current) return;
        const kind = event.state.kind;
        if (kind === "stop_word" || kind === "speech_ignored") {
            if (mode !== "stop_words") return;
            if (kind === "stop_word") stopWordRef.current?.();
            else ignoredRef.current?.();
            return;
        }
        // A stop-only session never passes text on, whatever arrives.
        if (mode === "stop_words" && (kind === "partial" || kind === "final")) return;

        setState((current) => {
            if (event.state.kind !== "idle") return event.state;
            if (current.kind === "final" || current.kind === "unavailable") return current;
            if (current.kind === "error" && current.code !== "cancelled") return current;
            return event.state;
        });
        if (event.state.kind === "listening") setLevel(event.state.level);
        if (event.state.kind === "partial") partialRef.current?.(event.state.text);
        if (event.state.kind === "final") finalRef.current?.(event.state.text);
        if (event.state.kind === "idle") {
            sessionRef.current = null;
            setSessionId(null);
            setLevel(0);
        }
    });

    const start = useCallback(async () => {
        if (startingRef.current || sessionRef.current) return;
        startingRef.current = true;
        setState(idleState);
        setLevel(0);
        try {
            const session = await invoke<VoiceSession>("voice_start", { surface, mode });
            sessionRef.current = session.sessionId;
            setSessionId(session.sessionId);
        } catch (error) {
            setState({ kind: "error", code: "recording_failed", message: errorMessage(error) });
        } finally {
            startingRef.current = false;
        }
    }, [mode, surface]);

    const stop = useCallback(async () => {
        const activeSession = sessionRef.current;
        if (!activeSession) return;
        try {
            await invoke("voice_stop", { sessionId: activeSession });
        } catch (error) {
            setState({ kind: "error", code: "recording_failed", message: errorMessage(error) });
        }
    }, []);

    const cancel = useCallback(async () => {
        const activeSession = sessionRef.current;
        if (!activeSession) return;
        try {
            await invoke("voice_cancel", { sessionId: activeSession });
        } catch (error) {
            setState({ kind: "error", code: "recording_failed", message: errorMessage(error) });
        }
    }, []);

    useEffect(
        () => () => {
            const activeSession = sessionRef.current;
            if (activeSession) void invoke("voice_cancel", { sessionId: activeSession });
        },
        [],
    );

    return {
        state,
        level,
        sessionId,
        surface,
        mode,
        isActive: sessionId !== null,
        start,
        stop,
        cancel,
        retry: start,
    };
}

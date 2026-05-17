import { useCallback, useEffect, useRef, useState } from "react";
import { fndrAnswer, type ComposedAnswer } from "../ipc/tauri";

interface State {
    answer: ComposedAnswer | null;
    loading: boolean;
    error: string | null;
}

/**
 * Phase 5 — debounced hook around `fndr_answer`. The IPC call is cancelled
 * (best-effort: the response is discarded) when the query changes faster
 * than the debounce window.
 */
export function useFndrAnswer(query: string, debounceMs = 250, limit?: number) {
    const [state, setState] = useState<State>({ answer: null, loading: false, error: null });
    const seq = useRef(0);

    useEffect(() => {
        if (!query.trim()) {
            setState({ answer: null, loading: false, error: null });
            return;
        }
        const callId = ++seq.current;
        setState((s) => ({ ...s, loading: true }));
        const timer = setTimeout(() => {
            fndrAnswer(query, limit)
                .then((answer) => {
                    if (callId === seq.current) {
                        setState({ answer, loading: false, error: null });
                    }
                })
                .catch((err) => {
                    if (callId === seq.current) {
                        setState({ answer: null, loading: false, error: String(err) });
                    }
                });
        }, debounceMs);
        return () => clearTimeout(timer);
    }, [query, debounceMs, limit]);

    const refresh = useCallback(() => {
        seq.current += 1;
    }, []);

    return { ...state, refresh };
}

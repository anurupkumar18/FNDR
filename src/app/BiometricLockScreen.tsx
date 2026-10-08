import { useCallback, useEffect, useRef, useState } from "react";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import { requestBiometricAuth } from "@/shared/ipc/onboarding";

interface BiometricLockScreenProps {
    onUnlock: () => void;
}

export function BiometricLockScreen({ onUnlock }: BiometricLockScreenProps) {
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const autoPromptedRef = useRef(false);

    const authenticate = useCallback(async () => {
        const startedAtMs = Date.now();
        const startedTrace = recordActivityStep(
            beginActivityTrace({
                id: "biometric-unlock",
                title: "Unlock activity",
                startedAtMs,
            }),
            {
                id: "request",
                label: "Waiting for macOS authentication",
                actor: "macOS authentication",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        );

        setLoading(true);
        setError(null);
        setActivityTrace(startedTrace);
        try {
            const ok = await requestBiometricAuth("Unlock your local FNDR memories");
            const completedAtMs = Date.now();
            const requestCompletedTrace = recordActivityStep(startedTrace, {
                id: "request",
                label: "macOS authentication request returned",
                actor: "macOS authentication",
                status: "completed",
                evidence: "ipc-boundary",
                atMs: completedAtMs,
                durationMs: completedAtMs - startedAtMs,
            });
            if (ok) {
                setActivityTrace(recordActivityStep(requestCompletedTrace, {
                    id: "result",
                    label: "Authentication confirmed",
                    actor: "macOS authentication",
                    status: "completed",
                    evidence: "result-metadata",
                    atMs: completedAtMs,
                    durationMs: completedAtMs - startedAtMs,
                }));
                onUnlock();
            } else {
                setError("Authentication was not completed. FNDR remains locked.");
                setActivityTrace(recordActivityStep(requestCompletedTrace, {
                    id: "result",
                    label: "Authentication was not completed",
                    actor: "macOS authentication",
                    status: "failed",
                    evidence: "result-metadata",
                    atMs: completedAtMs,
                    durationMs: completedAtMs - startedAtMs,
                    detail: "FNDR remains locked.",
                }));
            }
        } catch {
            const failedAtMs = Date.now();
            setError("Authentication is unavailable right now. FNDR remains locked. Try again.");
            setActivityTrace(recordActivityStep(startedTrace, {
                id: "request",
                label: "macOS authentication is unavailable",
                actor: "macOS authentication",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: failedAtMs,
                durationMs: failedAtMs - startedAtMs,
                detail: "FNDR remains locked. Try again.",
            }));
        } finally {
            setLoading(false);
        }
    }, [onUnlock]);

    useEffect(() => {
        if (autoPromptedRef.current) {
            return;
        }
        autoPromptedRef.current = true;
        void authenticate();
    }, [authenticate]);

    return (
        <div className="biometric-lock-overlay">
            <div
                className="biometric-lock-card"
                role="dialog"
                aria-modal="true"
                aria-labelledby="biometric-lock-title"
                aria-busy={loading}
            >
                <div className="biometric-lock-icon" aria-hidden="true">FNDR</div>
                <h1 id="biometric-lock-title" className="biometric-lock-title">FNDR is locked</h1>
                <p className="biometric-lock-subtitle">
                    Authenticate with Touch ID or your system password to access your memories.
                </p>
                {activityTrace && (
                    <ActivityTrace
                        trace={activityTrace}
                        className="biometric-lock-activity"
                        announce={!error}
                    />
                )}
                {error && <div className="biometric-lock-error" role="alert">{error}</div>}
                <button
                    className="biometric-lock-btn"
                    onClick={() => void authenticate()}
                    disabled={loading}
                >
                    {loading ? "Waiting for macOS..." : error ? "Try again" : "Unlock FNDR"}
                </button>
            </div>
        </div>
    );
}

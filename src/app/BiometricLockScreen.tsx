import { useCallback, useEffect, useRef, useState } from "react";
import { requestBiometricAuth } from "@/shared/ipc/onboarding";

interface BiometricLockScreenProps {
    onUnlock: () => void;
}

export function BiometricLockScreen({ onUnlock }: BiometricLockScreenProps) {
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const autoPromptedRef = useRef(false);

    const authenticate = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const ok = await requestBiometricAuth("Unlock your local FNDR memories");
            if (ok) {
                onUnlock();
            } else {
                setError("Authentication was not completed. FNDR remains locked.");
            }
        } catch {
            setError("Authentication is unavailable right now. FNDR remains locked. Try again.");
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

import { useCallback, useEffect, useRef, useState } from "react";
import { useReducedMotion } from "framer-motion";
import { BorderBeam } from "border-beam";
import {
    CODEX_LOGIN_COMPLETED_EVENT,
    codexAccountStatus,
    codexLoginCancel,
    codexLoginStart,
    codexLogout,
    type CodexAccountStatus,
    type CodexLoginCompleted,
    type CodexLoginStarted,
    type CodexUsageWindow,
} from "@/shared/ipc/tauri";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStatus,
} from "@/shared/activity/activityTrace";
import { useActiveCinematicPalette } from "@/shared/hooks/useActiveCinematicPalette";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { openExternalUrl } from "@/shared/utils/openExternalUrl";
import "./CodexAccountCard.css";

const CODEX_INSTALL_COMMAND = "npm install -g @openai/codex";
const CODEX_PLAN_HELP_URL = "https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan";

interface CodexAccountCardProps {
    /** Called whenever the account changes, so the panel can offer its models. */
    onStatusChange: (status: CodexAccountStatus) => void;
}

function formatPlan(planType: string | null): string {
    if (!planType) return "ChatGPT";
    return `ChatGPT ${planType.charAt(0).toUpperCase()}${planType.slice(1)}`;
}

function formatWindow(window: CodexUsageWindow): string {
    const minutes = window.windowMinutes ?? 0;
    if (minutes >= 60 * 24 * 7) return "This week";
    if (minutes >= 60) return `${Math.round(minutes / 60)}-hour window`;
    return `${minutes}-minute window`;
}

function formatReset(resetsAt: number | null): string | null {
    if (!resetsAt) return null;
    const date = new Date(resetsAt * 1000);
    const sameDay = date.toDateString() === new Date().toDateString();
    return sameDay
        ? `Resets ${date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`
        : `Resets ${date.toLocaleDateString([], { weekday: "short", hour: "numeric" })}`;
}

function UsageMeter({ window }: { window: CodexUsageWindow }) {
    const used = Math.min(100, Math.max(0, Math.round(window.usedPercent)));
    const reset = formatReset(window.resetsAt);
    return (
        <div className="codex-usage">
            <div className="codex-usage-row">
                <span>{formatWindow(window)}</span>
                <span className="codex-usage-value">{used}% used</span>
            </div>
            <div
                className="codex-usage-track"
                role="meter"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={used}
                aria-label={`${formatWindow(window)} usage`}
            >
                <div className="codex-usage-fill" style={{ transform: `scaleX(${used / 100})` }} />
            </div>
            {reset && <span className="codex-usage-reset">{reset}</span>}
        </div>
    );
}

/**
 * "Sign in with ChatGPT" for the Hermes Codex provider. Codex's app-server
 * runs the OAuth flow in the user's browser and keeps the tokens; FNDR only
 * shows the result, so the subscription powers Hermes without an API key.
 */
export function CodexAccountCard({ onStatusChange }: CodexAccountCardProps) {
    const [status, setStatus] = useState<CodexAccountStatus | null>(null);
    const [pending, setPending] = useState<CodexLoginStarted | null>(null);
    const [busy, setBusy] = useState(false);
    const [refreshing, setRefreshing] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [accountActivity, setAccountActivity] = useState<ActivityTraceSnapshot | null>(null);
    const { mode } = useActiveCinematicPalette();
    const reducedMotion = useReducedMotion() ?? false;
    const onStatusChangeRef = useRef(onStatusChange);
    const refreshGenerationRef = useRef(0);
    const pendingLoginIdRef = useRef<string | null>(null);
    const mountedRef = useRef(true);
    onStatusChangeRef.current = onStatusChange;

    const applyStatus = useCallback((next: CodexAccountStatus) => {
        setStatus(next);
        onStatusChangeRef.current(next);
    }, []);

    const recordAccountResult = useCallback((input: {
        id: string;
        label: string;
        status: ActivityTraceStatus;
        evidence: "ipc-boundary" | "backend-event" | "result-metadata";
        startedAtMs?: number;
    }) => {
        const atMs = Date.now();
        setAccountActivity((current) => {
            const trace = current ?? beginActivityTrace({
                id: `codex-account-${atMs}`,
                title: "ChatGPT account activity",
                startedAtMs: input.startedAtMs ?? atMs,
            });
            return recordActivityStep(trace, {
                id: input.id,
                label: input.label,
                actor: "Codex app server",
                status: input.status,
                evidence: input.evidence,
                atMs,
                ...(input.startedAtMs === undefined
                    ? {}
                    : { durationMs: Math.max(0, atMs - input.startedAtMs) }),
            });
        });
    }, []);

    const refresh = useCallback(async (preserveTrace = false) => {
        const generation = ++refreshGenerationRef.current;
        const startedAtMs = Date.now();
        setRefreshing(true);
        setError(null);
        if (!preserveTrace) {
            setAccountActivity(recordActivityStep(
                beginActivityTrace({
                    id: `codex-account-${startedAtMs}`,
                    title: "ChatGPT account activity",
                    startedAtMs,
                }),
                {
                    id: "account-status",
                    label: "Checking ChatGPT account status",
                    actor: "Codex app server",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: startedAtMs,
                },
            ));
        } else {
            recordAccountResult({
                id: "account-status",
                label: "Refreshing ChatGPT account status",
                status: "running",
                evidence: "ipc-boundary",
            });
        }
        try {
            const next = await codexAccountStatus();
            if (!mountedRef.current || generation !== refreshGenerationRef.current) return;
            applyStatus(next);
            recordAccountResult({
                id: "account-status",
                label: "ChatGPT account status checked",
                status: next.usableForHermes ? "completed" : "degraded",
                evidence: "result-metadata",
                startedAtMs,
            });
        } catch {
            if (!mountedRef.current || generation !== refreshGenerationRef.current) return;
            setError("FNDR could not check the ChatGPT account status. Try again.");
            recordAccountResult({
                id: "account-status",
                label: "ChatGPT account status check failed",
                status: "failed",
                evidence: "ipc-boundary",
                startedAtMs,
            });
        } finally {
            if (mountedRef.current && generation === refreshGenerationRef.current) {
                setRefreshing(false);
            }
        }
    }, [applyStatus, recordAccountResult]);

    useEffect(() => {
        mountedRef.current = true;
        void refresh();
        return () => {
            mountedRef.current = false;
            refreshGenerationRef.current += 1;
        };
    }, [refresh]);

    useTauriEvent<CodexLoginCompleted>(CODEX_LOGIN_COMPLETED_EVENT, (completed) => {
        if (pendingLoginIdRef.current !== completed.loginId) return;
        pendingLoginIdRef.current = null;
        setPending(null);
        if (!completed.success && completed.error) setError(completed.error);
        recordAccountResult({
            id: "sign-in",
            label: completed.success ? "Browser sign-in completed" : "Browser sign-in failed",
            status: completed.success ? "completed" : "failed",
            evidence: "backend-event",
        });
        void refresh(true);
    });

    const run = async (action: () => Promise<void>) => {
        setBusy(true);
        setError(null);
        try {
            await action();
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setBusy(false);
        }
    };

    const handleSignIn = () => {
        const startedAtMs = Date.now();
        setAccountActivity(recordActivityStep(
            beginActivityTrace({
                id: `codex-sign-in-${startedAtMs}`,
                title: "ChatGPT account activity",
                startedAtMs,
            }),
            {
                id: "sign-in",
                label: "Requesting browser sign-in",
                actor: "Codex app server",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        ));
        return run(async () => {
            try {
                const started = await codexLoginStart();
                pendingLoginIdRef.current = started.loginId;
                setPending(started);
                await openExternalUrl(started.authUrl);
                recordAccountResult({
                    id: "sign-in",
                    label: "Browser sign-in started",
                    status: "waiting",
                    evidence: "result-metadata",
                    startedAtMs,
                });
            } catch (reason) {
                recordAccountResult({
                    id: "sign-in",
                    label: "Browser sign-in could not start",
                    status: "failed",
                    evidence: "ipc-boundary",
                    startedAtMs,
                });
                throw reason;
            }
        });
    };

    const handleCancel = () =>
        run(async () => {
            const startedAtMs = Date.now();
            recordAccountResult({
                id: "sign-in-cancel",
                label: "Cancelling browser sign-in",
                status: "running",
                evidence: "ipc-boundary",
            });
            try {
                if (pending) await codexLoginCancel(pending.loginId);
                pendingLoginIdRef.current = null;
                setPending(null);
                recordAccountResult({
                    id: "sign-in-cancel",
                    label: "Browser sign-in cancelled",
                    status: "completed",
                    evidence: "result-metadata",
                    startedAtMs,
                });
            } catch (reason) {
                recordAccountResult({
                    id: "sign-in-cancel",
                    label: "Browser sign-in cancellation failed",
                    status: "failed",
                    evidence: "ipc-boundary",
                    startedAtMs,
                });
                throw reason;
            }
        });

    const handleSignOut = () =>
        run(async () => {
            const startedAtMs = Date.now();
            recordAccountResult({
                id: "sign-out",
                label: "Requesting ChatGPT sign-out",
                status: "running",
                evidence: "ipc-boundary",
            });
            try {
                applyStatus(await codexLogout());
                recordAccountResult({
                    id: "sign-out",
                    label: "ChatGPT account signed out",
                    status: "completed",
                    evidence: "result-metadata",
                    startedAtMs,
                });
            } catch (reason) {
                recordAccountResult({
                    id: "sign-out",
                    label: "ChatGPT sign-out failed",
                    status: "failed",
                    evidence: "ipc-boundary",
                    startedAtMs,
                });
                throw reason;
            }
        });

    if (!status) {
        return (
            <div className="codex-card" aria-busy="true">
                {accountActivity
                    ? <ActivityTrace trace={accountActivity} />
                    : (
                        <div className="codex-card-row">
                            <ThinkingIndicator state="connecting" size="sm" />
                            <span className="codex-card-muted">Checking for Codex…</span>
                        </div>
                    )}
            </div>
        );
    }

    if (status.cliState !== "ready") {
        const broken = status.cliState === "broken";
        return (
            <div className="codex-card">
                <div className="codex-card-title">
                    {broken ? "Codex won't start" : "Codex isn't installed"}
                </div>
                <p className="codex-card-body">
                    {broken
                        ? "FNDR found Codex on this Mac but it failed to start. Reinstalling it usually fixes a missing binary."
                        : "FNDR signs you in through the official Codex app. Install it, then come back here."}
                </p>
                <div className="codex-terminal">
                    <span className="codex-terminal-prompt">$</span>
                    <span>{CODEX_INSTALL_COMMAND}</span>
                </div>
                {accountActivity ? <ActivityTrace trace={accountActivity} /> : null}
                {broken && status.cliPath && <p className="codex-card-muted">Found at {status.cliPath}</p>}
                <div className="codex-actions">
                    <button type="button" className="codex-btn" onClick={() => void refresh()} disabled={busy || refreshing}>
                        Check again
                    </button>
                </div>
            </div>
        );
    }

    const account = status.account;
    if (status.usableForHermes && account) {
        return (
            <div className="codex-card">
                <div className="codex-card-header">
                    <div>
                        <div className="codex-card-title">{formatPlan(account.planType)}</div>
                        {account.email && <div className="codex-card-muted">{account.email}</div>}
                    </div>
                    <span className="codex-card-badge">Signed in</span>
                </div>
                {(status.primaryWindow || status.secondaryWindow) && (
                    <div className="codex-usage-stack">
                        {status.primaryWindow && <UsageMeter window={status.primaryWindow} />}
                        {status.secondaryWindow && <UsageMeter window={status.secondaryWindow} />}
                    </div>
                )}
                <p className="codex-card-muted">
                    Hermes runs on your subscription and counts against these limits.
                </p>
                {accountActivity ? <ActivityTrace trace={accountActivity} /> : null}
                {error && <p className="codex-card-error" role="alert">{error}</p>}
                <div className="codex-actions">
                    <button type="button" className="codex-btn" onClick={() => void handleSignOut()} disabled={busy || refreshing}>
                        Sign out
                    </button>
                </div>
            </div>
        );
    }

    const signInCard = (
        <div className="codex-card">
            <div className="codex-card-title">Use your ChatGPT plan</div>
            <p className="codex-card-body">
                Sign in with the ChatGPT account you use for Codex, on any plan including Free. Hermes then
                runs on your plan's Codex limits with no API key. Sign-in happens in your browser through
                Codex; FNDR never sees your password or tokens.
            </p>
            {account?.kind === "apiKey" && (
                <p className="codex-card-muted">
                    Codex is currently using an API key. Hermes needs a ChatGPT sign-in instead.
                </p>
            )}
            {pending ? (
                <div className="codex-card-row">
                    <ThinkingIndicator state="connecting" size="sm" />
                    <span>Finish signing in in your browser…</span>
                </div>
            ) : null}
            {accountActivity ? <ActivityTrace trace={accountActivity} /> : null}
            {error && <p className="codex-card-error" role="alert">{error}</p>}
            <div className="codex-actions">
                {pending ? (
                    <>
                        <button type="button" className="codex-btn" onClick={() => void openExternalUrl(pending.authUrl)}>
                            Open sign-in page
                        </button>
                        <button type="button" className="codex-btn" onClick={() => void handleCancel()} disabled={busy || refreshing}>
                            Cancel
                        </button>
                    </>
                ) : (
                    <>
                        <button
                            type="button"
                            className="codex-btn codex-btn-primary"
                            onClick={() => void handleSignIn()}
                            disabled={busy || refreshing}
                        >
                            Sign in with ChatGPT
                        </button>
                        <button type="button" className="codex-link" onClick={() => void openExternalUrl(CODEX_PLAN_HELP_URL)}>
                            Which plans work?
                        </button>
                    </>
                )}
            </div>
        </div>
    );

    return (
        <BorderBeam
            size="md"
            colorVariant="mono"
            theme={mode}
            strength={0.6}
            active={!!pending && !reducedMotion}
            borderRadius={12}
        >
            {signInCard}
        </BorderBeam>
    );
}

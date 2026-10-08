import { useCallback, useRef, useState } from "react";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    getPrivacyProof,
    type ModelRequest,
    type OperatorRunSummary,
    type PrivacyProof as PrivacyProofData,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import { usePolling } from "@/shared/hooks/usePolling";
import "../workspace/PipelineInspectorPanel.css";
import { PanelHeader } from "@/shared/components/PanelHeader";

interface Proof {
    evaluated: number;
    stored: number;
    skipped_by_reason: Record<string, number>;
    egress_requests: number;
    egress_hosts: string[];
    model_requests?: ModelRequest[];
    operator_runs?: OperatorRunSummary[];
}

/** What became of a run's actions, leaving out the kinds that did not happen. */
function runOutcome(run: OperatorRunSummary): string {
    const parts = [
        `${run.done} done`,
        run.asked > 0 ? `${run.asked} asked first` : "",
        run.refused > 0 ? `${run.refused} refused` : "",
        run.failed > 0 ? `${run.failed} failed` : "",
    ];
    return parts.filter(Boolean).join(" · ");
}

const FEATURE_LABELS: Record<string, string> = {
    notch_do_plan: "Notch Do planned a request",
    notch_do_step: "Notch Do ran a step",
    notch_do_screen_text: "Notch Do sent an app's on-screen text",
    hermes_chat: "Hermes chat message",
    screen_guide_answer: "Screen Guide asked ChatGPT",
};

const INCLUDED_LABELS: Record<string, string> = {
    memories: "memories",
    screen_text: "on-screen text",
    screenshot: "a screenshot",
};

function includedText(included: string[] | undefined): string {
    const kinds = (included ?? []).map((kind) => INCLUDED_LABELS[kind] ?? kind);
    return kinds.length > 0 ? ` · with ${kinds.join(" and ")}` : "";
}

function requestTime(atMs: number): string {
    const at = new Date(atMs);
    const time = at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    return at.toDateString() === new Date().toDateString()
        ? time
        : `${at.toLocaleDateString([], { month: "short", day: "numeric" })}, ${time}`;
}

function formatBytes(bytes: number): string {
    return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;
}

const REASON_LABELS: Record<string, string> = {
    self_app: "FNDR was open",
    blocklist: "Blocked app or site",
    sensitive_context: "Sensitive context",
    surface_policy: "Excluded screen type",
    perceptual_dup: "Repeated image",
    semantic_dup: "Repeated content",
    ocr_failed: "Text extraction failed",
    low_signal_text: "Too little usable text",
    noise: "Low-quality text",
    grounding: "Low-confidence analysis",
    stacked_extraction: "Analysis quality checks",
    visual_small: "Image too small",
    visual_novelty: "Repeated visual",
    visual_compose_failed: "Visual analysis failed",
    screen_capture_failed: "Screen capture failed",
    embedder_unavailable: "Search model unavailable",
    app_switched_during_capture: "App changed during capture",
};

const label = (reason: string) => REASON_LABELS[reason] ?? reason.replace(/_/g, " ");

export function PrivacyProof({ proof }: { proof: Proof }) {
    const reasons = Object.entries(proof.skipped_by_reason).filter(([, count]) => count > 0);
    return (
        <section aria-label="Privacy activity for this app session" className="pipeline-panel-card">
            <div className="pipeline-engine-kv">
                <span>Frames evaluated this app session</span>
                <strong>{proof.evaluated}</strong>
                <span>Frames stored this app session</span>
                <strong>{proof.stored}</strong>
            </div>
            <p className="pipeline-egress-summary">
                {`${proof.egress_requests} FNDR network ${proof.egress_requests === 1 ? "request" : "requests"} recorded this app session`}
            </p>
            <p className="pipeline-muted">These counts reset when FNDR closes.</p>

            {reasons.length > 0 && (
                <>
                    <h4>Not stored this app session</h4>
                    <ul className="pipeline-skip-reasons">
                        {reasons.map(([reason, count]) => (
                            <li key={reason}>
                                <span>{label(reason)}</span>
                                <strong>{count}</strong>
                            </li>
                        ))}
                    </ul>
                </>
            )}

            {proof.model_requests && proof.model_requests.length > 0 && (
                <>
                    <h4>Recently sent to cloud models</h4>
                    <ul className="pipeline-skip-reasons" aria-label="Cloud model requests">
                        {proof.model_requests
                            .slice(-12)
                            .reverse()
                            .map((request, index) => (
                                <li key={`${request.atMs}-${index}`}>
                                    <span>
                                        {FEATURE_LABELS[request.feature] ?? request.feature} · {request.host} ·{" "}
                                        {requestTime(request.atMs)}
                                        {includedText(request.included)}
                                    </span>
                                    <strong>{formatBytes(request.bytesSent)}</strong>
                                </li>
                            ))}
                    </ul>
                </>
            )}

            {proof.operator_runs && proof.operator_runs.length > 0 && (
                <>
                    <h4>Recent Notch Do runs</h4>
                    <ul className="pipeline-skip-reasons" aria-label="Notch Do runs">
                        {proof.operator_runs.map((run) => (
                            <li key={run.runId}>
                                <span>{requestTime(Date.parse(run.startedAt))}</span>
                                <strong>{runOutcome(run)}</strong>
                            </li>
                        ))}
                    </ul>
                </>
            )}

            {proof.egress_hosts.length > 0 && (
                <p className="pipeline-egress-hosts">Recorded hosts: {proof.egress_hosts.join(", ")}</p>
            )}

            <p className="pipeline-muted pipeline-privacy-note">
                Recorded FNDR requests can include model downloads and enabled integrations. Child processes,
                provider tools, and commands may make requests this counter does not see, so this is activity
                history—not a complete network audit.
            </p>
        </section>
    );
}

interface PrivacyProofPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

/** Self-fetching panel wrapper around {@link PrivacyProof}, following the same
 *  isVisible/onClose + polling contract as EngineMetricsPanel/EngineMetricsCard. */
export function PrivacyProofPanel({ isVisible, onClose }: PrivacyProofPanelProps) {
    const [proof, setProof] = useState<PrivacyProofData | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const dialogRef = useRef<HTMLDivElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);

    const loadPrivacyProof = useCallback(async (isMounted: () => boolean) => {
        const startedAtMs = Date.now();
        const startedTrace = recordActivityStep(
            beginActivityTrace({
                id: "privacy-activity-refresh",
                title: "Privacy activity refresh",
                startedAtMs,
            }),
            {
                id: "request",
                label: "Requesting privacy activity",
                actor: "Privacy counters",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        );

        if (isMounted()) setActivityTrace(startedTrace);
        try {
            const next = await getPrivacyProof();
            if (isMounted()) {
                const completedAtMs = Date.now();
                const skipped = Object.values(next.skipped_by_reason)
                    .reduce((total, count) => total + count, 0);
                const requestCompletedTrace = recordActivityStep(startedTrace, {
                    id: "request",
                    label: "Privacy activity request returned",
                    actor: "Privacy counters",
                    status: "completed",
                    evidence: "ipc-boundary",
                    atMs: completedAtMs,
                    durationMs: completedAtMs - startedAtMs,
                });
                setProof(next);
                setError(null);
                setActivityTrace(recordActivityStep(requestCompletedTrace, {
                    id: "result",
                    label: "Privacy activity refreshed",
                    actor: "Privacy counters",
                    status: "completed",
                    evidence: "result-metadata",
                    atMs: completedAtMs,
                    durationMs: completedAtMs - startedAtMs,
                    detail: `${next.evaluated} evaluated · ${next.stored} stored · ${skipped} not stored · ${next.egress_requests} recorded ${next.egress_requests === 1 ? "request" : "requests"}`,
                }));
            }
        } catch {
            if (isMounted()) {
                const failedAtMs = Date.now();
                setError("Privacy activity could not be refreshed. FNDR will retry while this view is open.");
                setActivityTrace(recordActivityStep(startedTrace, {
                    id: "request",
                    label: "Privacy activity refresh failed",
                    actor: "Privacy counters",
                    status: "failed",
                    evidence: "ipc-boundary",
                    atMs: failedAtMs,
                    durationMs: failedAtMs - startedAtMs,
                    detail: "FNDR will retry while this view is open.",
                }));
            }
        }
    }, []);

    usePolling(loadPrivacyProof, 5000, isVisible);
    useModalFocus(isVisible, dialogRef, closeButtonRef, onClose);

    if (!isVisible) {
        return null;
    }

    return (
        <div
            ref={dialogRef}
            className="pipeline-panel pipeline-panel--privacy"
            role="dialog"
            aria-modal="true"
            aria-labelledby="privacy-activity-title"
            aria-describedby="privacy-activity-description"
        >
            <PanelHeader
                title="Privacy activity"
                titleId="privacy-activity-title"
                subtitle="Capture outcomes and recorded network requests for the current app session."
                subtitleId="privacy-activity-description"
                closeLabel="Close privacy activity"
                closeRef={closeButtonRef}
                onClose={onClose}
            />
            <div className="pipeline-body">
                {activityTrace && (
                    <ActivityTrace
                        trace={activityTrace}
                        className="pipeline-system-activity"
                        announce={false}
                    />
                )}
                {error && <div className="pipeline-error" role="alert">{error}</div>}
                {proof ? <PrivacyProof proof={proof} /> : null}
            </div>
        </div>
    );
}

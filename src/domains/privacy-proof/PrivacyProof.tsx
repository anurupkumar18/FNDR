import { useCallback, useEffect, useRef, useState } from "react";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    getPrivacyProof,
    deleteMemoriesInRange,
    exportMemoriesArchive,
    importMemoriesArchive,
    getPrivacyExclusions,
    type PrivacyExclusion,
    listPeerRuns,
    type ModelRequest,
    type OperatorRunSummary,
    type PeerRunView,
    type PrivacyProof as PrivacyProofData,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import { usePolling } from "@/shared/hooks/usePolling";
import "../workspace/PipelineInspectorPanel.css";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { EncryptionStatus } from "./EncryptionStatus";

interface Proof {
    evaluated: number;
    stored: number;
    skipped_by_reason: Record<string, number>;
    egress_requests: number;
    egress_hosts: string[];
    model_requests?: ModelRequest[];
    operator_runs?: OperatorRunSummary[];
    peer_runs?: PeerRunView[];
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
    daily_output_send: "Daily output sent",
    notch_do_plan: "Notch Do planned a request",
    notch_do_step: "Notch Do ran a step",
    notch_do_screen_text: "Notch Do sent what an app showed",
    hermes_chat: "Hermes chat message",
    screen_guide_answer: "Screen Guide asked ChatGPT",
};

const INCLUDED_LABELS: Record<string, string> = {
    memory_summaries: "memory summaries",
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

function peerState(run: PeerRunView): string {
    if (run.status === "uncertain") return "delivery uncertain";
    if (run.remote_state === "DIRECT_MESSAGE_UNSUPPORTED") return "reply without displayable text";
    if (run.status === "direct_reply") return "direct reply";
    return run.remote_state?.replace(/^TASK_STATE_/, "").replace(/_/g, " ").toLowerCase()
        ?? "acknowledged";
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

function label(reason: string): string {
    const known = REASON_LABELS[reason];
    if (known) return known;
    const words = reason.replace(/_/g, " ");
    return words.charAt(0).toUpperCase() + words.slice(1);
}

export function PrivacyProof({ proof }: { proof: Proof }) {
    const reasons = Object.entries(proof.skipped_by_reason).filter(([, count]) => count > 0);
    return (
        <section aria-label="Privacy activity details" className="pipeline-panel-card">
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

            {proof.peer_runs && proof.peer_runs.length > 0 && (
                <>
                    <h4>Recent peer sends</h4>
                    <ul className="pipeline-skip-reasons" aria-label="Peer sends">
                        {proof.peer_runs.slice(-12).reverse().map((run) => (
                            <li key={run.local_id}>
                                <span>{run.host} · {requestTime(run.created_at_ms)} · {peerState(run)}</span>
                                <strong>{formatBytes(run.payload_bytes)}</strong>
                            </li>
                        ))}
                    </ul>
                    <p className="pipeline-muted">Peer send records stay on this Mac after FNDR closes. Task text and peer output are not stored here.</p>
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

function RangeEraseControls() {
    const [from, setFrom] = useState("");
    const [until, setUntil] = useState("");
    const [preview, setPreview] = useState<{ startMs: number; endMs: number; matched: number } | null>(null);
    const [deleted, setDeleted] = useState<number | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const startMs = from ? new Date(from).getTime() : NaN;
    const endMs = until ? new Date(until).getTime() : NaN;
    const validRange = Number.isFinite(startMs) && Number.isFinite(endMs) && startMs < endMs;
    const matched = preview?.startMs === startMs && preview.endMs === endMs ? preview.matched : null;

    async function previewRange() {
        if (!validRange) return;
        setBusy(true);
        setError(null);
        setDeleted(null);
        try {
            const result = await deleteMemoriesInRange(startMs, endMs, true);
            setPreview({ startMs, endMs, matched: result.matched });
        } catch {
            setError("FNDR could not preview this range.");
        } finally {
            setBusy(false);
        }
    }

    async function deleteRange() {
        if (!preview || matched === null || matched === 0 || busy) return;
        setBusy(true);
        setError(null);
        try {
            const result = await deleteMemoriesInRange(preview.startMs, preview.endMs, false);
            setDeleted(result.deleted);
            setPreview(null);
        } catch {
            setError("FNDR could not complete the deletion. Check the range and try again.");
        } finally {
            setBusy(false);
        }
    }

    return (
        <section aria-label="Delete memories by time range" className="pipeline-panel-card pipeline-range-controls">
            <h3>Delete memories by time range</h3>
            <p className="pipeline-muted">Preview the count before deleting. The end time is excluded.</p>
            <label>From <input type="datetime-local" value={from} onChange={(event) => setFrom(event.target.value)} /></label>
            <label>Until <input type="datetime-local" value={until} onChange={(event) => setUntil(event.target.value)} /></label>
            <div className="pipeline-range-actions">
                <button type="button" disabled={!validRange || busy} onClick={previewRange}>Preview deletion</button>
                <button type="button" disabled={matched === null || matched === 0 || busy} onClick={deleteRange}>
                    {matched === null ? "Delete matching memories" : `Delete ${matched} ${matched === 1 ? "memory" : "memories"}`}
                </button>
            </div>
            <p aria-live="polite">
                {matched !== null && `${matched} ${matched === 1 ? "memory matches" : "memories match"} this range.`}
                {deleted !== null && `${deleted} ${deleted === 1 ? "memory" : "memories"} deleted.`}
            </p>
            {error && <p role="alert">{error}</p>}
        </section>
    );
}

function ArchiveControls() {
    const [busy, setBusy] = useState(false);
    const [message, setMessage] = useState<string | null>(null);
    const [directory, setDirectory] = useState<string | null>(null);

    async function exportArchive() {
        setBusy(true);
        setMessage(null);
        setDirectory(null);
        try {
            const report = await exportMemoriesArchive();
            setMessage(`${report.exported} ${report.exported === 1 ? "memory" : "memories"} exported as encrypted JSONL and Markdown.`);
            setDirectory(report.jsonl_path.replace(/\/[^/]+$/, ""));
        } catch {
            setMessage("The encrypted export was not saved.");
        } finally {
            setBusy(false);
        }
    }

    async function importArchive() {
        setBusy(true);
        setMessage(null);
        setDirectory(null);
        try {
            const count = await importMemoriesArchive();
            setMessage(`${count} ${count === 1 ? "memory" : "memories"} restored.`);
        } catch {
            setMessage("FNDR could not restore this archive. Choose a JSONL archive encrypted on this Mac.");
        } finally {
            setBusy(false);
        }
    }

    return (
        <section aria-label="Memory archive" className="pipeline-panel-card pipeline-range-controls">
            <h3>Memory archive</h3>
            <p className="pipeline-muted">Save JSONL and Markdown together. Both files are encrypted with this Mac's Keychain key. Restore uses the JSONL file.</p>
            <div className="pipeline-range-actions">
                <button type="button" disabled={busy} onClick={exportArchive}>Export encrypted memories</button>
                <button type="button" disabled={busy} onClick={importArchive}>Import encrypted memories</button>
            </div>
            {message && <p aria-live="polite">{message}</p>}
            {directory && <p className="pipeline-muted">Saved in {directory}</p>}
        </section>
    );
}

function DefaultExclusions() {
    const [categories, setCategories] = useState<PrivacyExclusion[]>([]);
    const [failed, setFailed] = useState(false);
    useEffect(() => {
        let current = true;
        getPrivacyExclusions().then(
            (result) => { if (current) setCategories(result); },
            () => { if (current) setFailed(true); },
        );
        return () => { current = false; };
    }, []);
    if (categories.length === 0) {
        return failed ? <p role="alert">Default exclusions could not be loaded.</p> : null;
    }
    return (
        <section aria-label="Default capture exclusions" className="pipeline-panel-card">
            <h3>Excluded from capture by default</h3>
            <ul className="pipeline-skip-reasons">
                {categories.map((category) => (
                    <li key={category.id}>
                        <strong>{category.label}</strong>
                        <span>{category.examples}</span>
                    </li>
                ))}
            </ul>
            <p className="pipeline-muted">Private windows are recognized by title markers. A browser that shows no marker may not be detected.</p>
        </section>
    );
}

/** Self-fetching panel wrapper around {@link PrivacyProof}, following the same
 *  isVisible/onClose + polling contract as EngineMetricsPanel/EngineMetricsCard. */
export function PrivacyProofPanel({ isVisible, onClose }: PrivacyProofPanelProps) {
    const [proof, setProof] = useState<(PrivacyProofData & { peer_runs?: PeerRunView[] }) | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [peerRunsUnavailable, setPeerRunsUnavailable] = useState(false);
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
            const [proofResult, runsResult] = await Promise.allSettled([getPrivacyProof(), listPeerRuns()]);
            if (proofResult.status === "rejected") throw proofResult.reason;
            const next = proofResult.value;
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
                setProof({ ...next, peer_runs: runsResult.status === "fulfilled" ? runsResult.value ?? [] : [] });
                setPeerRunsUnavailable(runsResult.status === "rejected");
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
                subtitle="This session's capture and network activity, plus saved peer sends."
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
                {peerRunsUnavailable && <p className="pipeline-muted">Peer send history could not be loaded.</p>}
                {proof ? <PrivacyProof proof={proof} /> : null}
                <DefaultExclusions />
                <EncryptionStatus />
                <RangeEraseControls />
                <ArchiveControls />
            </div>
        </div>
    );
}

import { useCallback, useRef, useState } from "react";
import { getPrivacyProof, type PrivacyProof as PrivacyProofData } from "@/shared/ipc/tauri";
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
    const dialogRef = useRef<HTMLDivElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);

    const loadPrivacyProof = useCallback(async (isMounted: () => boolean) => {
        try {
            const next = await getPrivacyProof();
            if (isMounted()) {
                setProof(next);
                setError(null);
            }
        } catch (e) {
            if (isMounted()) {
                setError(e instanceof Error ? e.message : String(e));
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
                {error && <div className="pipeline-error">{error}</div>}
                {proof ? <PrivacyProof proof={proof} /> : !error && <p className="pipeline-muted">Loading privacy activity...</p>}
            </div>
        </div>
    );
}

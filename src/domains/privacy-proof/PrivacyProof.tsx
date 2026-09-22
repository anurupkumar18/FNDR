import { useCallback, useState } from "react";
import { getPrivacyProof, type PrivacyProof as PrivacyProofData } from "@/shared/ipc/tauri";
import { usePolling } from "@/shared/hooks/usePolling";
import "../workspace/PipelineInspectorPanel.css";

interface Proof {
    evaluated: number;
    stored: number;
    skipped_by_reason: Record<string, number>;
    egress_requests: number;
    egress_hosts: string[];
}

const label = (reason: string) => reason.replace(/_/g, " ");

export function PrivacyProof({ proof }: { proof: Proof }) {
    const reasons = Object.entries(proof.skipped_by_reason).filter(([, count]) => count > 0);
    return (
        <section aria-label="Privacy proof" className="pipeline-panel-card">
            <div className="pipeline-engine-kv">
                <span>Frames evaluated</span>
                <strong>{proof.evaluated}</strong>
                <span>Frames stored</span>
                <strong>{proof.stored}</strong>
            </div>
            <p className="pipeline-egress-summary">
                {proof.egress_requests} direct network requests from FNDR
            </p>

            {reasons.length > 0 && (
                <>
                    <h4>Skipped before storage</h4>
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
                <p className="pipeline-egress-hosts">Hosts: {proof.egress_hosts.join(", ")}</p>
            )}

            <p className="pipeline-muted pipeline-privacy-note">
                Counts only FNDR's own outbound calls. It does not include model downloads, the Hermes
                subprocess talking to a configured cloud provider, or network use by allowlisted commands
                like cargo check or npm run typecheck.
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

    if (!isVisible) {
        return null;
    }

    return (
        <div className="pipeline-panel">
            <header className="pipeline-header">
                <div>
                    <h2>Privacy proof</h2>
                    <p>Evidence that sensitive content never entered storage.</p>
                </div>
                <button type="button" className="ui-action-btn pipeline-close-btn" onClick={onClose}>
                    Close
                </button>
            </header>
            <div className="pipeline-body">
                {error && <div className="pipeline-error">{error}</div>}
                {proof ? <PrivacyProof proof={proof} /> : !error && <p className="pipeline-muted">Loading privacy proof...</p>}
            </div>
        </div>
    );
}

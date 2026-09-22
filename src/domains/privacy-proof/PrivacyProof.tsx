import { useState } from "react";
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
        <section aria-label="Privacy proof">
            <h2>Privacy proof</h2>
            <p>{proof.evaluated} frames evaluated, {proof.stored} stored</p>
            <ul>
                {reasons.map(([reason, count]) => (
                    <li key={reason}>{label(reason)}: {count}</li>
                ))}
            </ul>
            <p>{proof.egress_requests} direct network requests from FNDR</p>
            <p className="pipeline-muted">
                Counts only FNDR's own outbound calls. It does not include model downloads, the Hermes
                subprocess talking to a configured cloud provider, or network use by allowlisted commands
                like cargo check or npm run typecheck.
            </p>
            {proof.egress_hosts.length > 0 && <p>Hosts: {proof.egress_hosts.join(", ")}</p>}
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

    usePolling(
        async (isMounted) => {
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
        },
        5000,
        isVisible
    );

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

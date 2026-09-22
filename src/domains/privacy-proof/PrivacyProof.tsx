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
            <p>{proof.egress_requests} network requests since launch</p>
            {proof.egress_hosts.length > 0 && <p>Hosts: {proof.egress_hosts.join(", ")}</p>}
        </section>
    );
}

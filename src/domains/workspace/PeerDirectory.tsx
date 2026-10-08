import { useEffect, useState, type FormEvent } from "react";
import {
    addConfiguredPeer,
    listConfiguredPeers,
    removeConfiguredPeer,
    type ConfiguredPeer,
} from "@/shared/ipc/tauri";
import "./PeerDirectory.css";

interface PeerDirectoryProps {
    onBack: () => void;
}

function errorText(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

export function PeerDirectory({ onBack }: PeerDirectoryProps) {
    const [peers, setPeers] = useState<ConfiguredPeer[] | null>(null);
    const [url, setUrl] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let mounted = true;
        void listConfiguredPeers()
            .then((rows) => { if (mounted) setPeers(rows); })
            .catch((reason) => { if (mounted) setError(errorText(reason)); });
        return () => { mounted = false; };
    }, []);

    const save = async (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        if (busy || !url.trim()) return;
        setBusy(true);
        setError(null);
        try {
            const saved = await addConfiguredPeer(url.trim());
            setPeers((current) => [...(current ?? []).filter((peer) => peer.id !== saved.id), saved]);
            setUrl("");
        } catch (reason) {
            setError(errorText(reason));
        } finally {
            setBusy(false);
        }
    };

    const remove = async (peer: ConfiguredPeer) => {
        if (busy) return;
        setBusy(true);
        setError(null);
        try {
            const removed = await removeConfiguredPeer(peer.id);
            if (!removed) {
                setError("This peer was already removed. Reopen Peers to refresh the list.");
                return;
            }
            setPeers((current) => (current ?? []).filter((row) => row.id !== peer.id));
        } catch (reason) {
            setError(errorText(reason));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="aw-peer-directory">
            <button type="button" className="aw-peer-back" aria-label="Back to chat" onClick={onBack}>← Back to chat</button>
            <h2>Peer agents</h2>
            <p>Add an agent&apos;s HTTPS Agent Card URL. FNDR checks its destination and capabilities before saving it.</p>
            <form onSubmit={(event) => void save(event)} className="aw-peer-form">
                <label htmlFor="aw-peer-url">Agent Card URL</label>
                <div className="aw-peer-form-row">
                    <input
                        id="aw-peer-url"
                        type="url"
                        required
                        value={url}
                        onChange={(event) => setUrl(event.target.value)}
                        placeholder="https://agent.example/.well-known/agent-card.json"
                    />
                    <button type="submit" disabled={busy}>{busy ? "Checking…" : "Verify and save"}</button>
                </div>
            </form>
            {error ? <p className="aw-peer-error" role="alert">{error}</p> : null}
            <p className="aw-peer-note">Adding a peer sends only a request for its public Agent Card. No FNDR memory or task is sent.</p>
            <h3>Saved peers</h3>
            {peers === null ? (error ? null : <p>Loading peers…</p>) : peers.length === 0 ? <p>No peers saved.</p> : (
                <ul className="aw-peer-list">
                    {peers.map((peer) => (
                        <li key={peer.id}>
                            <div>
                                <strong>{peer.name}</strong>
                                <span>{peer.endpoint}</span>
                                {peer.requires_bearer ? <small>Bearer sign-in required before delegation</small> : null}
                            </div>
                            <button type="button" disabled={busy} aria-label={`Remove ${peer.name}`} onClick={() => void remove(peer)}>Remove</button>
                        </li>
                    ))}
                </ul>
            )}
            <p className="aw-peer-note">Task delegation will appear here when source review and task tracking are ready.</p>
        </div>
    );
}

import { useEffect, useState, type FormEvent } from "react";
import {
    addConfiguredPeer,
    listConfiguredPeers,
    previewPeerDelegation,
    removeConfiguredPeer,
    type AttachedMemory,
    type ConfiguredPeer,
    type DelegationPreview,
} from "@/shared/ipc/tauri";
import "./PeerDirectory.css";

interface PeerDirectoryProps {
    onBack: () => void;
    selectedMemories?: AttachedMemory[];
}

function errorText(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

export function PeerDirectory({ onBack, selectedMemories = [] }: PeerDirectoryProps) {
    const [peers, setPeers] = useState<ConfiguredPeer[] | null>(null);
    const [url, setUrl] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [selectedPeerId, setSelectedPeerId] = useState("");
    const [task, setTask] = useState("");
    const [outputGoal, setOutputGoal] = useState("");
    const [preview, setPreview] = useState<DelegationPreview | null>(null);

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
            setSelectedPeerId(saved.id);
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
            if (selectedPeerId === peer.id) setSelectedPeerId("");
            setPreview(null);
        } catch (reason) {
            setError(errorText(reason));
        } finally {
            setBusy(false);
        }
    };

    const makePreview = async (event: FormEvent<HTMLFormElement>) => {
        event.preventDefault();
        if (busy) return;
        const peerId = selectedPeerId || peers?.[0]?.id;
        if (!peerId) return;
        setBusy(true);
        setError(null);
        setPreview(null);
        try {
            setPreview(await previewPeerDelegation(peerId, task, outputGoal, selectedMemories.map((memory) => memory.id)));
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
            {peers && peers.length > 0 ? (
                <section className="aw-peer-draft" aria-label="Peer task draft">
                    <h3>Draft a peer task</h3>
                    <p>Preview the exact task text and current memory summaries. Nothing is sent from this draft.</p>
                    <form onSubmit={(event) => void makePreview(event)}>
                        <label htmlFor="aw-draft-peer">Peer</label>
                        <select id="aw-draft-peer" value={selectedPeerId || peers[0].id} onChange={(event) => { setSelectedPeerId(event.target.value); setPreview(null); }}>
                            {peers.map((peer) => <option key={peer.id} value={peer.id}>{peer.name}</option>)}
                        </select>
                        <label htmlFor="aw-draft-task">Task for peer</label>
                        <textarea id="aw-draft-task" required maxLength={4000} value={task} onChange={(event) => { setTask(event.target.value); setPreview(null); }} />
                        <label htmlFor="aw-draft-goal">Output goal</label>
                        <input id="aw-draft-goal" required maxLength={1000} value={outputGoal} onChange={(event) => { setOutputGoal(event.target.value); setPreview(null); }} />
                        <p className="aw-peer-note">{selectedMemories.length} memories selected in Agent chat. Return to chat to change them.</p>
                        <button type="submit" disabled={busy}>{busy ? "Checking sources…" : "Preview task"}</button>
                    </form>
                    {preview ? <div className="aw-peer-preview" aria-label="Task preview">
                        <p>Destination: {preview.destination}</p>
                        <pre>{preview.message_text}</pre>
                        <p>{preview.attachments.length} current memories included. A send would check them again.</p>
                    </div> : null}
                </section>
            ) : null}
        </div>
    );
}

import { useEffect, useState, type FormEvent } from "react";
import {
    addConfiguredPeer,
    cancelPeerTask,
    listConfiguredPeers,
    listPeerRuns,
    previewPeerDelegation,
    refreshPeerTask,
    removeConfiguredPeer,
    sendPeerDelegation,
    type AttachedMemory,
    type ConfiguredPeer,
    type DelegationPreview,
    type DelegationTaskResult,
    type PeerRun,
    type PeerRunView,
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
    const [run, setRun] = useState<PeerRun | null>(null);
    const [outputText, setOutputText] = useState<string | null>(null);
    const [recentRuns, setRecentRuns] = useState<PeerRunView[]>([]);
    const [sendAttempted, setSendAttempted] = useState(false);
    const [inspected, setInspected] = useState<DelegationTaskResult | null>(null);
    const [cancelNotice, setCancelNotice] = useState<string | null>(null);

    useEffect(() => {
        let mounted = true;
        void listConfiguredPeers()
            .then((rows) => { if (mounted) setPeers(rows); })
            .catch((reason) => { if (mounted) setError(errorText(reason)); });
        void listPeerRuns().then((rows) => { if (mounted) setRecentRuns(rows); }).catch(() => {});
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
            setRun(null);
            setOutputText(null);
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
        setRun(null);
        setOutputText(null);
        setSendAttempted(false);
        try {
            setPreview(await previewPeerDelegation(peerId, task, outputGoal, selectedMemories.map((memory) => memory.id)));
        } catch (reason) {
            setError(errorText(reason));
        } finally {
            setBusy(false);
        }
    };

    const send = async () => {
        if (busy || !preview || sendAttempted) return;
        setBusy(true);
        setError(null);
        setSendAttempted(true);
        try {
            const sent = await sendPeerDelegation(
                preview.peer_id, task, outputGoal, selectedMemories.map((memory) => memory.id),
                preview.destination, preview.message_text,
            );
            setRun(sent.run);
            setOutputText(sent.output_text);
        } catch (reason) {
            setError(`${errorText(reason)}. If delivery began, its status may be uncertain; preview again before another attempt.`);
        } finally {
            void listPeerRuns().then(setRecentRuns).catch(() => {});
            setBusy(false);
        }
    };

    const refresh = async (localId: string) => {
        if (busy) return;
        setBusy(true);
        setError(null);
        try {
            setCancelNotice(null);
            setInspected(await refreshPeerTask(localId));
            setRecentRuns(await listPeerRuns());
        } catch (reason) {
            setError(errorText(reason));
        } finally {
            setBusy(false);
        }
    };

    const cancel = async (localId: string) => {
        if (busy) return;
        setBusy(true);
        setError(null);
        try {
            const result = await cancelPeerTask(localId);
            setInspected(result);
            setCancelNotice(result.run.remote_state === "TASK_STATE_CANCELED"
                ? "Peer confirmed cancellation."
                : "Cancellation requested. The peer has not confirmed cancellation.");
            setRecentRuns(await listPeerRuns());
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
                        <select id="aw-draft-peer" value={selectedPeerId || peers[0].id} onChange={(event) => { setSelectedPeerId(event.target.value); setPreview(null); setRun(null); }}>
                            {peers.map((peer) => <option key={peer.id} value={peer.id}>{peer.name}</option>)}
                        </select>
                        <label htmlFor="aw-draft-task">Task for peer</label>
                        <textarea id="aw-draft-task" required maxLength={4000} value={task} onChange={(event) => { setTask(event.target.value); setPreview(null); setRun(null); }} />
                        <label htmlFor="aw-draft-goal">Output goal</label>
                        <input id="aw-draft-goal" required maxLength={1000} value={outputGoal} onChange={(event) => { setOutputGoal(event.target.value); setPreview(null); setRun(null); }} />
                        <p className="aw-peer-note">{selectedMemories.length} memories selected in Agent chat. Return to chat to change them.</p>
                        <button type="submit" disabled={busy}>{busy ? "Checking sources…" : "Preview task"}</button>
                    </form>
                    {preview ? <div className="aw-peer-preview" aria-label="Task preview">
                        <p>Destination: {preview.destination}</p>
                        <pre>{preview.message_text}</pre>
                        <p>{preview.attachments.length} current memories included. FNDR checks them again at Send.</p>
                        {peers.find((peer) => peer.id === preview.peer_id)?.requires_bearer
                            ? <p>Bearer sign-in is required before this peer can receive a task.</p>
                            : !sendAttempted && !run
                                ? <button type="button" disabled={busy} onClick={() => void send()}>Send task</button>
                                : null}
                        {busy && sendAttempted && !run ? <p role="status">Sending reviewed task…</p> : null}
                        {run ? <p role="status">{run.remote_task_id ? `Peer task ${run.remote_task_id}: ${run.remote_state}` : "Peer replied without creating a tracked task."}</p> : null}
                        {outputText ? <div aria-label="Untrusted peer output"><p>Peer output for review</p><pre>{outputText}</pre></div> : null}
                    </div> : null}
                </section>
            ) : null}
            {recentRuns.length > 0 ? <section aria-label="Recent peer sends">
                <h3>Recent peer sends</h3>
                <ul>{recentRuns.slice(-5).reverse().map((item) => <li key={item.local_id}>
                    {item.host}: {item.remote_task_id ?? item.local_id} ({item.remote_state ?? item.status})
                    {item.remote_task_id ? <button type="button" disabled={busy} onClick={() => void refresh(item.local_id)}>Check status</button> : null}
                    {item.remote_task_id && !["TASK_STATE_COMPLETED", "TASK_STATE_FAILED", "TASK_STATE_CANCELED", "TASK_STATE_REJECTED"].includes(item.remote_state ?? "")
                        ? <button type="button" disabled={busy} onClick={() => void cancel(item.local_id)}>Request cancel</button> : null}
                </li>)}</ul>
                {inspected ? <div aria-label="Peer task result">
                    <p>{inspected.run.remote_task_id}: {inspected.run.remote_state}</p>
                    {cancelNotice ? <p>{cancelNotice}</p> : null}
                    {inspected.output_text ? <><p>Untrusted peer output for review</p><pre>{inspected.output_text}</pre></> : null}
                </div> : null}
            </section> : null}
        </div>
    );
}

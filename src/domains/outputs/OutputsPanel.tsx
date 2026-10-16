import { useEffect, useRef, useState } from "react";
import {
    generateDailyOutput,
    forgetOutputAllowedDestination,
    listOutputAllowedDestinations,
    outputDestinationStatus,
    proposeDailyOutputSend,
    renderDailyOutput,
    resolveDailyOutputSend,
    sendDailyOutputAllowed,
    saveDailyOutput,
    setOutputDestination,
    trustOutputDestination,
    type DailyOutputAllowedDestination,
    type DailyOutputApprovalCard,
    type DailyOutputDestinationStatus,
    type DailyOutputDoc,
    type DailyOutputFormat,
    type DailyOutputKind,
    type DailyOutputTool,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./OutputsPanel.css";

interface OutputsPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenMemoryById: (memoryId: string) => void;
}

function localDay(offset: number): string {
    const date = new Date();
    date.setDate(date.getDate() + offset);
    return [date.getFullYear(), String(date.getMonth() + 1).padStart(2, "0"), String(date.getDate()).padStart(2, "0")].join("-");
}

function startOfDay(value: string): number {
    const [year, month, day] = value.split("-").map(Number);
    return new Date(year, month - 1, day).getTime();
}

function endOfDay(value: string): number {
    const [year, month, day] = value.split("-").map(Number);
    return new Date(year, month - 1, day + 1).getTime() - 1;
}

function errorText(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

const DESTINATIONS = [
    { name: "slack_webhook", label: "Slack webhook URL", type: "url" },
    { name: "linear_api_key", label: "Linear API key", type: "password" },
    { name: "linear_team_id", label: "Linear team ID", type: "text" },
] as const;

const TOOL_LABELS: Record<DailyOutputTool, string> = {
    "outputs.post_slack": "Slack",
    "outputs.create_linear_issue": "Linear",
    "outputs.open_mail_draft": "Mail draft",
};

export function OutputsPanel({ isVisible, onClose, onOpenMemoryById }: OutputsPanelProps) {
    const dialogRef = useRef<HTMLDivElement>(null);
    const closeRef = useRef<HTMLButtonElement>(null);
    const [kind, setKind] = useState<DailyOutputKind>("standup");
    const [fromDate, setFromDate] = useState(() => localDay(-1));
    const [throughDate, setThroughDate] = useState(() => localDay(-1));
    const [format, setFormat] = useState<DailyOutputFormat>("markdown");
    const [polish, setPolish] = useState(false);
    const [doc, setDoc] = useState<DailyOutputDoc | null>(null);
    const [card, setCard] = useState<DailyOutputApprovalCard | null>(null);
    const [status, setStatus] = useState<DailyOutputDestinationStatus | null>(null);
    const [allowedDestinations, setAllowedDestinations] = useState<DailyOutputAllowedDestination[]>([]);
    const [lastTrustId, setLastTrustId] = useState<string | null>(null);
    const [settings, setSettings] = useState(false);
    const [recipient, setRecipient] = useState("");
    const [values, setValues] = useState<Record<string, string>>({});
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState("");
    const [message, setMessage] = useState("");

    useModalFocus(isVisible, dialogRef, closeRef, onClose);

    useEffect(() => {
        if (!isVisible) return;
        void outputDestinationStatus().then(setStatus).catch(() => setStatus(null));
        void listOutputAllowedDestinations().then(setAllowedDestinations).catch(() => setAllowedDestinations([]));
    }, [isVisible]);

    if (!isVisible) return null;

    const changeKind = (next: DailyOutputKind) => {
        setKind(next);
        setFromDate(localDay(next === "standup" ? -1 : -7));
        setThroughDate(localDay(-1));
        setDoc(null);
        setCard(null);
        setLastTrustId(null);
    };

    const run = async (action: () => Promise<void>) => {
        setBusy(true);
        setError("");
        setMessage("");
        try {
            await action();
        } catch (cause) {
            setError(errorText(cause));
        } finally {
            setBusy(false);
        }
    };

    const generate = () => void run(async () => {
        const startMs = startOfDay(fromDate);
        const endMs = endOfDay(throughDate);
        if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs <= startMs) {
            throw new Error("Choose an ordered date range.");
        }
        setCard(null);
        setLastTrustId(null);
        setDoc(await generateDailyOutput(kind, startMs, endMs, polish));
    });

    const copy = () => void run(async () => {
        if (!doc) return;
        await navigator.clipboard.writeText(await renderDailyOutput(doc.id, format));
        setMessage("Copied to clipboard.");
    });

    const save = () => void run(async () => {
        if (!doc) return;
        const path = await saveDailyOutput(doc.id, format);
        if (path) setMessage(`Saved to ${path}`);
    });

    const propose = (tool: DailyOutputTool) => void run(async () => {
        if (!doc) return;
        setLastTrustId(null);
        const to = tool === "outputs.open_mail_draft" ? recipient : undefined;
        const receipt = await sendDailyOutputAllowed(doc.id, tool, to);
        if (receipt) {
            setLastTrustId(receipt.trust_id);
            setMessage(receipt.detail);
        } else {
            setCard(await proposeDailyOutputSend(doc.id, tool, to));
        }
    });

    const resolve = (approved: boolean) => void run(async () => {
        if (!card) return;
        const approvalId = card.approval_id;
        setCard(null);
        const receipt = await resolveDailyOutputSend(approvalId, approved);
        setLastTrustId(receipt?.trust_id ?? null);
        setMessage(receipt?.detail ?? "Nothing was sent.");
    });

    const trust = () => void run(async () => {
        if (!lastTrustId) return;
        await trustOutputDestination(lastTrustId);
        setAllowedDestinations(await listOutputAllowedDestinations());
        setLastTrustId(null);
        setMessage("This exact destination is now allowed for future sends. Remove it in Destinations.");
    });

    const forget = (key: string) => void run(async () => {
        await forgetOutputAllowedDestination(key);
        setAllowedDestinations(await listOutputAllowedDestinations());
        setMessage("Destination removed from the allow list.");
    });

    const saveSetting = (name: typeof DESTINATIONS[number]["name"]) => void run(async () => {
        await setOutputDestination(name, values[name] ?? "");
        setValues((current) => ({ ...current, [name]: "" }));
        setStatus(await outputDestinationStatus());
        setMessage("Destination setting saved in Keychain.");
    });

    return (
        <div className="outputs-page" role="dialog" aria-modal="true" aria-label="Daily outputs" ref={dialogRef}>
            <header className="outputs-header">
                <div><h2>Daily outputs</h2><p>Work notes from visible memories, with a source on every line.</p></div>
                <button ref={closeRef} type="button" onClick={onClose} aria-label="Close daily outputs">Close</button>
            </header>
            <main className="outputs-body">
                <div className="outputs-controls">
                    <label>Output
                        <select value={kind} onChange={(event) => changeKind(event.target.value as DailyOutputKind)}>
                            <option value="standup">Standup</option>
                            <option value="weekly_status">Weekly status</option>
                            <option value="timesheet">Timesheet</option>
                        </select>
                    </label>
                    <label>From <input type="date" value={fromDate} onChange={(event) => setFromDate(event.target.value)} /></label>
                    <label>Through <input type="date" value={throughDate} onChange={(event) => setThroughDate(event.target.value)} /></label>
                    <label className="outputs-check"><input type="checkbox" checked={polish} onChange={(event) => setPolish(event.target.checked)} /> Reword with local model</label>
                    <button type="button" onClick={generate} disabled={busy}>Generate</button>
                </div>

                {error && <p role="alert" className="outputs-error">{error}</p>}
                {message && <p role="status">{message}</p>}
                {lastTrustId && <button type="button" onClick={trust} disabled={busy}>Always allow this destination</button>}

                {doc && <>
                    <div className="outputs-toolbar">
                        <label>Format <select value={format} onChange={(event) => setFormat(event.target.value as DailyOutputFormat)}>
                            <option value="markdown">Markdown</option>
                            <option value="plain_text">Plain text</option>
                            <option value="html">HTML</option>
                        </select></label>
                        <button type="button" onClick={copy} disabled={busy}>Copy</button>
                        <button type="button" onClick={save} disabled={busy}>Save file</button>
                    </div>
                    <article className="outputs-doc">
                        <h3>{doc.title}</h3>
                        {doc.sections.map((section) => <section key={section.heading}>
                            <h4>{section.heading}</h4>
                            {section.lines.length === 0 ? <p>Nothing recorded.</p> : <ul>{section.lines.map((line, index) => <li key={`${section.heading}-${index}`}>
                                {line.text} {line.citations.map((citation) => citation.memory_id ?
                                    <button className="outputs-citation" type="button" key={`${citation.kind}-${citation.id}`} onClick={() => onOpenMemoryById(citation.memory_id!)} aria-label={`Open memory ${citation.memory_id}`} title={`Source ${citation.id}`}>[{citation.id}]</button> :
                                    <span key={`${citation.kind}-${citation.id}`} title={citation.id}>[{citation.id}]</span>)}
                            </li>)}</ul>}
                        </section>)}
                    </article>
                    <div className="outputs-toolbar">
                        <label>Mail recipient (optional) <input type="email" value={recipient} onChange={(event) => setRecipient(event.target.value)} /></label>
                        <button type="button" onClick={() => propose("outputs.open_mail_draft")} disabled={busy}>Open mail draft</button>
                        <button type="button" onClick={() => propose("outputs.post_slack")} disabled={busy}>Post to Slack</button>
                        <button type="button" onClick={() => propose("outputs.create_linear_issue")} disabled={busy}>Create Linear issue</button>
                    </div>
                </>}

                <button type="button" className="outputs-settings-toggle" onClick={() => setSettings(!settings)}>{settings ? "Hide destinations" : "Set up destinations"}</button>
                {settings && <section className="outputs-settings" aria-label="Destinations">
                    <p>Slack and Linear credentials stay in the macOS Keychain. Leave a field empty and save to remove it.</p>
                    {DESTINATIONS.map(({ name, label, type }) => <div key={name} className="outputs-setting">
                        <label>{label} <input type={type} value={values[name] ?? ""} onChange={(event) => setValues((current) => ({ ...current, [name]: event.target.value }))} /></label>
                        <span>{status?.[name] ? "Saved" : "Not set"}</span>
                        <button type="button" onClick={() => saveSetting(name)} disabled={busy}>Save {label}</button>
                    </div>)}
                    <h4>Allowed destinations</h4>
                    {allowedDestinations.length === 0 ? <p>None allowed yet.</p> : allowedDestinations.map((entry) => <div className="outputs-setting" key={entry.key}>
                        <span>{entry.destination} ({TOOL_LABELS[entry.tool]})</span>
                        <button type="button" onClick={() => forget(entry.key)} disabled={busy}>Remove {entry.destination}</button>
                    </div>)}
                </section>}
            </main>
            {card && <div className="outputs-approval-backdrop"><section className="outputs-approval" role="alertdialog" aria-modal="true" aria-label="Approve output send">
                <h3>{card.action}</h3>
                <p>Destination: <strong>{card.destination}</strong></p>
                <p>{card.bytes} bytes{card.trimmed ? ", trimmed to fit" : ""}</p>
                <pre>{card.body}</pre>
                <div className="outputs-toolbar"><button type="button" onClick={() => resolve(false)} disabled={busy}>Cancel send</button><button type="button" onClick={() => resolve(true)} disabled={busy}>Approve send</button></div>
            </section></div>}
        </div>
    );
}

import { useCallback, useEffect, useState } from "react";
import {
    getActionSettings,
    listActionApprovals,
    resolveActionApproval,
    setActionSettings,
    type ActionCard,
    type ActionSettings,
} from "@/shared/ipc/tauri";
import "./ActionApprovalPanel.css";

interface ActionApprovalPanelProps {
    embedded?: boolean;
}

export function ActionApprovalPanel({ embedded = false }: ActionApprovalPanelProps) {
    const [cards, setCards] = useState<ActionCard[]>([]);
    const [settings, setSettings] = useState<ActionSettings | null>(null);
    const [busyId, setBusyId] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        try {
            setCards(await listActionApprovals());
        } catch (cause) {
            setError(String(cause));
        }
    }, []);

    useEffect(() => {
        void refresh();
        const timer = window.setInterval(() => void refresh(), 1_000);
        return () => window.clearInterval(timer);
    }, [refresh]);

    useEffect(() => {
        if (embedded) {
            void getActionSettings().then(setSettings).catch((cause) => setError(String(cause)));
        }
    }, [embedded]);

    const decide = async (id: string, approve: boolean) => {
        setBusyId(id);
        setError(null);
        try {
            await resolveActionApproval(id, approve);
            await refresh();
        } catch (cause) {
            setError(String(cause));
        } finally {
            setBusyId(null);
        }
    };

    const changeSettings = async (next: ActionSettings) => {
        setError(null);
        try {
            setSettings(await setActionSettings(next));
        } catch (cause) {
            setError(String(cause));
        }
    };

    const pending = cards.filter((card) => card.status === "pending");
    if (!embedded && pending.length === 0) return null;

    return (
        <section className={`action-approval-panel ${embedded ? "embedded" : "floating"}`} aria-label="Action approvals">
            {embedded && (
                <>
                    <h2>Actions</h2>
                    <p>Review each action before it runs. The card shows the tool call and its undo information.</p>
                    {settings && (
                        <div className="action-approval-settings">
                            <label>
                                <input
                                    className="fndr-switch"
                                    type="checkbox"
                                    checked={settings.actions_kill_switch}
                                    onChange={(event) => void changeSettings({
                                        ...settings,
                                        actions_kill_switch: event.target.checked,
                                    })}
                                />
                                <span>Turn off actions</span>
                            </label>
                            <label>
                                <input
                                    className="fndr-switch"
                                    type="checkbox"
                                    checked={settings.confirm_unnamed_apps}
                                    onChange={(event) => void changeSettings({
                                        ...settings,
                                        confirm_unnamed_apps: event.target.checked,
                                    })}
                                />
                                <span>Confirm before opening apps I did not name</span>
                            </label>
                        </div>
                    )}
                </>
            )}
            {pending.length === 0 && embedded && <p>No actions are waiting for approval.</p>}
            {pending.map((card) => (
                <div key={card.id} className="action-approval-card" role="alertdialog" aria-label="Approve action" aria-modal="false">
                    <div className="action-approval-heading">
                        <strong>{card.title}</strong>
                        <span>{card.caller}</span>
                    </div>
                    <p>{card.reason}</p>
                    <dl>
                        <dt>Tool</dt><dd>{card.tool}</dd>
                        {card.target && <><dt>Target</dt><dd>{card.target}</dd></>}
                        {card.args.map((arg) => (
                            <div key={arg.name} className="action-approval-arg">
                                <dt>{arg.name}</dt><dd>{arg.value}</dd>
                            </div>
                        ))}
                    </dl>
                    <p className="action-approval-undo" data-undo={card.undo.kind}>
                        {card.undo.kind === "available" ? `Undo: ${card.undo.how}` : `No undo: ${card.undo.reason}`}
                    </p>
                    <div className="action-approval-buttons">
                        <button type="button" className="rcpt-quiet" disabled={busyId === card.id} onClick={() => void decide(card.id, false)}>Deny</button>
                        <button type="button" className="rcpt-primary" disabled={busyId === card.id} onClick={() => void decide(card.id, true)}>Approve action</button>
                    </div>
                </div>
            ))}
            {error && <p role="alert" className="action-approval-error">{error}</p>}
        </section>
    );
}

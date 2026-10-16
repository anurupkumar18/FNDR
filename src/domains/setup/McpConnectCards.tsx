import { useCallback, useEffect, useState } from "react";
import "./McpConnectCards.css";
import {
    mcpConnectApply,
    mcpConnectDisconnect,
    mcpConnectPreview,
    mcpConnectRevoke,
    mcpConnectRotateToken,
    mcpConnectSetKeepRunning,
    mcpConnectSetHideMore,
    mcpConnectStatus,
    mcpConnectTest,
    type McpConnectClient,
    type McpConnectPreview,
    type McpConnectStatus,
    type McpConnectTestReport,
} from "@/shared/ipc/tauri";

function reason(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
}

const STATE_LABEL: Record<McpConnectClient["state"], string> = {
    not_connected: "Not connected",
    connected: "Connected",
    server_off: "Server off",
    needs_update: "Needs update",
    conflict: "Conflict",
    unreadable: "Needs attention",
};

export function McpConnectCards() {
    const [status, setStatus] = useState<McpConnectStatus | null>(null);
    const [busy, setBusy] = useState<string | null>(null);
    const [feedback, setFeedback] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [preview, setPreview] = useState<McpConnectPreview | null>(null);
    const [testReport, setTestReport] = useState<McpConnectTestReport | null>(null);

    const refresh = useCallback(async () => {
        setStatus(await mcpConnectStatus());
    }, []);

    useEffect(() => {
        void refresh().catch((failure) => setError(reason(failure)));
    }, [refresh]);

    const act = async (id: string, work: () => Promise<string>) => {
        setBusy(id);
        setFeedback(null);
        setError(null);
        try {
            setFeedback(await work());
            await refresh();
        } catch (failure) {
            setError(reason(failure));
        } finally {
            setBusy(null);
        }
    };

    const showPreview = (client: McpConnectClient) => {
        void act(`preview:${client.id}`, async () => {
            setPreview(await mcpConnectPreview(client.id));
            return `Setup details for ${client.name}.`;
        });
    };

    return (
        <section aria-labelledby="mcp-connect-title">
            <h4 id="mcp-connect-title">Connect your AI tools</h4>
            <p className="section-hint">Let a connected assistant search FNDR memory. The assistant may send what it reads to its model.</p>
            <ul className="setup-center-list mcp-clients">
                {(status?.clients ?? []).map((client) => (
                    <li className="setup-center-row" aria-label={client.name} key={client.id}>
                        <span>
                            <strong>{client.name}</strong>
                            <small className="mcp-state" data-state={client.state}>{STATE_LABEL[client.state]}</small>
                            <small className="setup-center-detail mcp-path">{client.configPath.replace(/\//g, "/\u200b")}</small>
                            {client.detail ? <small className="setup-center-detail">{client.detail}</small> : null}
                            {client.missingTool ? <small className="setup-center-detail">Needs {client.missingTool} for automatic setup.</small> : null}
                        </span>
                        <span className="setup-center-status">
                            {client.state === "connected" ? (
                                <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => void act(client.id, async () => (await mcpConnectDisconnect(client.id)).message)}>
                                    Disconnect
                                </button>
                            ) : client.state === "conflict" || client.state === "unreadable" ? null : (
                                <button type="button" className="ui-action-btn" disabled={busy !== null || status?.actionsBlocked} onClick={() => void act(client.id, async () => (await mcpConnectApply(client.id)).message)}>
                                    {client.state === "needs_update" ? "Update" : "Connect"}
                                </button>
                            )}
                            <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => showPreview(client)}>Show setup</button>
                        </span>
                    </li>
                ))}
            </ul>
            <label className="mcp-toggle">
                <input className="fndr-switch" type="checkbox" checked={status?.keepRunning ?? true} disabled={busy !== null} onChange={(event) => {
                    const keepRunning = event.currentTarget.checked;
                    void act("keep-running", async () => {
                        await mcpConnectSetKeepRunning(keepRunning);
                        return keepRunning ? "MCP will start when FNDR opens." : "MCP will stay off when FNDR opens.";
                    });
                }} />
                <span>Keep the MCP server running</span>
            </label>
            <label className="mcp-toggle">
                <input className="fndr-switch" type="checkbox" checked={status?.hideMore ?? false} disabled={busy !== null} onChange={(event) => {
                    const hideMore = event.currentTarget.checked;
                    void act("hide-more", async () => {
                        await mcpConnectSetHideMore(hideMore);
                        return hideMore ? "Stricter AI-tool privacy is on." : "Stricter AI-tool privacy is off.";
                    });
                }} />
                <span>Hide more in AI-tool results</span>
            </label>
            {status?.actionsBlocked ? <p>Actions are off. Turn them on in Settings to connect a client.</p> : null}
            <div className="setup-center-status mcp-actions">
                <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => void act("test", async () => {
                    const result = await mcpConnectTest();
                    setTestReport(result);
                    return result.ok ? "Connection test passed." : "Connection test found a problem.";
                })}>Test connection</button>
                <button type="button" className="ui-action-btn" disabled={busy !== null || status?.actionsBlocked} onClick={() => void act("rotate", async () => {
                    await mcpConnectRotateToken();
                    return "MCP token rotated and connected clients updated.";
                })}>Rotate token</button>
                <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => void act("revoke", () => mcpConnectRevoke())}>Revoke all access</button>
            </div>
            {preview ? (
                <div>
                    <p>{preview.title}</p>
                    <pre>{preview.maskedText}</pre>
                    <button type="button" className="ui-action-btn" onClick={() => void navigator.clipboard.writeText(preview.text).then(() => setFeedback("Setup copied.")).catch((failure) => setError(reason(failure)))}>Copy setup</button>
                </div>
            ) : null}
            {testReport ? <ul className="mcp-report">{testReport.steps.map((step) => <li key={step.name} data-ok={step.ok}>{step.name}: {step.ok ? "Passed" : "Failed"}. {step.detail}</li>)}</ul> : null}
            {feedback ? <p role="status">{feedback}</p> : null}
            {error ? <p className="sg-diagnostics-error" role="alert">{error}</p> : null}
        </section>
    );
}

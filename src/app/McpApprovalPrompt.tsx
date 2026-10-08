import { useEffect, useState } from "react";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import {
    MCP_APPROVAL_EVENT,
    resolveMcpApproval,
    type McpApprovalPrompt as McpApprovalPromptData,
} from "@/shared/ipc/tauri";
import "./McpApprovalPrompt.css";

export function McpApprovalPrompt() {
    const [requests, setRequests] = useState<McpApprovalPromptData[]>([]);
    const [resolving, setResolving] = useState<string | null>(null);

    useTauriEvent<McpApprovalPromptData>(MCP_APPROVAL_EVENT, (request) => {
        setRequests((current) => [
            ...current.filter((item) => item.request_id !== request.request_id),
            request,
        ]);
    });

    useEffect(() => {
        if (requests.length === 0) return;
        const timers = requests.map((request) =>
            window.setTimeout(() => {
                setRequests((current) => current.filter((item) => item.request_id !== request.request_id));
            }, Math.max(0, request.expires_at_ms - Date.now()))
        );
        return () => timers.forEach(window.clearTimeout);
    }, [requests]);

    const resolve = async (request: McpApprovalPromptData, approved: boolean) => {
        setResolving(request.request_id);
        try {
            await resolveMcpApproval(request.request_id, approved);
        } finally {
            setRequests((current) => current.filter((item) => item.request_id !== request.request_id));
            setResolving(null);
        }
    };

    if (requests.length === 0) return null;

    return (
        <div className="mcp-approval-stack" aria-label="MCP action approvals">
            {requests.map((request) => {
                const titleId = `mcp-approval-title-${request.request_id}`;
                return (
                    <section
                        key={request.request_id}
                        className="mcp-approval-card"
                        role="alertdialog"
                        aria-modal="false"
                        aria-labelledby={titleId}
                    >
                        <h2 id={titleId}>MCP action approval</h2>
                        <p className="mcp-approval-intro">
                            An MCP client requested this action. Review the exact tool and arguments before it runs.
                        </p>
                        <div className="mcp-approval-tool">
                            <span>Tool</span>
                            <code>{request.tool}</code>
                        </div>
                        <pre className="mcp-approval-arguments">
                            {JSON.stringify(request.arguments, null, 2)}
                        </pre>
                        <div className="mcp-approval-actions">
                            <button
                                type="button"
                                className="mcp-approval-decline"
                                onClick={() => void resolve(request, false)}
                                disabled={resolving === request.request_id}
                            >
                                Decline
                            </button>
                            <button
                                type="button"
                                className="mcp-approval-confirm"
                                onClick={() => void resolve(request, true)}
                                disabled={resolving === request.request_id}
                            >
                                Approve and run
                            </button>
                        </div>
                    </section>
                );
            })}
        </div>
    );
}

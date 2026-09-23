import { useState } from "react";
import { Button } from "@/shared/components/atoms";
import { fndrBuildContextPack, type MemoryCard } from "../../shared/ipc/tauri";

interface Props {
    card: MemoryCard;
}

/**
 * Phase 5 — "Copy for Agent" button. Calls fndrBuildContextPack and writes
 * a markdown-rendered ContextPack to the clipboard. Shows a transient
 * "Copied" toast so the user knows the action succeeded.
 */
export function CopyForAgentButton({ card }: Props) {
    const [status, setStatus] = useState<"idle" | "copying" | "copied" | "error">("idle");

    async function handleClick() {
        setStatus("copying");
        try {
            const pack = await fndrBuildContextPack({
                query: card.title,
                project: card.project,
            });
            const md = renderMemoryContextMarkdown(card, pack);
            await navigator.clipboard.writeText(md);
            setStatus("copied");
            setTimeout(() => setStatus("idle"), 1800);
        } catch (err) {
            console.error("CopyForAgent failed", err);
            setStatus("error");
            setTimeout(() => setStatus("idle"), 2400);
        }
    }

    return (
        <Button
            variant="secondary"
            onClick={handleClick}
            disabled={status === "copying"}
            data-testid="fndr-copy-for-agent"
            aria-live="polite"
            aria-label="Copy this memory and related context"
        >
            {status === "copying"
                ? "Copying…"
                : status === "copied"
                  ? "Copied!"
                  : status === "error"
                    ? "Copy failed"
                    : "Copy memory context"}
        </Button>
    );
}

function renderMemoryContextMarkdown(card: MemoryCard, pack: unknown): string {
    const summary =
        card.insight_what_happened?.trim()
        || card.display_summary?.trim()
        || card.summary.trim();
    const lines = [
        `# FNDR memory: ${card.title}`,
        "",
        `- Memory ID: ${card.id}`,
        `- Captured: ${new Date(card.timestamp).toISOString()}`,
        `- Source: ${card.app_name}${card.window_title ? ` — ${card.window_title}` : ""}`,
    ];
    if (card.project?.trim()) lines.push(`- Project: ${card.project.trim()}`);
    if (summary) lines.push("", "## What happened", summary);
    if (card.insight_why_mattered?.trim()) {
        lines.push("", "## Why it mattered", card.insight_why_mattered.trim());
    }
    if (card.insight_what_changed?.trim()) {
        lines.push("", "## What changed", card.insight_what_changed.trim());
    }

    if (typeof pack !== "object" || pack === null) return String(pack);
    const p = pack as Record<string, unknown>;
    if (typeof p.summary === "string" && p.summary.trim().length > 0) {
        lines.push("", "## Related FNDR context", p.summary);
    }
    if (Array.isArray(p.relevant_files) && p.relevant_files.length > 0) {
        lines.push("", "## Relevant files");
        for (const f of p.relevant_files) {
            const path = (f as { path?: string }).path;
            if (path) lines.push(`- ${path}`);
        }
    }
    if (Array.isArray(p.recent_decisions) && p.recent_decisions.length > 0) {
        lines.push("", "## Recent decisions");
        for (const d of p.recent_decisions) {
            const summary = (d as { summary?: string }).summary;
            if (summary) lines.push(`- ${summary}`);
        }
    }
    return lines.join("\n");
}

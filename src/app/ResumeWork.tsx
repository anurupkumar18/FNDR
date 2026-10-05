import { useEffect, useState } from "react";
import { resumeWork, type ResumeThread } from "@/shared/ipc/tauri";
import "./ResumeWork.css";

interface ResumeWorkProps {
    onOpenMemory: (memoryId: string) => void;
    onOpenVault: () => void;
}

function activityAge(minutes: number): string {
    if (minutes < 1) return "Just now";
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.floor(minutes / 60);
    return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
}

/** Mounted only on unlocked Home; reentry refreshes from current stored rows. */
export function ResumeWork({ onOpenMemory, onOpenVault }: ResumeWorkProps) {
    const [threads, setThreads] = useState<ResumeThread[]>([]);
    const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
    const [refresh, setRefresh] = useState(0);

    useEffect(() => {
        let cancelled = false;
        setThreads([]);
        setStatus("loading");
        void resumeWork().then((result) => {
            if (!cancelled) {
                setThreads(result.slice(0, 3));
                setStatus("ready");
            }
        }).catch(() => {
            if (!cancelled) setStatus("error");
        });
        return () => { cancelled = true; };
    }, [refresh]);

    return (
        <section className="resume-work" aria-labelledby="resume-work-title" aria-busy={status === "loading"}>
            <div className="resume-work-header">
                <div>
                    <h2 id="resume-work-title">Pick up where you left off</h2>
                    <p>Recent work · last 24 hours</p>
                </div>
                <div className="resume-work-actions">
                    <button type="button" onClick={onOpenVault}>Open Vault</button>
                    <button type="button" disabled={status === "loading"} onClick={() => setRefresh((value) => value + 1)}>
                        {status === "error" ? "Retry" : "Refresh"}
                    </button>
                </div>
            </div>
            {status === "loading" && <p role="status">Loading recent work…</p>}
            {status === "error" && <p role="alert">Recent work couldn’t be loaded. Try again, or search above.</p>}
            {status === "ready" && threads.length === 0 && (
                <p className="resume-work-empty">No recent work yet. Saved work will appear here; search above or open the Vault to find older memories.</p>
            )}
            {status === "ready" && threads.length > 0 && (
                <ul className="resume-work-list">
                    {threads.map((thread) => {
                        const latestSource = thread.evidence[thread.evidence.length - 1];
                        const suggestion = thread.suggested_next_steps.find((step) => thread.evidence.includes(step.source_memory_id));
                        return (
                            <li key={thread.title}>
                                <div className="resume-work-thread-heading">
                                    <h3>{thread.title || "Recent work"}</h3>
                                    <span>{activityAge(thread.age_minutes)}</span>
                                </div>
                                {thread.last_state && <p>{thread.last_state.replace(/\bin_progress\b/g, "in progress")}</p>}
                                {latestSource && (
                                    <button type="button" aria-label={`View latest source for ${thread.title}`} onClick={() => onOpenMemory(latestSource)}>View latest source</button>
                                )}
                                {suggestion && (
                                    <div className="resume-work-next">
                                        <span>Suggested next step</span>
                                        <p>{suggestion.title}</p>
                                        <button type="button" aria-label={`View source for suggested step: ${suggestion.title}`} onClick={() => onOpenMemory(suggestion.source_memory_id)}>View source</button>
                                    </div>
                                )}
                            </li>
                        );
                    })}
                </ul>
            )}
        </section>
    );
}

import { useEffect, useId, useState } from "react";
import { resolveWorkSet, resumeWork, type ResumeThread } from "@/shared/ipc/tauri";
import { pickSet } from "./homeWorkSet";
import { HomeDeadlines } from "./HomeDeadlines";
import { HomeSets } from "./HomeSets";
import { RoutineOfferCard } from "./RoutineOfferCard";
import { SaveAsSet } from "./SaveAsSet";
import { ThreadChanges } from "./ThreadChanges";
import { WorkSetOpener, type WorkSetLoad } from "./WorkSetOpener";
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

/** A thread's work set through the one engine (`workset::resolve`), kept only when it holds the thread's own memories. */
function threadSet(thread: ResumeThread): WorkSetLoad {
    return async () => {
        const resolution = await resolveWorkSet(thread.title || thread.app_name || "recent work");
        const set = pickSet(resolution, thread.evidence);
        if (set) return set.items;
        return { why: resolution.kind === "none" ? resolution.value.why : "FNDR found no places to reopen for this thread." };
    };
}

/**
 * Home's work area, mounted only on unlocked Home: a routine offer when one
 * is due, deadlines, recent threads, and saved sets. Reentry refreshes from
 * current stored rows; nothing opens until the person taps.
 */
export function ResumeWork({ onOpenMemory, onOpenVault }: ResumeWorkProps) {
    const [threads, setThreads] = useState<ResumeThread[]>([]);
    const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
    const [refresh, setRefresh] = useState(0);
    const [setsVersion, setSetsVersion] = useState(0);
    const [arrange, setArrange] = useState(false);
    const arrangeId = useId();

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
        <div className="home-work">
            <div className="home-work-options">
                <input id={arrangeId} type="checkbox" checked={arrange} onChange={(event) => setArrange(event.target.checked)} />
                <label htmlFor={arrangeId}>Arrange side by side when a set opens</label>
            </div>
            <RoutineOfferCard arrange={arrange} />
            <HomeDeadlines arrange={arrange} />
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
                            const title = thread.title || "Recent work";
                            const load = threadSet(thread);
                            return (
                                <li key={`${thread.title}:${thread.evidence[0] ?? ""}`}>
                                    <div className="resume-work-thread-heading">
                                        <h3>{title}</h3>
                                        <span>{thread.app_name && thread.app_name !== thread.title ? `${thread.app_name} · ` : ""}{activityAge(thread.age_minutes)}</span>
                                    </div>
                                    {thread.last_state && <p>{thread.last_state.replace(/\bin_progress\b/g, "in progress")}</p>}
                                    {thread.key && <ThreadChanges threadKey={thread.key} title={title} />}
                                    <WorkSetOpener name={title} load={load} continueMemoryId={latestSource} arrange={arrange}>
                                        {latestSource && (
                                            <button type="button" aria-label={`View latest source for ${thread.title}`} onClick={() => onOpenMemory(latestSource)}>View latest source</button>
                                        )}
                                        <SaveAsSet name={title} load={load} onSaved={() => setSetsVersion((value) => value + 1)} />
                                    </WorkSetOpener>
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
            <HomeSets refreshKey={setsVersion} arrange={arrange} />
        </div>
    );
}

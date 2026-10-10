import { useEffect, useState } from "react";
import { getTodos, resolveWorkSet, type Task, type WorkItem } from "@/shared/ipc/tauri";
import { deadlineHeadline, pickSet, taskMemoryIds, upcomingDeadlines } from "./homeWorkSet";
import { WorkSetOpener } from "./WorkSetOpener";

/** At most this many deadline cards; the rest are in To-dos. */
const MAX_CARDS = 3;

interface Deadline {
    task: Task;
    items: WorkItem[];
}

/** The set that belongs to a task: resolved by its title, kept only when it holds the task's own memories. */
async function deadlineFor(task: Task): Promise<Deadline> {
    const ids = taskMemoryIds(task);
    if (ids.length === 0) return { task, items: [] };
    try {
        return { task, items: pickSet(await resolveWorkSet(task.title), ids)?.items ?? [] };
    } catch {
        return { task, items: [] };
    }
}

interface HomeDeadlinesProps {
    nowMs?: number;
    arrange?: boolean;
}

/** Tasks due within 72 hours, each with the places it needs ready to reopen. Nothing opens on load. */
export function HomeDeadlines({ nowMs, arrange = false }: HomeDeadlinesProps) {
    const [deadlines, setDeadlines] = useState<Deadline[]>([]);
    const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
    const [loadedAt, setLoadedAt] = useState(() => nowMs ?? Date.now());

    useEffect(() => {
        let cancelled = false;
        const at = nowMs ?? Date.now();
        void getTodos()
            .then((tasks) => Promise.all(upcomingDeadlines(tasks, at).slice(0, MAX_CARDS).map(deadlineFor)))
            .then((rows) => {
                if (cancelled) return;
                setLoadedAt(at);
                setDeadlines(rows);
                setStatus("ready");
            })
            .catch(() => { if (!cancelled) setStatus("error"); });
        return () => { cancelled = true; };
    }, [nowMs]);

    return (
        <section className="home-deadlines" aria-labelledby="home-deadlines-title" aria-busy={status === "loading"}>
            <h2 id="home-deadlines-title">Due soon</h2>
            {status === "loading" && <p className="home-quiet" role="status">Checking what is due…</p>}
            {status === "error" && <p className="home-quiet" role="alert">Deadlines could not be loaded. They are still in To-dos.</p>}
            {status === "ready" && deadlines.length === 0 && <p className="home-quiet">Nothing is due in the next three days.</p>}
            {status === "ready" && deadlines.length > 0 && (
                <ul className="home-deadline-list">
                    {deadlines.map(({ task, items }) => (
                        <li key={task.id} className="home-deadline">
                            <h3>{deadlineHeadline(task, items.length, loadedAt)}</h3>
                            {items.length > 0 ? (
                                <WorkSetOpener
                                    name={task.title}
                                    load={async () => items}
                                    taskId={task.id}
                                    continueMemoryId={task.source_memory_id ?? undefined}
                                    arrange={arrange}
                                />
                            ) : (
                                <p className="home-quiet">Nothing saved to reopen for it yet.</p>
                            )}
                        </li>
                    ))}
                </ul>
            )}
        </section>
    );
}

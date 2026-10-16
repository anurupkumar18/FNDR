import { useEffect, useId, useRef, useState } from "react";
import { markThreadSeen, whatChangedSince, type ThreadDigest } from "@/shared/ipc/tauri";
import { changeLine } from "./homeWorkSet";

const NAMES_PER_KIND = 5;

interface ThreadChangesProps {
    threadKey: string;
    title: string;
    nowMs?: number;
}

function NameList({ heading, names, total }: { heading: string; names: string[]; total: number }) {
    if (total === 0) return null;
    const shown = names.slice(0, NAMES_PER_KIND);
    const more = total - shown.length;
    return (
        <div className="thread-changes-kind">
            <h4>{heading}</h4>
            {shown.length > 0 && (
                <ul>
                    {shown.map((name, index) => <li key={`${index}:${name}`}>{name}</li>)}
                </ul>
            )}
            {more > 0 && <p>{shown.length > 0 ? `and ${more} more` : `${total} in all`}</p>}
        </div>
    );
}

/**
 * "3 new pages, 1 task since yesterday" under a Resume thread. Reading it
 * marks nothing; opening the list or dismissing it marks the thread seen.
 */
export function ThreadChanges({ threadKey, title, nowMs = Date.now() }: ThreadChangesProps) {
    const [digest, setDigest] = useState<ThreadDigest | null>(null);
    const [expanded, setExpanded] = useState(false);
    const [dismissed, setDismissed] = useState(false);
    const seenRef = useRef(false);
    const listId = useId();

    useEffect(() => {
        let cancelled = false;
        void whatChangedSince(threadKey)
            .then((row) => { if (!cancelled) setDigest(row); })
            .catch(() => { if (!cancelled) setDigest(null); });
        return () => { cancelled = true; };
    }, [threadKey]);

    function markSeen() {
        if (seenRef.current) return;
        seenRef.current = true;
        void markThreadSeen(threadKey).catch(() => { seenRef.current = false; });
    }

    const line = digest ? changeLine(digest, nowMs) : null;
    if (!digest || !line || dismissed) return null;

    return (
        <div className="thread-changes">
            <div className="thread-changes-row">
                <button
                    type="button"
                    className="thread-changes-toggle"
                    aria-expanded={expanded}
                    aria-controls={listId}
                    onClick={() => {
                        setExpanded((open) => !open);
                        markSeen();
                    }}
                >
                    {line}
                </button>
                <button
                    type="button"
                    className="thread-changes-dismiss"
                    aria-label={`Dismiss what changed in ${title}`}
                    onClick={() => {
                        setDismissed(true);
                        markSeen();
                    }}
                >
                    Dismiss
                </button>
            </div>
            <div id={listId} hidden={!expanded} className="thread-changes-list">
                {expanded && (
                    <>
                        <NameList heading="Pages" names={digest.pages} total={digest.page_count} />
                        <NameList heading="Files" names={digest.files} total={digest.file_count} />
                        <NameList heading="Tasks" names={digest.tasks} total={digest.task_count} />
                        <NameList heading="Commits" names={[]} total={digest.commit_count} />
                    </>
                )}
            </div>
        </div>
    );
}

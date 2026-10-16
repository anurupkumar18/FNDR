import { useEffect, useState } from "react";
import { deleteNamedSet, listNamedSets, type NamedWorkSet } from "@/shared/ipc/tauri";
import { WorkSetOpener } from "./WorkSetOpener";

interface HomeSetsProps {
    /** Bumped after a set is saved elsewhere on Home, to read the list again. */
    refreshKey: number;
    arrange?: boolean;
}

/** "Your sets": named work sets, each opened by a tap and deleted after a second tap. */
export function HomeSets({ refreshKey, arrange = false }: HomeSetsProps) {
    const [sets, setSets] = useState<NamedWorkSet[]>([]);
    const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
    const [confirming, setConfirming] = useState<string | null>(null);
    const [deleteFailed, setDeleteFailed] = useState(false);

    useEffect(() => {
        let cancelled = false;
        void listNamedSets()
            .then((rows) => {
                if (cancelled) return;
                setSets(rows);
                setStatus("ready");
            })
            .catch(() => { if (!cancelled) setStatus("error"); });
        return () => { cancelled = true; };
    }, [refreshKey]);

    async function remove(set: NamedWorkSet) {
        setDeleteFailed(false);
        try {
            await deleteNamedSet(set.id);
            setSets((rows) => rows.filter((row) => row.id !== set.id));
        } catch {
            setDeleteFailed(true);
        }
        setConfirming(null);
    }

    return (
        <section className="home-sets" aria-labelledby="home-sets-title" aria-busy={status === "loading"}>
            <h2 id="home-sets-title">Your sets</h2>
            {status === "loading" && <p className="home-quiet" role="status">Loading saved sets…</p>}
            {status === "error" && <p className="home-quiet" role="alert">Saved sets could not be loaded.</p>}
            {status === "ready" && sets.length === 0 && (
                <p className="home-quiet">No saved sets yet. Use Save as set on a thread to keep its places together.</p>
            )}
            {deleteFailed && <p className="home-quiet" role="alert">The set could not be deleted. Try again.</p>}
            {status === "ready" && sets.length > 0 && (
                <ul className="home-set-list">
                    {sets.map((set) => (
                        <li key={set.id} className="home-set">
                            <h3>{set.name}</h3>
                            <p className="home-quiet">{set.items.length === 1 ? "1 place" : `${set.items.length} places`}</p>
                            <WorkSetOpener
                                name={set.name}
                                label="Open"
                                arrange={arrange}
                                load={async () => set.items.length > 0 ? set.items : { why: "Nothing in this set can be reopened now." }}
                            >
                                {confirming === set.id ? (
                                    <>
                                        <button type="button" aria-label={`Yes, delete ${set.name}`} onClick={() => void remove(set)}>Delete</button>
                                        <button type="button" onClick={() => setConfirming(null)}>Keep</button>
                                    </>
                                ) : (
                                    <button type="button" aria-label={`Delete ${set.name}`} onClick={() => setConfirming(set.id)}>Delete</button>
                                )}
                            </WorkSetOpener>
                        </li>
                    ))}
                </ul>
            )}
        </section>
    );
}

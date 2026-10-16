import { useCallback, useEffect, useRef, useState } from "react";
import {
    importCancel, importFirstAnswerMs, importForget, importGetConsent,
    importListSources, importRecordFirstAnswer, importSetConsent, importStart,
    importStatus, listMemoryCards, reopenMemory, searchMemoryCards,
    type ImportKind, type ImportSourceInfo, type ImportStatus, type MemoryCard,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./ImportSources.css";

const SOURCES: Array<{ kind: ImportKind; title: string; reads: string; permission: string }> = [
    { kind: "browser", title: "Browser history", reads: "Page titles, addresses and visit times from local browser history.", permission: "Safari needs Full Disk Access; Chromium browsers need access to their local history files." },
    { kind: "file", title: "Recent files", reads: "Names and text from files changed recently.", permission: "Needs permission to read the selected folders." },
    { kind: "calendar", title: "Calendar", reads: "Past event titles, times, attendees and notes.", permission: "Needs macOS Calendar access." },
    { kind: "mail", title: "Mail", reads: "Messages in selected local mailboxes or dropped mail files.", permission: "Apple Mail may need Full Disk Access; dropped files need file access." },
    { kind: "slack", title: "Slack export", reads: "Messages from a Slack export ZIP you choose.", permission: "Needs access to the chosen ZIP file. No Slack account connection." },
    { kind: "git", title: "GitHub and git", reads: "Commit titles and changed files in chosen local repositories.", permission: "Needs access to those folders. Optional GitHub CLI access is a separate choice." },
];
const RECOMMENDED: ImportKind[] = ["browser", "file", "calendar"];
const FIRST_STARTED_KEY = "fndr.import.first_started_at_ms";
const windowKey = (kind: ImportKind) => `fndr.import.window_days.${kind}`;
const readWindow = (kind: ImportKind) => {
    const stored = Number(localStorage.getItem(windowKey(kind)));
    return Number.isInteger(stored) && stored >= 1 && stored <= 90 ? stored : 30;
};

export function ImportSources({ mode, onContinue }: { mode: "onboarding" | "settings"; onContinue?: () => void }) {
    const [sources, setSources] = useState<ImportSourceInfo[]>([]);
    const [consent, setConsent] = useState<Partial<Record<ImportKind, boolean>>>({});
    const [statuses, setStatuses] = useState<Partial<Record<ImportKind, ImportStatus>>>({});
    const [windows, setWindows] = useState<Partial<Record<ImportKind, number>>>(() =>
        Object.fromEntries(SOURCES.map(({ kind }) => [kind, readWindow(kind)])));
    const [loading, setLoading] = useState(true);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [forgetKind, setForgetKind] = useState<ImportKind | null>(null);
    const [suggestions, setSuggestions] = useState<string[]>([]);
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<MemoryCard[]>([]);
    const [searched, setSearched] = useState(false);
    const [answerMs, setAnswerMs] = useState<number | null>(null);
    const startedAt = useRef<number>(Number(localStorage.getItem(FIRST_STARTED_KEY)) || Date.now());
    const firstAnswerRecorded = useRef(false);
    const forgetDialog = useRef<HTMLDivElement>(null);
    const keepButton = useRef<HTMLButtonElement>(null);
    useModalFocus(forgetKind !== null, forgetDialog, keepButton, () => setForgetKind(null));

    const load = useCallback(async () => {
        setLoading(true);
        try {
            const listed = await importListSources();
            const permissions = await Promise.all(SOURCES.map(async ({ kind }) => [kind, await importGetConsent(kind)] as const));
            setSources(listed);
            setConsent(Object.fromEntries(permissions));
            setStatuses(Object.fromEntries(listed.map((source) => [source.kind, source.status])));
            setAnswerMs(await importFirstAnswerMs());
            setError(null);
        } catch {
            setError("Could not load import sources. Try again.");
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => { void load(); }, [load]);
    useEffect(() => {
        if (!Object.values(statuses).some((status) => status?.state === "running")) return;
        const timer = window.setInterval(() => {
            void Promise.all(sources.map(async (source) => [source.kind, await importStatus(source.id)] as const))
                .then(async (entries) => {
                    setStatuses(Object.fromEntries(entries));
                    if (entries.some(([kind, next]) => statuses[kind]?.state === "running" && next.state !== "running")) {
                        setSources(await importListSources());
                    }
                })
                .catch(() => setError("Could not refresh import progress."));
        }, 1000);
        return () => window.clearInterval(timer);
    }, [sources, statuses]);

    const hasImported = sources.some((source) => source.row_count > 0 || (statuses[source.kind]?.stored ?? 0) > 0);
    useEffect(() => {
        if (!hasImported || mode !== "onboarding") return;
        void listMemoryCards(100).then((cards) => {
            const titles = cards.filter((card) => card.source_type?.startsWith("import:"))
                .map((card) => card.title.trim()).filter(Boolean);
            const uniqueTitles = [...new Set(titles)];
            const shorter = uniqueTitles.flatMap((title) => {
                const words = title.split(/\s+/);
                return words.length > 1 ? [words.slice(0, -1).join(" "), words.slice(1).join(" ")] : [];
            });
            setSuggestions([...new Set([...uniqueTitles, ...shorter])].slice(0, 3));
        }).catch(() => setError("Imported items are ready, but suggestions could not load."));
    }, [hasImported, mode]);

    const act = async (work: () => Promise<void>) => {
        setBusy(true);
        setError(null);
        try { await work(); }
        catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
        finally { setBusy(false); }
    };

    const turn = (kind: ImportKind, enabled: boolean) => void act(async () => {
        await importSetConsent(kind, enabled);
        setConsent((current) => ({ ...current, [kind]: enabled }));
        if (enabled) {
            if (!localStorage.getItem(FIRST_STARTED_KEY)) {
                startedAt.current = Date.now();
                localStorage.setItem(FIRST_STARTED_KEY, String(startedAt.current));
            }
            const status = await importStart(kind, windows[kind] ?? 30);
            setStatuses((current) => ({ ...current, [kind]: status }));
        } else {
            await importCancel(kind);
        }
    });

    const recommended = () => void act(async () => {
        if (!localStorage.getItem(FIRST_STARTED_KEY)) {
            startedAt.current = Date.now();
            localStorage.setItem(FIRST_STARTED_KEY, String(startedAt.current));
        }
        for (const kind of RECOMMENDED) {
            if (!sources.some((source) => source.kind === kind)) continue;
            await importSetConsent(kind, true);
            setConsent((current) => ({ ...current, [kind]: true }));
            const status = await importStart(kind, 30);
            setStatuses((current) => ({ ...current, [kind]: status }));
        }
    });

    const search = async () => {
        if (!query.trim()) return;
        await act(async () => {
            const cards = await searchMemoryCards(query.trim(), undefined, undefined, 10);
            setResults(cards.filter((card) => card.source_type?.startsWith("import:")));
            setSearched(true);
        });
    };

    const open = (card: MemoryCard) => void act(async () => {
        const outcome = await reopenMemory(card.id);
        if ((outcome.kind === "opened" || outcome.kind === "opened_moved") && !firstAnswerRecorded.current) {
            firstAnswerRecorded.current = true;
            const elapsed = Math.max(0, Date.now() - startedAt.current);
            await importRecordFirstAnswer(elapsed);
            setAnswerMs(elapsed);
        } else if (outcome.kind !== "opened" && outcome.kind !== "opened_moved") {
            setError("FNDR could not reopen that item. Choose another result.");
        }
    });

    return (
        <section className="import-sources" aria-label="Bring your past">
            <h2>Bring your past</h2>
            <p>Choose what FNDR may read on this Mac. Every source starts off. The first scan covers 30 days.</p>
            {error && <p role="alert">{error}</p>}
            {loading && <p role="status">Loading sources…</p>}
            {error && <button type="button" onClick={() => void load()}>Retry</button>}
            <div className="import-source-grid">
                {SOURCES.map(({ kind, title, reads, permission }) => {
                    const source = sources.find((entry) => entry.kind === kind);
                    const status = statuses[kind] ?? source?.status;
                    const count = Math.max(source?.row_count ?? 0, status?.stored ?? 0);
                    return <div className="import-source-card" role="group" aria-label={title} key={kind}>
                        <label>
                            <span><strong>{title}</strong><small>{reads}</small><small>{permission}</small></span>
                            <input type="checkbox" role="switch" aria-label={title} checked={Boolean(consent[kind])}
                                disabled={!source || busy} onChange={(event) => turn(kind, event.target.checked)} />
                        </label>
                        {!source && !loading && <small>Not available in this build.</small>}
                        {source && <>
                            <small>{count} items imported{status?.state === "running" ? `, ${status.scanned} scanned` : ""}.</small>
                            {status?.state === "running" && <button type="button" onClick={() => void act(async () => { await importCancel(kind); const next = await importStatus(kind); setStatuses((current) => ({ ...current, [kind]: next })); })}>Cancel</button>}
                            {status?.state === "failed" && <small role="alert">{status.error ?? "Import failed."}</small>}
                            {mode === "settings" && <>
                                <label>Window days <input type="number" min={1} max={90} value={windows[kind] ?? 30}
                                    onChange={(event) => {
                                        const days = Math.max(1, Math.min(90, Number(event.target.value) || 1));
                                        setWindows((current) => ({ ...current, [kind]: days }));
                                        localStorage.setItem(windowKey(kind), String(days));
                                    }} /></label>
                                <small>Last import: {status?.finished_at_ms ? new Date(status.finished_at_ms).toLocaleString() : "Never"}</small>
                                <button type="button" disabled={busy || !consent[kind]} onClick={() => void act(async () => { const next = await importStart(kind, windows[kind] ?? 30); setStatuses((current) => ({ ...current, [kind]: next })); })}>Import again</button>
                                <button type="button" disabled={busy || status?.state === "running"} onClick={() => void act(async () => {
                                    setSources(await importListSources());
                                    setForgetKind(kind);
                                })}>Forget</button>
                            </>}
                        </>}
                    </div>;
                })}
            </div>
            {mode === "onboarding" && <>
                <button type="button" disabled={busy || loading || !RECOMMENDED.some((kind) => sources.some((source) => source.kind === kind))} onClick={recommended}>Import everything recommended</button>
                {hasImported && <div className="import-guided-search">
                    <h3>Where did I see that?</h3>
                    <p>Try a title from the imported items.</p>
                    {suggestions.map((title) => <button type="button" key={title} onClick={() => setQuery(title)}>{title}</button>)}
                    <label>Where did I see that?<input type="search" value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void search(); }} /></label>
                    <button type="button" onClick={() => void search()}>Search</button>
                    {searched && results.length === 0 && <p>No imported result yet.</p>}
                    {results.map((card) => <button type="button" key={card.id} onClick={() => open(card)}>Open {card.title}</button>)}
                </div>}
                <button type="button" onClick={onContinue}>{hasImported ? "Continue to FNDR" : "Skip imports"}</button>
            </>}
            {mode === "settings" && answerMs !== null && <p>First imported answer: {(answerMs / 1000).toFixed(1)} seconds. Stored locally.</p>}
            {forgetKind && <div ref={forgetDialog} role="alertdialog" aria-label="Forget source" aria-modal="true">
                <p>Forget {SOURCES.find((entry) => entry.kind === forgetKind)?.title} and remove {sources.find((entry) => entry.kind === forgetKind)?.row_count ?? 0} items?</p>
                <button ref={keepButton} type="button" onClick={() => setForgetKind(null)}>Keep items</button>
                <button type="button" onClick={() => void act(async () => {
                    const kind = forgetKind;
                    await importForget(kind);
                    setForgetKind(null);
                    await load();
                })}>Remove {sources.find((entry) => entry.kind === forgetKind)?.row_count ?? 0} items</button>
            </div>}
        </section>
    );
}

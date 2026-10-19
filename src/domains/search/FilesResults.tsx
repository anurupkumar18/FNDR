import { useEffect, useRef, useState } from "react";
import { filesQuickLook, filesSearch, type FileHit, type FileKind, type FilesOpenSearch } from "@/shared/ipc/files";
import { FilePreview, FileResult } from "@/shared/files/FileResult";
import { copyFileHit, openFileHit, revealFileHit } from "@/shared/files/fileActions";
import { fileName, hitPlace } from "@/shared/files/fileHit";
import { useThumbnails } from "@/shared/files/useThumbnails";
import { dragOutHandlers } from "@/shared/files/dragOut";
import "./FilesResults.css";

const LIMIT = 40;
/** Thumbnails asked for at once: the first rows and tiles. */
const THUMBNAILS = 48;

interface Filter {
    label: string;
    /** null: every kind. */
    kinds: FileKind[] | null;
    /** Code lives in its own index, searched with `code:`. */
    code?: boolean;
}

/** DigUp's window filters, ⌘1 to ⌘8 in this order. */
export const FILTERS: Filter[] = [
    { label: "All", kinds: null },
    { label: "Pictures", kinds: ["image"] },
    { label: "Screenshots", kinds: ["screenshot"] },
    { label: "PDFs", kinds: ["pdf"] },
    { label: "Documents", kinds: ["doc"] },
    { label: "Recordings", kinds: ["audio"] },
    { label: "Videos", kinds: ["video"] },
    { label: "Code", kinds: null, code: true },
];

function isPicture(hit: FileHit): boolean {
    return hit.kind === "image" || hit.kind === "screenshot";
}

/** The query without a `file:` or `code:` prefix. */
function plain(query: string): string {
    return query.replace(/^\s*(file|code):\s*/i, "").trim();
}

function filterFor(open: FilesOpenSearch): number {
    if (/^\s*code:/i.test(open.query) || open.kinds?.includes("code")) return FILTERS.findIndex((item) => item.code);
    const kind = open.kinds?.[0];
    const index = kind ? FILTERS.findIndex((item) => item.kinds?.length === 1 && item.kinds[0] === kind) : -1;
    return Math.max(0, index);
}

/**
 * Files search in Search and Ask (ADR 033), DigUp's bigger window: the same
 * files Quick Find finds, kind filters with counts, a picture grid, and the
 * keyboard (arrows, ↵ open, ⌘↵ Finder, Space Quick Look, ⌘Y preview, ⌘C copy,
 * ⌘1 to ⌘8 filters). Shown only when Files search found something, so it
 * never adds an empty section to a memory search.
 */
export function FilesResults({ query, opened }: { query: string; opened?: FilesOpenSearch | null }) {
    const [filter, setFilter] = useState(0);
    const [all, setAll] = useState<FileHit[]>([]);
    const [code, setCode] = useState<FileHit[]>([]);
    const [hits, setHits] = useState<FileHit[]>([]);
    const [strong, setStrong] = useState(0);
    const [selected, setSelected] = useState(0);
    const [preview, setPreview] = useState<FileHit | null>(null);
    const [quickLook, setQuickLook] = useState(false);
    const [note, setNote] = useState<string | null>(null);
    const section = useRef<HTMLElement>(null);
    const text = plain(query);

    // Quick Find's ⌘O and "Show all" land here with the filter they meant.
    const quickLookOpen = useRef(false);
    quickLookOpen.current = quickLook;
    useEffect(
        () => () => {
            if (quickLookOpen.current) void filesQuickLook().catch(() => undefined);
        },
        [],
    );

    const wantFocus = useRef(false);
    useEffect(() => {
        if (!opened) return;
        setFilter(filterFor(opened));
        wantFocus.current = true;
    }, [opened]);
    // The section shows once its results arrive; the keys go to it then.
    useEffect(() => {
        if (wantFocus.current && hits.length && section.current) {
            wantFocus.current = false;
            section.current.focus();
        }
    }, [hits]);

    // Every kind, and code, for the counts on the filters.
    useEffect(() => {
        let live = true;
        if (!text) {
            setAll([]);
            setCode([]);
            return;
        }
        filesSearch(text, undefined, LIMIT)
            .then((result) => live && setAll(result.hits))
            .catch(() => live && setAll([]));
        filesSearch(`code: ${text}`, undefined, LIMIT)
            .then((result) => live && setCode(result.hits))
            .catch(() => live && setCode([]));
        return () => {
            live = false;
        };
    }, [text]);

    // The chosen filter, searched on its own so a kind gets its full list.
    useEffect(() => {
        let live = true;
        setPreview(null);
        setSelected(0);
        if (!text) {
            setHits([]);
            return;
        }
        const chosen = FILTERS[filter];
        filesSearch(chosen.code ? `code: ${text}` : text, chosen.kinds ?? undefined, LIMIT)
            .then((result) => {
                if (!live) return;
                setHits(result.hits);
                setStrong(result.strong);
            })
            .catch(() => live && setHits([]));
        return () => {
            live = false;
        };
    }, [text, filter]);

    const pictures = hits.filter(isPicture);
    const others = hits.filter((hit) => !isPicture(hit));
    // Display order: the grid, then the list. The arrows follow it.
    const ordered = [...pictures, ...others];
    const current = ordered[selected];
    const tiles = useThumbnails(pictures.slice(0, THUMBNAILS).map((hit) => hit.path), 320);
    const icons = useThumbnails(others.slice(0, THUMBNAILS).map((hit) => hit.path));

    // Quick Look and the preview follow the selection.
    useEffect(() => {
        if (!current) return;
        if (quickLook) {
            void filesQuickLook(current.path, true)
                .then(setQuickLook)
                .catch(() => setQuickLook(false));
        }
        setPreview((open) => (open ? current : open));
    }, [current?.path]);

    if (!text || (!all.length && !code.length && filter === 0)) {
        return null;
    }

    const count = (item: Filter): number =>
        item.code ? code.length : item.kinds ? all.filter((hit) => item.kinds?.includes(hit.kind)).length : all.length;

    const act = (action: Promise<unknown>, done: string) => {
        setNote(null);
        void action.then(() => setNote(done)).catch(() => setNote("That did not work. Try again."));
    };
    const open = (hit: FileHit) =>
        act(openFileHit(hit, query), `Opened ${fileName(hit.path)}${hitPlace(hit) ? ` ${hitPlace(hit)}` : ""}.`);
    const toggleQuickLook = () => {
        void filesQuickLook(current?.path)
            .then(setQuickLook)
            .catch(() => setQuickLook(false));
    };

    // Rows in the grid, for ↑↓ there.
    const columns = () => {
        const grid = section.current?.querySelector(".files-results-grid");
        if (!grid) return 1;
        return Math.max(1, getComputedStyle(grid).gridTemplateColumns.split(" ").length);
    };

    const onKeyDown = (event: React.KeyboardEvent) => {
        const command = event.metaKey || event.ctrlKey;
        if (command && /^[1-8]$/.test(event.key)) {
            event.preventDefault();
            setFilter(Number(event.key) - 1);
            return;
        }
        if (!current) return;
        const inGrid = selected < pictures.length;
        const step = (by: number) => setSelected((index) => Math.min(Math.max(index + by, 0), ordered.length - 1));
        if (event.key === "ArrowRight" && inGrid) step(1);
        else if (event.key === "ArrowLeft" && inGrid) step(-1);
        else if (event.key === "ArrowDown") step(inGrid ? Math.min(columns(), pictures.length - selected) : 1);
        else if (event.key === "ArrowUp") step(inGrid || selected === pictures.length ? -columns() : -1);
        else if (event.key === "Enter" && command) act(revealFileHit(current), "Shown in Finder.");
        else if (event.key === "Enter") open(current);
        else if (event.key === " ") toggleQuickLook();
        else if (command && event.key.toLowerCase() === "y") setPreview((shown) => (shown ? null : current));
        else if (command && event.key.toLowerCase() === "c") act(copyFileHit(current), "Copied the file.");
        else if (event.key === "Escape" && (quickLook || preview)) {
            if (quickLook) {
                setQuickLook(false);
                void filesQuickLook().catch(() => undefined);
            }
            else setPreview(null);
        } else return;
        event.preventDefault();
    };

    return (
        <section
            ref={section}
            className="files-results"
            aria-label="Files"
            tabIndex={0}
            onKeyDown={onKeyDown}
            aria-activedescendant={current ? `files-result-${selected}` : undefined}
        >
            <header className="files-results-header">
                <h2>Files</h2>
                <div className="files-results-filters" role="tablist" aria-label="Kinds of files">
                    {FILTERS.map((item, index) => {
                        const found = count(item);
                        if (index > 0 && !found && index !== filter) return null;
                        return (
                            <button
                                key={item.label}
                                type="button"
                                role="tab"
                                aria-selected={index === filter}
                                aria-keyshortcuts={`Meta+${index + 1}`}
                                className="files-results-filter"
                                onClick={() => setFilter(index)}
                            >
                                {item.label}
                                <span className="files-results-count">{found}</span>
                            </button>
                        );
                    })}
                </div>
            </header>

            {pictures.length > 0 && (
                <div className="files-results-grid" role="listbox" aria-label="Pictures">
                    {pictures.map((hit, index) => (
                        <button
                            key={`${hit.path}-${hit.segment_id}`}
                            id={`files-result-${index}`}
                            type="button"
                            role="option"
                            aria-selected={index === selected}
                            data-selected={index === selected}
                            className="files-results-tile"
                            title={fileName(hit.path)}
                            onMouseEnter={() => setSelected(index)}
                            onClick={() => open(hit)}
                            {...dragOutHandlers(hit.path, tiles[hit.path])}
                            onContextMenu={(event) => {
                                event.preventDefault();
                                setPreview(hit);
                            }}
                        >
                            {tiles[hit.path] ? (
                                <img src={tiles[hit.path]} alt={fileName(hit.path)} draggable={false} />
                            ) : (
                                <span className="files-results-tile-name">{fileName(hit.path)}</span>
                            )}
                        </button>
                    ))}
                </div>
            )}

            {others.length > 0 && (
                <div className="files-results-list" role="listbox" aria-label="Files found">
                    {others.map((hit, offset) => {
                        const index = pictures.length + offset;
                        return (
                            <FileResult
                                key={`${hit.path}-${hit.segment_id}`}
                                id={`files-result-${index}`}
                                hit={hit}
                                selected={index === selected}
                                weaker={hits.indexOf(hit) >= strong}
                                thumbnail={icons[hit.path]}
                                onSelect={() => setSelected(index)}
                                onOpen={() => open(hit)}
                            />
                        );
                    })}
                </div>
            )}

            {!hits.length && <p className="files-results-empty">No {FILTERS[filter].label.toLowerCase()} match.</p>}

            {current && (
                <div className="files-results-actions">
                    <button type="button" onClick={() => open(current)}>
                        Open
                    </button>
                    <button type="button" onClick={() => act(revealFileHit(current), "Shown in Finder.")}>
                        Show in Finder
                    </button>
                    <button type="button" onClick={() => act(copyFileHit(current), "Copied the file.")}>
                        Copy file
                    </button>
                    <button type="button" onClick={() => setPreview(current)}>
                        Preview
                    </button>
                    <button type="button" onClick={toggleQuickLook}>
                        Quick Look
                    </button>
                    {note && <span role="status">{note}</span>}
                </div>
            )}

            {preview && <FilePreview hit={preview} query={query} onClose={() => setPreview(null)} />}
        </section>
    );
}

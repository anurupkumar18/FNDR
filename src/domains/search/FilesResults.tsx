import { useEffect, useState } from "react";
import { filesPreview, filesSearch, type FileHit, type FileKind } from "@/shared/ipc/files";
import { FilePreview, FileResult } from "@/shared/files/FileResult";
import { copyFileHit, openFileHit, revealFileHit } from "@/shared/files/fileActions";
import { fileName, hitPlace } from "@/shared/files/fileHit";
import "./FilesResults.css";

const LIMIT = 40;
/** Pictures shown as thumbnails at once; more load as the person filters. */
const THUMBNAILS = 12;

const FILTERS: { label: string; kinds: FileKind[] | null }[] = [
    { label: "All", kinds: null },
    { label: "Pictures", kinds: ["image", "screenshot"] },
    { label: "PDFs", kinds: ["pdf"] },
    { label: "Documents", kinds: ["doc"] },
    { label: "Recordings", kinds: ["audio"] },
    { label: "Videos", kinds: ["video"] },
];

function isPicture(hit: FileHit): boolean {
    return hit.kind === "image" || hit.kind === "screenshot";
}

/**
 * Files search in Search and Ask (ADR 033): the same files Quick Find finds,
 * with kind filters and a picture grid. Shown only when Files search found
 * something, so it never adds an empty section to a memory search.
 */
export function FilesResults({ query }: { query: string }) {
    const [filter, setFilter] = useState(0);
    const [hits, setHits] = useState<FileHit[]>([]);
    const [strong, setStrong] = useState(0);
    const [selected, setSelected] = useState(0);
    const [preview, setPreview] = useState<FileHit | null>(null);
    const [thumbnails, setThumbnails] = useState<Record<string, string>>({});
    const [note, setNote] = useState<string | null>(null);

    useEffect(() => {
        let live = true;
        const trimmed = query.trim();
        setPreview(null);
        setSelected(0);
        if (!trimmed) {
            setHits([]);
            return;
        }
        const kinds = FILTERS[filter].kinds ?? undefined;
        filesSearch(trimmed, kinds, LIMIT)
            .then((result) => {
                if (!live) return;
                setHits(result.hits);
                setStrong(result.strong);
            })
            .catch(() => live && setHits([]));
        return () => {
            live = false;
        };
    }, [query, filter]);

    useEffect(() => {
        let live = true;
        for (const hit of hits.filter(isPicture).slice(0, THUMBNAILS)) {
            if (thumbnails[hit.path]) continue;
            filesPreview(hit.path, hit.kind)
                .then((url) => live && setThumbnails((current) => ({ ...current, [hit.path]: url })))
                .catch(() => undefined);
        }
        return () => {
            live = false;
        };
        // Thumbnails already loaded are kept; only new hits are asked for.
    }, [hits]);

    if (!query.trim() || (!hits.length && filter === 0)) {
        return null;
    }

    const act = (action: Promise<unknown>, done: string) => {
        setNote(null);
        void action.then(() => setNote(done)).catch(() => setNote("That did not work. Try again."));
    };
    const pictures = hits.filter(isPicture);
    const others = hits.filter((hit) => !isPicture(hit));
    const current = hits[selected];

    return (
        <section className="files-results" aria-label="Files">
            <header className="files-results-header">
                <h2>Files</h2>
                <div className="files-results-filters" role="tablist" aria-label="Kinds of files">
                    {FILTERS.map((item, index) => (
                        <button
                            key={item.label}
                            type="button"
                            role="tab"
                            aria-selected={index === filter}
                            className="files-results-filter"
                            onClick={() => setFilter(index)}
                        >
                            {item.label}
                        </button>
                    ))}
                </div>
            </header>

            {pictures.length > 0 && (
                <div className="files-results-grid" role="list" aria-label="Pictures">
                    {pictures.map((hit) => (
                        <button
                            key={`${hit.path}-${hit.segment_id}`}
                            type="button"
                            role="listitem"
                            className="files-results-tile"
                            title={fileName(hit.path)}
                            onClick={() => act(openFileHit(hit, query), `Opened ${fileName(hit.path)}.`)}
                            onContextMenu={(event) => {
                                event.preventDefault();
                                setPreview(hit);
                            }}
                        >
                            {thumbnails[hit.path] ? (
                                <img src={thumbnails[hit.path]} alt={fileName(hit.path)} />
                            ) : (
                                <span className="files-results-tile-name">{fileName(hit.path)}</span>
                            )}
                        </button>
                    ))}
                </div>
            )}

            {others.length > 0 && (
                <div className="files-results-list" role="listbox" aria-label="Files found">
                    {others.map((hit) => {
                        const index = hits.indexOf(hit);
                        return (
                            <FileResult
                                key={`${hit.path}-${hit.segment_id}`}
                                id={`files-result-${index}`}
                                hit={hit}
                                selected={index === selected}
                                weaker={index >= strong}
                                onSelect={() => setSelected(index)}
                                onOpen={() => act(openFileHit(hit, query), `Opened ${fileName(hit.path)}${hitPlace(hit) ? ` ${hitPlace(hit)}` : ""}.`)}
                            />
                        );
                    })}
                </div>
            )}

            {!hits.length && <p className="files-results-empty">No {FILTERS[filter].label.toLowerCase()} match.</p>}

            {current && (
                <div className="files-results-actions">
                    <button type="button" onClick={() => act(openFileHit(current, query), "Opened.")}>
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
                    {note && <span role="status">{note}</span>}
                </div>
            )}

            {preview && <FilePreview hit={preview} onClose={() => setPreview(null)} />}
        </section>
    );
}

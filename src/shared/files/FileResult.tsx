import { useEffect, useState } from "react";
import { filesPreview, type FileHit } from "@/shared/ipc/files";
import { excerptParts, fileFolder, fileName, hasPreview, hitMoment, hitPage, hitPlace, kindLabel } from "./fileHit";
import "./FileResult.css";

interface FileResultProps {
    hit: FileHit;
    id: string;
    selected: boolean;
    /** Ranked below the strong matches. */
    weaker?: boolean;
    onSelect: () => void;
    onOpen: () => void;
    /** Class names of the surface's own result rows, so a file row looks like its neighbours. */
    className?: string;
}

/** One found file: its name, kind and place, the words that matched, where it lives. */
export function FileResult({ hit, id, selected, weaker, onSelect, onOpen, className }: FileResultProps) {
    const place = hitPlace(hit);
    return (
        <button
            id={id}
            type="button"
            role="option"
            aria-selected={selected}
            data-selected={selected}
            data-weaker={weaker || undefined}
            className={`file-result ${className ?? ""}`}
            onMouseEnter={onSelect}
            onClick={onOpen}
        >
            <span className="file-result-title">
                <span className="file-result-kind" data-kind={hit.kind}>
                    {kindLabel(hit.kind)}
                </span>
                {fileName(hit.path)}
                {place && <span className="file-result-place">{place}</span>}
            </span>
            {hit.excerpt && (
                <span className="file-result-excerpt">
                    {excerptParts(hit.excerpt).map((part, index) =>
                        part.marked ? <mark key={index}>{part.text}</mark> : <span key={index}>{part.text}</span>,
                    )}
                </span>
            )}
            <span className="file-result-folder">{fileFolder(hit.path)}</span>
        </button>
    );
}

/** A big preview at the page or moment: the picture, the matched PDF page, the video frame. */
export function FilePreview({ hit, onClose }: { hit: FileHit; onClose: () => void }) {
    const [image, setImage] = useState<string | null>(null);
    const [failed, setFailed] = useState(false);
    useEffect(() => {
        let live = true;
        setImage(null);
        setFailed(false);
        if (!hasPreview(hit.kind)) {
            setFailed(true);
            return;
        }
        filesPreview(hit.path, hit.kind, hitPage(hit), hitMoment(hit))
            .then((url) => live && setImage(url))
            .catch(() => live && setFailed(true));
        return () => {
            live = false;
        };
    }, [hit]);
    return (
        <div className="file-preview" role="region" aria-label={`Preview of ${fileName(hit.path)}`}>
            <div className="file-preview-header">
                <span>
                    {fileName(hit.path)}
                    {hitPlace(hit) && <span className="file-result-place">{hitPlace(hit)}</span>}
                </span>
                <button type="button" className="file-preview-close" onClick={onClose} aria-label="Close preview">
                    ×
                </button>
            </div>
            {image && <img className="file-preview-image" src={image} alt="" />}
            {!image && !failed && <div className="file-preview-loading" aria-hidden />}
            {failed && (
                <p className="file-preview-text">
                    {hit.excerpt ? excerptParts(hit.excerpt).map((part) => part.text).join("") : "No preview for this file."}
                </p>
            )}
        </div>
    );
}

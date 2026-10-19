import { useEffect, useState } from "react";
import { filesClip, filesPassage, filesPreview, filesReveal, type FileHit } from "@/shared/ipc/files";
import {
    clock,
    excerptParts,
    fileDetails,
    fileFolder,
    fileName,
    hasPreview,
    hitLine,
    hitMoment,
    hitPage,
    hitPlace,
    kindLabel,
    matchReason,
    queryWords,
} from "./fileHit";
import "./FileResult.css";

interface FileResultProps {
    hit: FileHit;
    id: string;
    selected: boolean;
    /** Ranked below the strong matches. */
    weaker?: boolean;
    /** Quick Look's thumbnail, as a data URL; the kind shows until it comes. */
    thumbnail?: string;
    onSelect: () => void;
    onOpen: () => void;
    /** Class names of the surface's own result rows, so a file row looks like its neighbours. */
    className?: string;
}

/** One found file: its name, kind and place, the words that matched, where it lives. */
export function FileResult({ hit, id, selected, weaker, thumbnail, onSelect, onOpen, className }: FileResultProps) {
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
            data-thumbnail={thumbnail !== undefined || undefined}
            onMouseEnter={onSelect}
            onClick={onOpen}
        >
            {thumbnail !== undefined && (
                <img className="file-result-thumbnail" src={thumbnail} alt="" aria-hidden draggable={false} />
            )}
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

/** The words of `text` the query has, marked, for a passage. */
function marked(text: string, words: string[]) {
    if (!words.length) return text;
    const pattern = new RegExp(`(${words.map((word) => word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "giu");
    return text.split(pattern).map((part, index) => (index % 2 ? <mark key={index}>{part}</mark> : part));
}

/**
 * DigUp's preview pane, at the page or moment: the picture, the matched page
 * or frame; a recording or video played from where it matched; the passage
 * or the lines of code; then what the file is, where it lives (click for
 * Finder) and why it is here.
 */
export function FilePreview({ hit, query = "", onClose }: { hit: FileHit; query?: string; onClose: () => void }) {
    const [image, setImage] = useState<string | null>(null);
    const [failed, setFailed] = useState(false);
    const [passage, setPassage] = useState<string | null>(null);
    const [clip, setClip] = useState<string | null>(null);
    const [clipState, setClipState] = useState<"idle" | "loading" | "failed">("idle");
    const moment = hitMoment(hit);
    const playable = (hit.kind === "audio" || hit.kind === "video") && moment !== undefined;
    // A PDF shows its page; a document and code show the passage that matched.
    const textual = hit.kind === "code" || hit.kind === "doc";

    useEffect(() => {
        let live = true;
        setImage(null);
        setFailed(false);
        setClip(null);
        setClipState("idle");
        setPassage(null);
        if (hasPreview(hit.kind)) {
            filesPreview(hit.path, hit.kind, hitPage(hit), moment)
                .then((url) => live && setImage(url))
                .catch(() => live && setFailed(true));
        }
        if (textual) {
            filesPassage(hit.path, hit.segment_id)
                .then((text) => live && setPassage(text))
                .catch(() => undefined);
        }
        return () => {
            live = false;
        };
    }, [hit]);

    const play = () => {
        if (moment === undefined) return;
        setClipState("loading");
        filesClip(hit.path, hit.kind, moment)
            .then((url) => {
                setClip(url);
                setClipState("idle");
            })
            .catch(() => setClipState("failed"));
    };

    const words = queryWords(query);
    const firstLine = hit.kind === "code" && hit.segment === "lines" ? Math.max(1, Math.round(hit.loc ?? 1)) : null;
    const focus = hitLine(hit);
    const span = moment !== undefined ? `${clock(moment)} – ${clock(hit.loc_end ?? moment + 30)}` : null;

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
            {!image && !failed && hasPreview(hit.kind) && (
                <div className="file-preview-loading" aria-hidden />
            )}
            {playable && (
                <div className="file-preview-play">
                    {clip ? (
                        hit.kind === "video" ? (
                            <video className="file-preview-media" src={clip} controls autoPlay />
                        ) : (
                            <audio className="file-preview-media" src={clip} controls autoPlay />
                        )
                    ) : (
                        <button type="button" className="file-preview-button" onClick={play} disabled={clipState === "loading"}>
                            {clipState === "loading" ? "Getting it ready…" : `Play from ${clock(moment)}`}
                        </button>
                    )}
                    {span && <span className="file-preview-span">{span}</span>}
                    {clipState === "failed" && <span className="file-preview-span">It won’t play here. Open the file instead.</span>}
                </div>
            )}
            {passage && firstLine !== null ? (
                <pre className="file-preview-code" aria-label="The lines that matched">
                    {passage.split("\n").map((line, index) => (
                        <span key={index} className="file-preview-line" data-focus={firstLine + index === focus || undefined}>
                            <span className="file-preview-number">{firstLine + index}</span>
                            {marked(line, words)}
                            {"\n"}
                        </span>
                    ))}
                </pre>
            ) : passage ? (
                <p className="file-preview-text">{marked(passage, words)}</p>
            ) : null}
            {failed && !passage && !playable && (
                <p className="file-preview-text">
                    {hit.excerpt ? excerptParts(hit.excerpt).map((part) => part.text).join("") : "No preview for this file."}
                </p>
            )}
            <div className="file-preview-about">
                <button
                    type="button"
                    className="file-preview-folder"
                    title="Show in Finder"
                    onClick={() => void filesReveal(hit.path).catch(() => undefined)}
                >
                    {fileFolder(hit.path)}
                </button>
                <span>{fileDetails(hit)}</span>
                <span className="file-preview-why">{matchReason(hit)}</span>
                {hit.excerpt && hit.kind !== "code" && !passage && (
                    <span className="file-preview-excerpt">
                        {excerptParts(hit.excerpt).map((part, index) =>
                            part.marked ? <mark key={index}>{part.text}</mark> : <span key={index}>{part.text}</span>,
                        )}
                    </span>
                )}
            </div>
        </div>
    );
}

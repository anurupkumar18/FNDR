import { useCallback, useEffect, useRef, useState } from "react";
import {
    type ClipboardEntry,
    type ComposedAnswer,
    type MemoryCard,
    OMNIBAR_FOCUS_EVENT,
    copyClipboardEntry,
    dismissOmnibar,
    fndrAnswer,
    notchDoRequest,
    notchDoSkills,
    getClipboardHistory,
    omnibarOpenMemory,
    pasteClipboardEntry,
    searchMemoryCards,
    type NotchDoSkill,
} from "@/shared/ipc/tauri";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStatus,
} from "@/shared/activity/activityTrace";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import { filesOpenInWindow, filesQuickLook, type FileHit } from "@/shared/ipc/files";
import { FilePreview, FileResult } from "@/shared/files/FileResult";
import { copyFileHit, openFileHit, revealFileHit } from "@/shared/files/fileActions";
import { filesNote, isFileQuery, openTitle } from "@/shared/files/fileHit";
import { fileRows, kindPlural } from "@/shared/files/fileRows";
import { useEditorName } from "@/shared/files/useEditorName";
import { useFileSearch } from "@/shared/files/useFileSearch";
import { useFilesStatus } from "@/shared/files/useFilesStatus";
import { useThumbnails } from "@/shared/files/useThumbnails";

const SEARCH_DEBOUNCE_MS = 250;
const CLIP_DEBOUNCE_MS = 150;
const RESULT_LIMIT = 8;
/** Enough to group by kind with "Show all"; the weaker ones stay folded. */
const FILE_LIMIT = 40;
const CLIP_LIMIT = 30;
const COPIED_FLASH_MS = 550;

type Surface = "memory" | "clipboard";

type Mode =
    | { kind: "search" }
    | { kind: "asking" }
    | { kind: "answer"; answer: ComposedAnswer };

type Feedback = { kind: "error" | "status"; message: string };

function formatTimestamp(ms: number): string {
    const date = new Date(ms);
    const now = new Date();
    const sameDay = date.toDateString() === now.toDateString();
    if (sameDay) {
        return date.toLocaleTimeString(undefined, {
            hour: "numeric",
            minute: "2-digit",
        });
    }
    return date.toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
    });
}

function answerGroundingLabel(answer: ComposedAnswer): string {
    const count = answer.cards.length;
    const memories = `${count} local ${count === 1 ? "memory" : "memories"}`;
    if (answer.verify_outcome.kind === "grounded") {
        return `Grounded in ${memories}`;
    }
    if (answer.verify_outcome.kind === "partial_answer") {
        return `Partial answer from ${memories}`;
    }
    return "Limited evidence in your local memory";
}

/** Words a request to do something starts with. */
const ACTION_OPENERS = [
    "open", "launch", "start", "look up", "search for", "google", "play", "go to", "switch to",
    "show", "pull up", "bring up", "bring back", "set up", "save this", "save it",
];

/** The text to hand to Notch Do, when the query asks FNDR to do something. */
export function requestToDo(query: string): string | null {
    const text = query.trim();
    const lower = text.toLowerCase().replace(/^(please|hey fndr|fndr)\s+/, "");
    const asks = ACTION_OPENERS.some((opener) => lower.startsWith(`${opener} `));
    return asks && text.split(/\s+/).length >= 2 ? text : null;
}

/** The saved skill a query asks to run: its whole name ("Monday setup"),
 *  or, after "run", "do" or "start", its first three letters or more ("run
 *  mon"). A bare prefix stays a memory search, so typing "notes" never runs a
 *  skill called "Notes setup". */
export function skillNamed(query: string, skills: NotchDoSkill[]): NotchDoSkill | null {
    const words = query
        .trim()
        .toLowerCase()
        .replace(/\s+/g, " ")
        .replace(/[.!?]+$/, "")
        .replace(/^(please|hey fndr|fndr) /, "");
    const asked = /^(run|do|start) /.test(words);
    const said = words.replace(/^(run|do|start) /, "").replace(/^(my|the) /, "");
    if (said.length < 3) return null;
    const name = (skill: NotchDoSkill) => skill.name.toLowerCase().replace(/^(my|the) /, "");
    return skills.find((skill) => (asked ? name(skill).startsWith(said) : name(skill) === said)) ?? null;
}

function matchCountLabel(count: number): string {
    return `${count} ${count === 1 ? "match" : "matches"}`;
}

function answerActivityResult(answer: ComposedAnswer): {
    label: string;
    status: ActivityTraceStatus;
} {
    const memories = `${answer.cards.length} local ${answer.cards.length === 1 ? "memory" : "memories"}`;
    if (answer.verify_outcome.kind === "grounded") {
        return { label: `Answer grounded in ${memories}`, status: "completed" };
    }
    if (answer.verify_outcome.kind === "partial_answer") {
        return { label: `Partial answer composed from ${memories}`, status: "degraded" };
    }
    return { label: "Answer returned with limited evidence", status: "degraded" };
}

export function OmnibarApp() {
    const [surface, setSurface] = useState<Surface>("memory");
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<MemoryCard[]>([]);
    const [clips, setClips] = useState<ClipboardEntry[]>([]);
    const [selectedIndex, setSelectedIndex] = useState(0);
    const [searching, setSearching] = useState(false);
    const [mode, setMode] = useState<Mode>({ kind: "search" });
    const [copiedId, setCopiedId] = useState<string | null>(null);
    const [feedback, setFeedback] = useState<Feedback | null>(null);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const [previewHit, setPreviewHit] = useState<FileHit | null>(null);
    const [showWeaker, setShowWeaker] = useState(false);
    const [quickLook, setQuickLook] = useState(false);
    // Arrowed into the results: Space then previews instead of typing.
    const [navigated, setNavigated] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);
    const searchSeq = useRef(0);
    const actionSeq = useRef(0);
    const copiedTimer = useRef<number | null>(null);

    const reset = useCallback(() => {
        searchSeq.current += 1;
        actionSeq.current += 1;
        if (copiedTimer.current !== null) {
            window.clearTimeout(copiedTimer.current);
            copiedTimer.current = null;
        }
        setSurface("memory");
        setQuery("");
        setResults([]);
        setClips([]);
        setSelectedIndex(0);
        setSearching(false);
        setMode({ kind: "search" });
        setCopiedId(null);
        setFeedback(null);
        setActivityTrace(null);
        setPreviewHit(null);
    }, []);

    // Skills are read each time the command bar opens, so one saved in the
    // notch or Settings is offered at once.
    const [skills, setSkills] = useState<NotchDoSkill[]>([]);
    const loadSkills = useCallback(() => {
        void notchDoSkills()
            .then(setSkills)
            .catch(() => setSkills([]));
    }, []);
    useEffect(loadSkills, [loadSkills]);

    useTauriEvent<void>(OMNIBAR_FOCUS_EVENT, () => {
        reset();
        loadSkills();
        inputRef.current?.focus();
    });

    useEffect(() => {
        inputRef.current?.focus();
    }, []);

    useEffect(() => {
        if (surface !== "memory" || mode.kind !== "search") {
            return;
        }
        const trimmed = isFileQuery(query) ? "" : query.trim();
        if (!trimmed) {
            setResults([]);
            setSelectedIndex(0);
            setSearching(false);
            setActivityTrace(null);
            return;
        }
        const seq = ++searchSeq.current;
        const traceId = `omnibar-memory-search-${seq}`;
        const startedAt = Date.now();
        setFeedback(null);
        setSearching(true);
        setActivityTrace(recordActivityStep(
            beginActivityTrace({ id: traceId, title: "Quick Find activity", startedAtMs: startedAt }),
            {
                id: "debounce",
                label: "Waiting for typing to settle",
                actor: "Quick Find",
                status: "waiting",
                evidence: "frontend-event",
                atMs: startedAt,
            },
        ));
        const timer = window.setTimeout(() => {
            const requestedAt = Date.now();
            setActivityTrace((current) => {
                if (current?.id !== traceId) return current;
                const withSettledInput = recordActivityStep(current, {
                    id: "debounce",
                    label: "Typing settled",
                    actor: "Quick Find",
                    status: "completed",
                    evidence: "frontend-event",
                    atMs: requestedAt,
                    durationMs: requestedAt - startedAt,
                });
                return recordActivityStep(withSettledInput, {
                    id: "request",
                    label: "Requesting local memory matches",
                    actor: "Memory search",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: requestedAt,
                });
            });
            searchMemoryCards(trimmed, undefined, undefined, RESULT_LIMIT)
                .then((cards) => {
                    if (searchSeq.current !== seq) {
                        return;
                    }
                    setResults(cards);
                    setSelectedIndex(0);
                    setSearching(false);
                    const finishedAt = Date.now();
                    setActivityTrace((current) => {
                        if (current?.id !== traceId) return current;
                        const withCompletedRequest = recordActivityStep(current, {
                            id: "request",
                            label: "Local memory search completed",
                            actor: "Memory search",
                            status: "completed",
                            evidence: "ipc-boundary",
                            atMs: finishedAt,
                            durationMs: finishedAt - requestedAt,
                        });
                        return recordActivityStep(withCompletedRequest, {
                            id: "result",
                            label: `Memory search returned ${matchCountLabel(cards.length)}`,
                            actor: "Memory search",
                            status: "completed",
                            evidence: "result-metadata",
                            atMs: finishedAt,
                        });
                    });
                })
                .catch(() => {
                    if (searchSeq.current !== seq) {
                        return;
                    }
                    setResults([]);
                    setSearching(false);
                    setFeedback({
                        kind: "error",
                        message: "Couldn’t search your memory. Try again.",
                    });
                    const failedAt = Date.now();
                    setActivityTrace((current) => current?.id === traceId
                        ? recordActivityStep(current, {
                            id: "request",
                            label: "Memory search failed",
                            actor: "Memory search",
                            status: "failed",
                            evidence: "ipc-boundary",
                            atMs: failedAt,
                            durationMs: failedAt - requestedAt,
                        })
                        : current);
                });
        }, SEARCH_DEBOUNCE_MS);
        return () => window.clearTimeout(timer);
    }, [query, surface, mode.kind]);

    useEffect(() => {
        if (surface !== "clipboard") {
            return;
        }
        const seq = ++searchSeq.current;
        const traceId = `omnibar-clipboard-search-${seq}`;
        const startedAt = Date.now();
        setFeedback(null);
        setSearching(true);
        setActivityTrace(recordActivityStep(
            beginActivityTrace({ id: traceId, title: "Quick Find activity", startedAtMs: startedAt }),
            {
                id: "debounce",
                label: "Waiting to read clipboard history",
                actor: "Quick Find",
                status: "waiting",
                evidence: "frontend-event",
                atMs: startedAt,
            },
        ));
        const timer = window.setTimeout(() => {
            const requestedAt = Date.now();
            setActivityTrace((current) => {
                if (current?.id !== traceId) return current;
                const withSettledInput = recordActivityStep(current, {
                    id: "debounce",
                    label: "Clipboard request ready",
                    actor: "Quick Find",
                    status: "completed",
                    evidence: "frontend-event",
                    atMs: requestedAt,
                    durationMs: requestedAt - startedAt,
                });
                return recordActivityStep(withSettledInput, {
                    id: "request",
                    label: "Requesting clipboard history",
                    actor: "Clipboard history",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: requestedAt,
                });
            });
            getClipboardHistory(query.trim() || undefined, CLIP_LIMIT)
                .then((entries) => {
                    if (searchSeq.current !== seq) {
                        return;
                    }
                    setClips(entries);
                    setSelectedIndex(0);
                    setSearching(false);
                    const finishedAt = Date.now();
                    setActivityTrace((current) => {
                        if (current?.id !== traceId) return current;
                        const withCompletedRequest = recordActivityStep(current, {
                            id: "request",
                            label: "Clipboard history request completed",
                            actor: "Clipboard history",
                            status: "completed",
                            evidence: "ipc-boundary",
                            atMs: finishedAt,
                            durationMs: finishedAt - requestedAt,
                        });
                        return recordActivityStep(withCompletedRequest, {
                            id: "result",
                            label: `Clipboard search returned ${matchCountLabel(entries.length)}`,
                            actor: "Clipboard history",
                            status: "completed",
                            evidence: "result-metadata",
                            atMs: finishedAt,
                        });
                    });
                })
                .catch(() => {
                    if (searchSeq.current !== seq) {
                        return;
                    }
                    setClips([]);
                    setSearching(false);
                    setFeedback({
                        kind: "error",
                        message: "Couldn’t load clipboard history. Try again.",
                    });
                    const failedAt = Date.now();
                    setActivityTrace((current) => current?.id === traceId
                        ? recordActivityStep(current, {
                            id: "request",
                            label: "Clipboard history request failed",
                            actor: "Clipboard history",
                            status: "failed",
                            evidence: "ipc-boundary",
                            atMs: failedAt,
                            durationMs: failedAt - requestedAt,
                        })
                        : current);
                });
        }, CLIP_DEBOUNCE_MS);
        return () => window.clearTimeout(timer);
    }, [query, surface]);

    useEffect(() => {
        const selected = listRef.current?.querySelector(
            '[data-selected="true"]'
        );
        if (selected && typeof selected.scrollIntoView === "function") {
            selected.scrollIntoView({ block: "nearest" });
        }
    }, [selectedIndex]);

    const doIt = useCallback((text: string) => {
        const seq = ++actionSeq.current;
        setFeedback(null);
        void notchDoRequest(text)
            .then(() => {
                if (actionSeq.current === seq) reset();
            })
            .catch((reason: unknown) => {
                if (actionSeq.current === seq) {
                    setFeedback({
                        kind: "error",
                        message: typeof reason === "string" ? reason : "Notch Do could not take that. Try again.",
                    });
                }
            });
    }, [reset]);

    const openMemory = useCallback(
        (memoryId: string) => {
            const seq = ++actionSeq.current;
            setFeedback(null);
            void omnibarOpenMemory(memoryId)
                .then(() => {
                    if (actionSeq.current === seq) reset();
                })
                .catch(() => {
                    if (actionSeq.current === seq) {
                        setFeedback({
                            kind: "error",
                            message: "Couldn’t open that memory. Try again.",
                        });
                    }
                });
        },
        [reset]
    );

    const copyClip = useCallback(
        (clip: ClipboardEntry) => {
            const seq = ++actionSeq.current;
            setFeedback(null);
            void copyClipboardEntry(clip.text)
                .then(() => {
                    if (actionSeq.current !== seq) return;
                    setCopiedId(clip.id);
                    setFeedback({ kind: "status", message: "Copied to your clipboard." });
                    copiedTimer.current = window.setTimeout(() => {
                        if (actionSeq.current !== seq) return;
                        void dismissOmnibar()
                            .then(reset)
                            .catch(() => {
                                setFeedback({
                                    kind: "error",
                                    message: "Copied, but Quick Find couldn’t close. Use Alt+Space to hide it.",
                                });
                            });
                    }, COPIED_FLASH_MS);
                })
                .catch(() => {
                    if (actionSeq.current === seq) {
                        setCopiedId(null);
                        setFeedback({
                            kind: "error",
                            message: "Couldn’t copy that clip. Try again.",
                        });
                    }
                });
        },
        [reset]
    );

    const pasteClip = useCallback(
        (clip: ClipboardEntry) => {
            const seq = ++actionSeq.current;
            setFeedback(null);
            void pasteClipboardEntry(clip.text)
                .then(() => {
                    if (actionSeq.current === seq) reset();
                })
                .catch(() => {
                    if (actionSeq.current === seq) {
                        setFeedback({
                            kind: "error",
                            message: "Couldn’t paste into the previous app. Focus the field and try again.",
                        });
                    }
                });
        },
        [reset]
    );

    const ask = useCallback(() => {
        const trimmed = query.trim();
        if (!trimmed) {
            return;
        }
        searchSeq.current += 1;
        const seq = ++actionSeq.current;
        const traceId = `omnibar-answer-${seq}`;
        const requestedAt = Date.now();
        setSearching(false);
        setFeedback(null);
        setMode({ kind: "asking" });
        setActivityTrace(recordActivityStep(
            beginActivityTrace({ id: traceId, title: "Quick Find activity", startedAtMs: requestedAt }),
            {
                id: "request",
                label: "Requesting an answer from local memory",
                actor: "FNDR answer service",
                status: "running",
                evidence: "ipc-boundary",
                atMs: requestedAt,
            },
        ));
        fndrAnswer(trimmed)
            .then((answer) => {
                if (actionSeq.current !== seq) return;
                setMode({ kind: "answer", answer });
                const finishedAt = Date.now();
                const result = answerActivityResult(answer);
                setActivityTrace((current) => {
                    if (current?.id !== traceId) return current;
                    const withCompletedRequest = recordActivityStep(current, {
                        id: "request",
                        label: "Local-memory answer request completed",
                        actor: "FNDR answer service",
                        status: "completed",
                        evidence: "ipc-boundary",
                        atMs: finishedAt,
                        durationMs: finishedAt - requestedAt,
                    });
                    return recordActivityStep(withCompletedRequest, {
                        id: "result",
                        label: result.label,
                        actor: "FNDR answer service",
                        status: result.status,
                        evidence: "result-metadata",
                        atMs: finishedAt,
                    });
                });
            })
            .catch(() => {
                if (actionSeq.current !== seq) return;
                setMode({ kind: "search" });
                setFeedback({
                    kind: "error",
                    message: "Couldn’t answer from your memory right now. Try again.",
                });
                const failedAt = Date.now();
                setActivityTrace((current) => current?.id === traceId
                    ? recordActivityStep(current, {
                        id: "request",
                        label: "Answer request failed",
                        actor: "FNDR answer service",
                        status: "failed",
                        evidence: "ipc-boundary",
                        atMs: failedAt,
                        durationMs: failedAt - requestedAt,
                    })
                    : current);
                window.requestAnimationFrame(() => inputRef.current?.focus());
            });
    }, [query]);

    const dismiss = useCallback(() => {
        actionSeq.current += 1;
        void dismissOmnibar()
            .then(reset)
            .catch(() => {
                setFeedback({
                    kind: "error",
                    message: "Couldn’t close Quick Find. Use Alt+Space to hide it.",
                });
            });
    }, [reset]);

    const toggleSurface = useCallback(() => {
        searchSeq.current += 1;
        actionSeq.current += 1;
        setSurface((s) => (s === "memory" ? "clipboard" : "memory"));
        setQuery("");
        setResults([]);
        setClips([]);
        setSelectedIndex(0);
        setMode({ kind: "search" });
        setCopiedId(null);
        setFeedback(null);
        setActivityTrace(null);
        inputRef.current?.focus();
    }, []);

    // A saved skill's name, or a request to do something, gets a first row
    // that hands it to Notch Do.
    const typingSearch = surface === "memory" && mode.kind === "search";
    const skill = typingSearch ? skillNamed(query, skills) : null;
    const doText = skill ? skill.name : typingSearch ? requestToDo(query) : null;
    const actionRows = doText ? 1 : 0;
    const files = useFileSearch(query, typingSearch, FILE_LIMIT);
    const filesState = useFilesStatus(typingSearch);
    const rows = fileRows(files.hits, files.strong, showWeaker);
    const fileStart = actionRows + results.length;
    const selectedRow = surface === "memory" && selectedIndex >= fileStart ? rows[selectedIndex - fileStart] : undefined;
    const selectedFile: FileHit | undefined = selectedRow?.type === "hit" ? selectedRow.hit : undefined;
    const shownHits = rows.flatMap((row) => (row.type === "hit" ? [row.hit] : []));
    const thumbnails = useThumbnails(shownHits.map((hit) => hit.path));
    const editorName = useEditorName(
        shownHits.some((hit) => hit.kind === "code"),
        filesState.settings,
    );
    const fileNote = filesNote(files.phase, filesState.status);
    const codeQuery = /^\s*code:/i.test(query);

    // A new search folds the weaker matches again and types into the field.
    useEffect(() => {
        setShowWeaker(false);
        setNavigated(false);
    }, [query]);

    const openFile = useCallback(
        (hit: FileHit) => {
            const seq = ++actionSeq.current;
            setFeedback(null);
            void openFileHit(hit, query)
                .then(() => dismissOmnibar())
                .then(() => {
                    if (actionSeq.current === seq) reset();
                })
                .catch(() => {
                    if (actionSeq.current === seq) {
                        setFeedback({ kind: "error", message: "Couldn’t open that file. Try again." });
                    }
                });
        },
        [query, reset],
    );

    // An open preview follows the selection, as Quick Look does.
    useEffect(() => {
        setPreviewHit((current) => (current && selectedFile && current.path !== selectedFile.path ? selectedFile : current));
    }, [selectedFile]);

    const fileAction = useCallback((action: Promise<void>, done: string, failed: string) => {
        const seq = ++actionSeq.current;
        void action
            .then(() => actionSeq.current === seq && setFeedback({ kind: "status", message: done }))
            .catch(() => actionSeq.current === seq && setFeedback({ kind: "error", message: failed }));
    }, []);

    const toggleQuickLook = useCallback((hit: FileHit | undefined) => {
        void filesQuickLook(quickLook ? undefined : hit?.path)
            .then(setQuickLook)
            .catch(() => setQuickLook(false));
    }, [quickLook]);

    // Quick Look shows the file that is selected, as it follows the arrows.
    useEffect(() => {
        if (quickLook && selectedFile) {
            void filesQuickLook(selectedFile.path).catch(() => setQuickLook(false));
        }
        // Only a new selection moves it; opening and closing go through toggleQuickLook.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [selectedFile?.path]);

    const openInWindow = useCallback(
        (kinds?: FileHit["kind"][]) => {
            void filesOpenInWindow(query.trim(), kinds)
                .then(reset)
                .catch(() => setFeedback({ kind: "error", message: "Couldn’t open FNDR’s window. Try again." }));
        },
        [query, reset],
    );

    const handleKeyDown = (event: React.KeyboardEvent) => {
        if (event.key === "Escape") {
            event.preventDefault();
            // A preview closes first, as Quick Look does; the next Esc closes Quick Find.
            if (quickLook) {
                toggleQuickLook(undefined);
            } else if (previewHit) {
                setPreviewHit(null);
            } else if (mode.kind === "answer" || mode.kind === "asking") {
                setMode({ kind: "search" });
                inputRef.current?.focus();
            } else {
                dismiss();
            }
            return;
        }
        if (event.key === "Tab") {
            event.preventDefault();
            toggleSurface();
            return;
        }
        if (mode.kind !== "search") {
            return;
        }
        const listLength =
            surface === "memory" ? results.length + actionRows + rows.length : clips.length;
        if (surface === "memory" && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
            event.preventDefault();
            if (query.trim()) openInWindow();
            return;
        }
        if (event.key === " " && navigated && selectedRow && !event.metaKey && !event.ctrlKey && !event.altKey) {
            event.preventDefault();
            if (selectedRow.type === "weaker") setShowWeaker((open) => !open);
            else if (selectedRow.type === "hit") toggleQuickLook(selectedRow.hit);
            return;
        }
        if (selectedFile && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "y") {
            event.preventDefault();
            setPreviewHit((current) => (current?.path === selectedFile.path ? null : selectedFile));
            return;
        }
        if (
            selectedFile &&
            (event.metaKey || event.ctrlKey) &&
            event.key.toLowerCase() === "c" &&
            inputRef.current?.selectionStart === inputRef.current?.selectionEnd
        ) {
            event.preventDefault();
            fileAction(copyFileHit(selectedFile), "Copied the file. Paste it anywhere.", "Couldn’t copy that file.");
            return;
        }
        if (event.key === "ArrowDown") {
            event.preventDefault();
            setNavigated(true);
            if (listLength > 0) {
                setSelectedIndex((i) => Math.min(i + 1, listLength - 1));
            }
        } else if (event.key === "ArrowUp") {
            event.preventDefault();
            setNavigated(true);
            setSelectedIndex((i) => Math.max(i - 1, 0));
        } else if (event.key === "Enter") {
            event.preventDefault();
            if (surface === "clipboard") {
                const clip = clips[selectedIndex];
                if (!clip) {
                    return;
                }
                if (event.metaKey || event.ctrlKey) {
                    pasteClip(clip);
                } else {
                    copyClip(clip);
                }
                return;
            }
            if (event.metaKey || event.ctrlKey) {
                ask();
            } else if (selectedRow?.type === "weaker") {
                setShowWeaker((open) => !open);
            } else if (selectedRow?.type === "more") {
                openInWindow([selectedRow.kind]);
            } else if (selectedFile && event.shiftKey) {
                fileAction(revealFileHit(selectedFile), "Shown in Finder.", "Couldn’t show that file in Finder.");
            } else if (selectedFile) {
                openFile(selectedFile);
            } else if (doText && selectedIndex === 0) {
                doIt(doText);
            } else if (results[selectedIndex - actionRows]) {
                openMemory(results[selectedIndex - actionRows].id);
            }
        }
    };

    return (
        <div
            className="omnibar"
            onKeyDown={handleKeyDown}
            role="dialog"
            aria-modal="false"
            aria-label="FNDR Quick Find"
        >
            <div className="omnibar-input-row">
                <span className="omnibar-glyph" aria-hidden>
                    ⌕
                </span>
                <input
                    ref={inputRef}
                    className="omnibar-input"
                    type="search"
                    aria-label={
                        surface === "memory"
                            ? "Search your memory"
                            : "Search clipboard history"
                    }
                    aria-controls={`omnibar-${surface}-results`}
                    aria-activedescendant={
                        mode.kind === "search" &&
                        (surface === "memory" ? results.length + actionRows + rows.length : clips.length)
                            ? `omnibar-${surface}-result-${selectedIndex}`
                            : undefined
                    }
                    aria-busy={searching}
                    value={query}
                    onChange={(e) => {
                        if (mode.kind === "answer") {
                            actionSeq.current += 1;
                            setMode({ kind: "search" });
                            setResults([]);
                            setSelectedIndex(0);
                        }
                        setQuery(e.target.value);
                        setFeedback(null);
                    }}
                    onMouseDown={() => setNavigated(false)}
                    placeholder={
                        surface === "memory"
                            ? "Search your memory…"
                            : "Search clipboard history…"
                    }
                    spellCheck={false}
                    autoComplete="off"
                    disabled={mode.kind === "asking"}
                />
                {(searching || files.searching) && <span className="omnibar-spinner" aria-hidden />}
                <button
                    type="button"
                    className="omnibar-surface-toggle"
                    onClick={toggleSurface}
                    tabIndex={-1}
                    aria-label={
                        surface === "memory"
                            ? "Search clipboard history"
                            : "Search your memory"
                    }
                >
                    <span data-active={surface === "memory"}>Memory</span>
                    <span data-active={surface === "clipboard"}>Clips</span>
                </button>
            </div>

            {activityTrace && (
                <ActivityTrace
                    trace={activityTrace}
                    className="omnibar-activity"
                    showDetails={false}
                />
            )}

            {surface === "memory" && mode.kind === "search" && (
                <div
                    id="omnibar-memory-results"
                    className="omnibar-results"
                    ref={listRef}
                    role="listbox"
                    aria-label="Memory matches"
                >
                    {doText && (
                        <button
                            id="omnibar-memory-result-0"
                            type="button"
                            role="option"
                            aria-selected={selectedIndex === 0}
                            data-selected={selectedIndex === 0}
                            className="omnibar-result omnibar-result-do"
                            onMouseEnter={() => setSelectedIndex(0)}
                            onClick={() => doIt(doText)}
                        >
                            <span className="omnibar-result-title">
                                {skill ? `Run skill: ${skill.name}` : `Do it: ${doText}`}
                            </span>
                            <span className="omnibar-result-meta">
                                {skill
                                    ? skill.steps.join(" · ")
                                    : "Notch Do plans it, and asks before anything that needs a yes"}
                            </span>
                        </button>
                    )}
                    {results.map((card, index) => (
                        <button
                            key={card.id}
                            id={`omnibar-memory-result-${index + actionRows}`}
                            type="button"
                            role="option"
                            aria-selected={index + actionRows === selectedIndex}
                            data-selected={index + actionRows === selectedIndex}
                            className="omnibar-result"
                            onMouseEnter={() => setSelectedIndex(index + actionRows)}
                            onClick={() => openMemory(card.id)}
                        >
                            <span className="omnibar-result-title">
                                {card.title || card.window_title}
                            </span>
                            <span className="omnibar-result-snippet">
                                {card.display_summary || card.summary}
                            </span>
                            <span className="omnibar-result-meta">
                                {card.app_name} ·{" "}
                                {formatTimestamp(card.timestamp)}
                            </span>
                        </button>
                    ))}
                    {(files.hits.length > 0 || (isFileQuery(query) && fileNote)) && (
                        <div className="omnibar-section" role="presentation">
                            <span>Files</span>
                            {(fileNote || files.hits.length > 0) && (
                                <span className="file-section-note">
                                    {fileNote ??
                                        `${files.strong} ${files.strong === 1 ? "result" : "results"}` +
                                            (files.hits.length > files.strong
                                                ? ` · ${files.hits.length - files.strong} weaker`
                                                : "")}
                                </span>
                            )}
                        </div>
                    )}
                    {rows.map((row, index) => {
                        const id = `omnibar-memory-result-${fileStart + index}`;
                        const selected = fileStart + index === selectedIndex;
                        const select = () => setSelectedIndex(fileStart + index);
                        if (row.type === "hit") {
                            return (
                                <div key={`${row.hit.path}-${row.hit.segment_id}`} role="presentation">
                                    {row.heading && (
                                        <div className="file-group-heading" role="presentation">
                                            {row.heading}
                                        </div>
                                    )}
                                    <FileResult
                                        id={id}
                                        hit={row.hit}
                                        className="omnibar-result"
                                        selected={selected}
                                        weaker={row.weaker}
                                        thumbnail={thumbnails[row.hit.path]}
                                        onSelect={select}
                                        onOpen={() => openFile(row.hit)}
                                    />
                                </div>
                            );
                        }
                        return (
                            <button
                                key={row.type === "more" ? `more-${row.kind}` : "weaker"}
                                id={id}
                                type="button"
                                role="option"
                                aria-selected={selected}
                                aria-expanded={row.type === "weaker" ? row.open : undefined}
                                data-selected={selected}
                                className={`omnibar-result ${row.type === "more" ? "file-row-more" : "file-row-weaker"}`}
                                onMouseEnter={select}
                                onClick={() =>
                                    row.type === "more" ? openInWindow([row.kind]) : setShowWeaker((open) => !open)
                                }
                            >
                                {row.type === "more"
                                    ? `Show all ${row.count} ${kindPlural(row.kind)}`
                                    : `${row.open ? "Hide" : "Show"} ${row.count} weaker ${row.count === 1 ? "match" : "matches"}`}
                            </button>
                        );
                    })}
                    {!results.length && !files.hits.length && query.trim() && !searching && !files.searching && !feedback && (
                        <div className="omnibar-empty">
                            {isFileQuery(query) ? fileEmpty(codeQuery, filesState.settings) : "No memory matches this search."}
                        </div>
                    )}
                    {!query.trim() && !searching && !feedback && (
                        <div className="omnibar-empty omnibar-empty-guidance">
                            Search the memories and files on this Mac, or press ⌘↵ to ask a question. Start with file: for files only.
                        </div>
                    )}
                </div>
            )}

            {surface === "clipboard" && (
                <div
                    id="omnibar-clipboard-results"
                    className="omnibar-results"
                    ref={listRef}
                    role="listbox"
                    aria-label="Clipboard history"
                >
                    {clips.map((clip, index) => (
                        <button
                            key={clip.id}
                            id={`omnibar-clipboard-result-${index}`}
                            type="button"
                            role="option"
                            aria-selected={index === selectedIndex}
                            data-selected={index === selectedIndex}
                            className="omnibar-result"
                            onMouseEnter={() => setSelectedIndex(index)}
                            onClick={() => copyClip(clip)}
                        >
                            <span className="omnibar-result-snippet omnibar-clip-text">
                                {clip.text}
                            </span>
                            <span className="omnibar-result-meta">
                                {copiedId === clip.id
                                    ? "Copied ✓"
                                    : [
                                          clip.app_name,
                                          clip.window_title,
                                          formatTimestamp(clip.timestamp),
                                      ]
                                          .filter(Boolean)
                                          .join(" · ")}
                            </span>
                        </button>
                    ))}
                    {!clips.length && !searching && !feedback && (
                        <div className="omnibar-empty">
                            {query.trim()
                                ? "No matching clips"
                                : "Nothing copied yet"}
                        </div>
                    )}
                </div>
            )}

            {surface === "memory" && mode.kind === "search" && previewHit && (
                <FilePreview hit={previewHit} onClose={() => setPreviewHit(null)} />
            )}

            {surface === "memory" && mode.kind === "asking" && (
                <div className="omnibar-answer omnibar-answer-loading" aria-hidden="true" />
            )}

            {surface === "memory" && mode.kind === "answer" && (
                <div className="omnibar-answer" role="region" aria-label="Answer from your memory">
                    <span className="omnibar-answer-label">Answer from your local memory</span>
                    <span className="omnibar-answer-grounding">
                        {answerGroundingLabel(mode.answer)}
                    </span>
                    <p className="omnibar-answer-text">{mode.answer.answer}</p>
                    {mode.answer.cards.length > 0 && (
                        <div className="omnibar-citations" aria-label="Supporting memories">
                            {mode.answer.cards.slice(0, 4).map((card) => (
                                <button
                                    key={card.id}
                                    type="button"
                                    className="omnibar-citation"
                                    onClick={() => openMemory(card.id)}
                                >
                                    <span className="omnibar-result-title">
                                        {card.title || card.window_title}
                                    </span>
                                    <span className="omnibar-result-meta">
                                        {card.app_name} ·{" "}
                                        {formatTimestamp(card.timestamp)}
                                    </span>
                                </button>
                            ))}
                        </div>
                    )}
                </div>
            )}

            {feedback && (
                <div
                    className={`omnibar-feedback ${feedback.kind}`}
                    role={feedback.kind === "error" ? "alert" : "status"}
                    aria-live={feedback.kind === "error" ? "assertive" : "polite"}
                >
                    {feedback.message}
                </div>
            )}

            <div className="omnibar-footer">
                <span>↹ {surface === "memory" ? "clips" : "memory"}</span>
                <span>↑↓ navigate</span>
                {surface === "memory" && selectedFile ? (
                    <>
                        <span>↵ {openTitle(selectedFile, editorName).toLowerCase()}</span>
                        <span>⇧↵ Finder</span>
                        <span>␣ Quick Look</span>
                        <span>⌘Y preview</span>
                        <span>⌘C copy file</span>
                        <span>⌘O window</span>
                    </>
                ) : surface === "memory" && selectedRow ? (
                    <>
                        <span>↵ {selectedRow.type === "more" ? "show all" : selectedRow.type === "weaker" && selectedRow.open ? "hide" : "show"}</span>
                        <span>⌘O window</span>
                    </>
                ) : surface === "memory" ? (
                    <>
                        <span>↵ open</span>
                        <span>⌘↵ ask</span>
                    </>
                ) : (
                    <>
                        <span>↵ copy</span>
                        <span>⌘↵ paste</span>
                    </>
                )}
                <span>esc {mode.kind === "search" ? "close" : "back"}</span>
            </div>
        </div>
    );
}

/** Why a files-only search found nothing, and what to do about it. */
function fileEmpty(code: boolean, settings: { enabled: boolean; folders: string[]; code_folders: string[] } | null): string {
    if (settings && !settings.enabled) return "Files search is off. Turn it on in Settings, AI.";
    if (code && settings && !settings.code_folders.length) {
        return "Code search is off. Add a code folder in Settings, AI.";
    }
    if (!code && settings && !settings.folders.length) {
        return "No folders chosen yet. Files search only looks in the folders you pick: choose them in Settings, AI.";
    }
    return code
        ? "No good matches in your code. Try saying what the code does, or a name that’s in it."
        : "No good matches. Try what you’d see or hear in it, or words you remember from it.";
}

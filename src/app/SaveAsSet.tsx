import { useEffect, useId, useRef, useState } from "react";
import { saveNamedSet } from "@/shared/ipc/tauri";
import type { WorkSetLoad } from "./WorkSetOpener";

interface SaveAsSetProps {
    /** The thread's title; also the suggested name. */
    name: string;
    /** The same lookup Open all uses, so the saved set is the one that would open. */
    load: WorkSetLoad;
    onSaved: () => void;
}

/** A refusal from the backend is written for people ("A set is already called …"); anything else is not shown. */
function refusal(error: unknown): string {
    return typeof error === "string" && error.length < 140 ? error : "The set could not be saved. Try again.";
}

/** "Save as set" on a thread: a name field, then the thread's work set saved under that name. */
export function SaveAsSet({ name, load, onSaved }: SaveAsSetProps) {
    const [open, setOpen] = useState(false);
    const [draft, setDraft] = useState(name);
    const [state, setState] = useState<{ kind: "idle" | "saving" } | { kind: "saved"; name: string } | { kind: "error"; message: string }>({ kind: "idle" });
    const fieldRef = useRef<HTMLInputElement>(null);
    const fieldId = useId();

    useEffect(() => {
        if (!open) return;
        fieldRef.current?.focus();
        fieldRef.current?.select();
    }, [open]);

    async function save() {
        const chosen = draft.trim();
        if (!chosen) {
            setState({ kind: "error", message: "Give the set a name." });
            return;
        }
        setState({ kind: "saving" });
        try {
            const found = await load();
            if (!Array.isArray(found) || found.length === 0) {
                setState({ kind: "error", message: Array.isArray(found) ? "Nothing saved to reopen for this yet." : found.why });
                return;
            }
            const saved = await saveNamedSet(chosen, found.map((item) => item.memoryId));
            setOpen(false);
            setState({ kind: "saved", name: saved.name });
            onSaved();
        } catch (error) {
            setState({ kind: "error", message: refusal(error) });
        }
    }

    if (!open) {
        return (
            <div className="save-set">
                <button
                    type="button"
                    aria-label={`Save ${name} as a set`}
                    onClick={() => {
                        setDraft(name);
                        setState({ kind: "idle" });
                        setOpen(true);
                    }}
                >
                    Save as set
                </button>
                {state.kind === "saved" && <span className="home-quiet" role="status">Saved as “{state.name}”</span>}
            </div>
        );
    }

    return (
        <form
            className="save-set save-set-form"
            onSubmit={(event) => {
                event.preventDefault();
                void save();
            }}
        >
            <label htmlFor={fieldId}>Set name</label>
            <input
                ref={fieldRef}
                id={fieldId}
                type="text"
                value={draft}
                maxLength={80}
                autoComplete="off"
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                    if (event.key === "Escape") setOpen(false);
                }}
            />
            <button type="submit" disabled={state.kind === "saving"}>Save</button>
            <button type="button" onClick={() => setOpen(false)}>Cancel</button>
            {state.kind === "error" && <span className="home-quiet" role="alert">{state.message}</span>}
        </form>
    );
}

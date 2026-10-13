import { useEffect, useId, useState } from "react";
import { getDictationSettings, setDictationSettings, type DictationSettings } from "@/shared/ipc/tauri";
import "./voice.css";

function errorText(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

function wordsFromText(text: string): string[] {
    return text.split("\n").map((word) => word.trim()).filter(Boolean);
}

/** Settings > Voice: hold a key, speak, and FNDR types into the field that has focus. */
export function DictationSection() {
    const ids = useId();
    const [settings, setSettings] = useState<DictationSettings | null>(null);
    const [shortcut, setShortcut] = useState("");
    const [words, setWords] = useState("");
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let live = true;
        void getDictationSettings()
            .then((next) => {
                if (!live) return;
                setSettings(next);
                setShortcut(next.shortcut);
                setWords(next.learned_words.join("\n"));
            })
            .catch((reason) => live && setError(errorText(reason)));
        return () => {
            live = false;
        };
    }, []);

    const save = async (patch: Partial<DictationSettings>) => {
        if (!settings) return;
        setError(null);
        try {
            const next = await setDictationSettings({ ...settings, ...patch });
            setSettings(next);
            setShortcut(next.shortcut);
            setWords(next.learned_words.join("\n"));
        } catch (reason) {
            setShortcut(settings.shortcut);
            setError(errorText(reason));
        }
    };

    return (
        <div className="fndr-voice-output" aria-label="Dictation">
            <div className="sg-setting-row">
                <span>
                    <strong>
                        <label htmlFor={`${ids}-enabled`}>Dictate into any app</label>
                    </strong>
                    <small>Hold the shortcut, speak, release. FNDR types the text where your cursor is, and fixes spellings with words from your screen.</small>
                </span>
                <input
                    id={`${ids}-enabled`}
                    type="checkbox"
                    role="switch"
                    checked={settings?.enabled ?? false}
                    disabled={!settings}
                    onChange={(event) => void save({ enabled: event.target.checked })}
                />
            </div>
            <div className="sg-setting-row">
                <span>
                    <strong>
                        <label htmlFor={`${ids}-shortcut`}>Hold-to-dictate shortcut</label>
                    </strong>
                    <small>For example Control+Alt+D.</small>
                </span>
                <input
                    id={`${ids}-shortcut`}
                    type="text"
                    value={shortcut}
                    disabled={!settings}
                    onChange={(event) => setShortcut(event.target.value)}
                    onBlur={() => {
                        const next = shortcut.trim();
                        if (next && next !== settings?.shortcut) void save({ shortcut: next });
                    }}
                />
            </div>
            <div className="sg-setting-row">
                <span>
                    <strong>
                        <label htmlFor={`${ids}-words`}>Words to spell your way</label>
                    </strong>
                    <small>One per line: names, products, jargon.</small>
                </span>
                <textarea
                    id={`${ids}-words`}
                    rows={3}
                    value={words}
                    disabled={!settings}
                    onChange={(event) => setWords(event.target.value)}
                    onBlur={() => {
                        const next = wordsFromText(words);
                        if (next.join("\n") !== (settings?.learned_words ?? []).join("\n")) void save({ learned_words: next });
                    }}
                />
            </div>
            {error && (
                <p className="fndr-voice-output__error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}

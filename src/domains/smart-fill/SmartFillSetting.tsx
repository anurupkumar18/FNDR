import { useEffect, useState } from "react";
import {
    forgetSavedDetails,
    getSavedDetailsCount,
    getSmartFillEnabled,
    setSmartFillEnabled,
} from "@/shared/ipc/tauri";
import "./SmartFillSetting.css";

/**
 * The Smart Fill switch. It lives with the ChatGPT sign-in because the text
 * is written on the person's own plan; the sentence says what leaves the Mac.
 */
export function SmartFillSetting() {
    const [enabled, setEnabled] = useState<boolean | null>(null);
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [savedCount, setSavedCount] = useState<number | null>(null);

    useEffect(() => {
        let active = true;
        getSmartFillEnabled()
            .then((loaded) => active && setEnabled(loaded))
            .catch(() => active && setError("FNDR could not read the Smart Fill setting."));
        getSavedDetailsCount()
            .then((count) => active && setSavedCount(count))
            .catch(() => undefined);
        return () => {
            active = false;
        };
    }, []);

    const forget = async () => {
        setError(null);
        try {
            setSavedCount(await forgetSavedDetails());
        } catch {
            setError("FNDR could not forget the saved details.");
        }
    };

    const toggle = async (next: boolean) => {
        if (enabled === null) return;
        setSaving(true);
        setError(null);
        try {
            setEnabled(await setSmartFillEnabled(next));
        } catch (reason) {
            setError(typeof reason === "string" ? reason : "FNDR could not save the Smart Fill setting.");
        } finally {
            setSaving(false);
        }
    };

    return (
        <section className="smart-fill-setting" aria-label="Smart Fill">
            <label className="smart-fill-row">
                <span>
                    <strong>Smart Fill</strong>
                    <small>
                        Press Command twice in any text box and FNDR writes the text for it, such as a reply to the
                        message on screen or the answer a form asks for. On a form it fills every field your memory states, sending the field names and up to 24 related memories (no screenshot). In any other text box each use sends a downscaled screenshot of your
                        main display, the text on it and up to four related memories to ChatGPT, on your plan. Esc
                        stops it. It never writes into password fields, apps on your blocklist or anything while
                        Private Mode is on. While it is on, FNDR also saves labelled details it sees on screen, such as "Phone: ...", in a private file on this Mac, so it can fill a whole form; ChatGPT then sees those details and your related memories. If a form has a file box, the names of your recent files (names only, nothing opened) are sent too, so it can suggest which one to attach. If macOS Dictation is set to start on Command twice, change one of them.
                    </small>
                </span>
                <input
                    type="checkbox"
                    role="switch"
                    className="fndr-switch"
                    aria-label="Smart Fill"
                    checked={enabled ?? false}
                    disabled={enabled === null || saving}
                    onChange={(event) => void toggle(event.target.checked)}
                />
            </label>
            {savedCount !== null && savedCount > 0 && (
                <p className="smart-fill-saved">
                    {savedCount} personal {savedCount === 1 ? "detail" : "details"} saved on this Mac.{" "}
                    <button type="button" className="smart-fill-forget" onClick={() => void forget()}>
                        Forget saved details
                    </button>
                </p>
            )}
            {error && (
                <p className="smart-fill-error" role="alert">
                    {error}
                </p>
            )}
        </section>
    );
}

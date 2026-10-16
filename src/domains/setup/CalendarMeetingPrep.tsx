import { useEffect, useState } from "react";
import {
    calendarMeetingPrepState,
    openCalendarPrivacySettings,
    setCalendarMeetingPrep,
    type CalendarPrepState,
    type CalendarStatus,
} from "@/shared/ipc/tauri";

const STATUS_LINE: Record<CalendarStatus, string> = {
    not_determined: "Turning this on asks macOS for calendar access.",
    granted: "Calendar access is allowed.",
    denied: "Calendar access is off for FNDR in System Settings.",
    restricted: "Calendar access is blocked on this Mac, for example by a profile.",
};

function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

/**
 * The switch for meeting prep from the calendar (ADR 029). Off by default
 * because it asks for a new permission; turning it on shows the macOS prompt.
 */
export function CalendarMeetingPrep() {
    const [prep, setPrep] = useState<CalendarPrepState | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        // A state read that cannot run must not take Settings down with it.
        Promise.resolve()
            .then(() => calendarMeetingPrepState())
            .then(setPrep)
            .catch((reason) => setError(message(reason)));
    }, []);

    const toggle = async () => {
        if (!prep) return;
        setBusy(true);
        setError(null);
        try {
            setPrep(await setCalendarMeetingPrep(!prep.enabled));
        } catch (reason) {
            setError(message(reason));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="setup-center-calendar">
            <label className="settings-switch-row">
                <span>Meeting prep from your calendar</span>
                <input
                    type="checkbox"
                    role="switch"
                    className="settings-switch"
                    aria-describedby="setup-calendar-hint"
                    checked={prep?.enabled === true}
                    disabled={prep === null || busy}
                    onChange={() => void toggle()}
                />
            </label>
            <p className="section-hint" id="setup-calendar-hint">
                A few minutes before a meeting, FNDR shows related work. Events are read on this Mac and never stored
                or sent anywhere. {prep ? STATUS_LINE[prep.status] : null}
            </p>
            {prep?.status === "denied" ? (
                <button type="button" className="ui-action-btn" onClick={() => void openCalendarPrivacySettings()}>
                    Open System Settings
                </button>
            ) : null}
            {error ? (
                <p className="sg-diagnostics-error" role="alert">
                    {error}
                </p>
            ) : null}
        </div>
    );
}

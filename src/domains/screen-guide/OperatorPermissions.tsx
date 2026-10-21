import { useCallback, useEffect, useState } from "react";
import { computerUsePermissions, type OperatorPermissions as Permissions } from "@/shared/ipc/tauri";
import { openSystemSettings } from "@/shared/ipc/onboarding";

type Pane = Parameters<typeof openSystemSettings>[0];

interface Row {
    label: string;
    why: string;
    ok: boolean | null;
    pane: Pane;
}

function rows(p: Permissions): Row[] {
    return [
        { label: "Accessibility", why: "Checks that each step landed (which app is in front, which page is open).", ok: p.accessibility, pane: "accessibility" },
        { label: "Screen Recording", why: "Lets Computer Use read the app it operates.", ok: p.screenRecording, pane: "screen-recording" },
        {
            label: "Automation",
            why: "Lets FNDR ask Spotify or Music what is playing, and drive Computer Use.",
            ok: p.automationMedia,
            pane: "automation",
        },
        {
            label: "Computer Use",
            why: p.backendDetail ?? "Opens the computer-use helper once to confirm it can see your apps.",
            ok: p.backendReady,
            pane: "automation",
        },
    ];
}

/** One-time walkthrough of the macOS permissions Notch Do needs, each checked live. */
export function OperatorPermissions() {
    const [permissions, setPermissions] = useState<Permissions | null>(null);
    const [checking, setChecking] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const check = useCallback(async (probe: boolean) => {
        setChecking(true);
        setError(null);
        try {
            setPermissions(await computerUsePermissions(probe));
        } catch (reason) {
            setError(reason instanceof Error ? reason.message : String(reason));
        } finally {
            setChecking(false);
        }
    }, []);

    useEffect(() => {
        void check(false);
    }, [check]);

    return (
        <section className="sg-operator-permissions" aria-label="Notch Do permissions">
            <ul>
                {permissions
                    ? rows(permissions).map((row) => (
                          <li key={row.label} className="sg-setting-row">
                              <span>
                                  <strong>{row.label}</strong>
                                  <small>{row.why}</small>
                              </span>
                              {/* The state is a word, not only a symbol, so it reads the same to everyone. */}
                              <div className="sg-permission-state">
                                  <em className={`is-${row.ok === true ? "allowed" : row.ok === false ? "denied" : "unknown"}`}>
                                      {row.ok === true ? "Allowed" : row.ok === false ? "Not allowed" : "Not checked"}
                                  </em>
                                  {row.ok !== true ? (
                                      <button
                                          type="button"
                                          className="ui-action-btn"
                                          aria-label={`Open ${row.label} settings`}
                                          onClick={() => void openSystemSettings(row.pane)}
                                      >
                                          Open Settings
                                      </button>
                                  ) : null}
                              </div>
                          </li>
                      ))
                    : null}
            </ul>
            {permissions && !permissions.backend ? (
                <p className="sg-operator-note" role="status">No computer-use helper is installed.</p>
            ) : null}
            {error ? <p className="sg-operator-note is-error" role="alert">{error}</p> : null}
            <button type="button" className="ui-action-btn" disabled={checking} onClick={() => void check(true)}>
                {checking ? "Checking…" : "Check again"}
            </button>
        </section>
    );
}

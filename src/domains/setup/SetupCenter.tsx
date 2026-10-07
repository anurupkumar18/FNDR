import { useCallback, useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
    checkHermesUpdate,
    codexLoginStart,
    installComponent,
    setupComponents,
    updateHermes,
    type HermesUpdateStatus,
    type SetupComponent,
} from "@/shared/ipc/tauri";
import { openExternalUrl } from "@/shared/utils/openExternalUrl";
import { OperatorPermissions } from "@/domains/screen-guide/OperatorPermissions";
import "./SetupCenter.css";

function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

const STATE_LABEL: Record<SetupComponent["state"], string> = {
    ready: "Ready",
    missing: "Not installed",
    error: "Needs attention",
};

/**
 * Everything FNDR depends on, with one action each, so a downloaded FNDR can
 * install and update the rest without a terminal: Codex and the ChatGPT
 * sign-in, Computer Use, Hermes, macOS permissions and FNDR itself.
 */
export function SetupCenter() {
    const [components, setComponents] = useState<SetupComponent[] | null>(null);
    const [busy, setBusy] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [hermesUpdate, setHermesUpdate] = useState<HermesUpdateStatus | null>(null);
    const [appUpdate, setAppUpdate] = useState<Update | null>(null);

    const refresh = useCallback(async () => {
        try {
            setComponents(await setupComponents());
        } catch (reason) {
            setError(message(reason));
        }
    }, []);

    useEffect(() => {
        void refresh();
        // An update check that cannot run (offline, no release yet) must not
        // take Settings down with it.
        Promise.resolve()
            .then(() => check())
            .then((update) => setAppUpdate(update ?? null))
            .catch(() => setAppUpdate(null));
    }, [refresh]);

    const act = async (id: string, work: () => Promise<unknown>) => {
        setBusy(id);
        setError(null);
        try {
            await work();
            await refresh();
        } catch (reason) {
            setError(message(reason));
        } finally {
            setBusy(null);
        }
    };

    const actionButton = (item: SetupComponent) => {
        switch (item.action) {
            case "install":
                return (
                    <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => void act(item.id, () => installComponent(item.id))}>
                        {busy === item.id ? "Installing…" : "Install"}
                    </button>
                );
            case "sign_in":
                return (
                    <button
                        type="button"
                        className="ui-action-btn"
                        disabled={busy !== null}
                        onClick={() => void act(item.id, async () => openExternalUrl((await codexLoginStart()).authUrl))}
                    >
                        Sign in
                    </button>
                );
            case "open_url":
                return item.url ? (
                    <button type="button" className="ui-action-btn" onClick={() => void openExternalUrl(item.url!)}>
                        Get it
                    </button>
                ) : null;
            default:
                return null;
        }
    };

    const hermesControls = (item: SetupComponent) => {
        if (item.id !== "hermes" || item.state !== "ready") return null;
        if (hermesUpdate?.update_available && hermesUpdate.latest) {
            return (
                <button type="button" className="ui-action-btn" disabled={busy !== null} onClick={() => void act("hermes", updateHermes)}>
                    {busy === "hermes" ? "Updating…" : `Update to ${hermesUpdate.latest}`}
                </button>
            );
        }
        return (
            <button
                type="button"
                className="ui-action-btn"
                disabled={busy !== null}
                onClick={() =>
                    void act("hermes-check", async () => {
                        const status = await checkHermesUpdate();
                        setHermesUpdate(status);
                        if (status.error) throw new Error(status.error);
                    })
                }
            >
                {hermesUpdate && !hermesUpdate.update_available ? "Up to date" : "Check for update"}
            </button>
        );
    };

    return (
        <section className="panel-section setup-center" aria-labelledby="setup-center-title">
            <h3 id="setup-center-title">Setup and updates</h3>
            <p className="section-hint">What FNDR needs, and installing or updating it from here.</p>

            <ul className="setup-center-list">
                <li className="setup-center-row" aria-label="FNDR">
                    <span>
                        <strong>FNDR</strong>
                        <small>{appUpdate ? `Version ${appUpdate.version} is available.` : "This app."}</small>
                    </span>
                    {appUpdate ? (
                        <button
                            type="button"
                            className="ui-action-btn"
                            disabled={busy !== null}
                            onClick={() =>
                                void act("fndr", async () => {
                                    await appUpdate.downloadAndInstall();
                                    await relaunch();
                                })
                            }
                        >
                            {busy === "fndr" ? "Updating…" : `Update FNDR to ${appUpdate.version}`}
                        </button>
                    ) : null}
                </li>
                {(components ?? []).map((item) => (
                    <li key={item.id} className="setup-center-row" aria-label={item.name}>
                        <span>
                            <strong>
                                {item.name}
                                {item.required ? "" : " (optional)"}
                            </strong>
                            <small>{item.purpose}</small>
                            {item.detail ? <small className="setup-center-detail">{item.detail}</small> : null}
                        </span>
                        <span className="setup-center-status">
                            <span className={`setup-center-state is-${item.state}`}>{STATE_LABEL[item.state]}</span>
                            {item.version ? <small>{item.version}</small> : null}
                            {actionButton(item)}
                            {hermesControls(item)}
                        </span>
                    </li>
                ))}
            </ul>

            <h4>macOS permissions</h4>
            <OperatorPermissions />

            {error ? (
                <p className="sg-diagnostics-error" role="alert">
                    {error}
                </p>
            ) : null}
        </section>
    );
}

/**
 * Settings > AI > Notch Do: the person's skills and recent runs. A skill is
 * a run that worked, saved under a name; it runs again from here, from the
 * command bar or by saying its name. A run can be rated here after the
 * notch has closed, so the learning loop does not depend on the notch's
 * card (docs/product/command-surface.md, "Learning from runs").
 */

import { useCallback, useEffect, useState } from "react";
import {
    notchDoDeleteSkill,
    notchDoRateRun,
    notchDoRequest,
    notchDoRuns,
    notchDoSaveSkill,
    notchDoSkills,
    type NotchDoRun,
    type NotchDoSkill,
} from "@/shared/ipc/tauri";
import "./NotchDoSettings.css";

function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

function when(at: string): string {
    const date = new Date(at);
    return Number.isNaN(date.getTime())
        ? ""
        : date.toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

const PLANNED_BY: Record<string, string> = {
    local: "Planned on this Mac",
    planner: "Planned by the cloud planner",
    skill: "A skill",
};

function outcome(run: NotchDoRun): string {
    if (run.ok === null) return "Did not end";
    return run.ok ? "Done" : "Did not finish";
}

export function NotchDoSettings() {
    const [skills, setSkills] = useState<NotchDoSkill[] | null>(null);
    const [runs, setRuns] = useState<NotchDoRun[] | null>(null);
    const [problem, setProblem] = useState<string | null>(null);
    const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
    const [names, setNames] = useState<Record<string, string>>({});
    const [savedFrom, setSavedFrom] = useState<Record<string, string>>({});

    const load = useCallback(async () => {
        const [skillsResult, runsResult] = await Promise.allSettled([notchDoSkills(), notchDoRuns()]);
        if (skillsResult.status === "fulfilled") setSkills(skillsResult.value);
        if (runsResult.status === "fulfilled") setRuns(runsResult.value);
        if (skillsResult.status === "rejected" || runsResult.status === "rejected") {
            setProblem("Notch Do's skills and runs could not be loaded.");
        }
    }, []);

    useEffect(() => {
        void load();
    }, [load]);

    const act = async (work: () => Promise<unknown>) => {
        setProblem(null);
        try {
            await work();
        } catch (reason) {
            setProblem(message(reason));
        }
    };

    const rate = (run: NotchDoRun, worked: boolean) =>
        act(async () => {
            await notchDoRateRun(run.runId, worked);
            setRuns((current) =>
                current?.map((each) =>
                    each.runId === run.runId
                        ? {
                              ...each,
                              rating: worked ? "worked" : "did_not_work",
                              repeatable: worked && each.repeatable,
                          }
                        : each,
                ) ?? current,
            );
        });

    const save = (run: NotchDoRun) =>
        act(async () => {
            const wanted = (names[run.runId] ?? "").trim();
            if (!wanted) return;
            const saved = await notchDoSaveSkill(run.runId, wanted);
            setSavedFrom((current) => ({ ...current, [run.runId]: saved }));
            setSkills(await notchDoSkills());
        });

    const remove = (skill: NotchDoSkill) =>
        act(async () => {
            await notchDoDeleteSkill(skill.id);
            setConfirmDelete(null);
            setSkills((current) => current?.filter((each) => each.id !== skill.id) ?? current);
        });

    return (
        <section className="panel-section settings-section--notch-do" aria-labelledby="settings-notch-do-title">
            <h3 id="settings-notch-do-title">Notch Do</h3>
            <p className="section-hint">
                A skill is a run that worked, saved under a name. Run it here, type its name in the command bar or say it to
                the notch; its steps run again without planning, and anything that needs a yes still waits for your tap.
            </p>

            <h4 className="notch-do-settings-heading">Skills</h4>
            {skills === null ? null : skills.length === 0 ? (
                <p className="notch-do-settings-empty">
                    No skills yet. Rate a run as worked, in the notch or below, then save it under a name.
                </p>
            ) : (
                <ul className="notch-do-settings-list" aria-label="Skills">
                    {skills.map((skill) => (
                        <li key={skill.id} className="notch-do-settings-item">
                            <div className="notch-do-settings-text">
                                <strong>{skill.name}</strong>
                                <span>{skill.steps.join(" · ")}</span>
                            </div>
                            <div className="notch-do-settings-actions">
                                {confirmDelete === skill.id ? (
                                    <>
                                        <span className="notch-do-settings-note">Delete it?</span>
                                        <button type="button" className="ui-action-btn" onClick={() => void remove(skill)}>
                                            Delete
                                        </button>
                                        <button type="button" className="ui-action-btn" onClick={() => setConfirmDelete(null)}>
                                            Keep
                                        </button>
                                    </>
                                ) : (
                                    <>
                                        <button
                                            type="button"
                                            className="ui-action-btn"
                                            onClick={() => void act(() => notchDoRequest(skill.name))}
                                        >
                                            Run
                                        </button>
                                        <button
                                            type="button"
                                            className="ui-action-btn"
                                            aria-label={`Delete ${skill.name}`}
                                            onClick={() => setConfirmDelete(skill.id)}
                                        >
                                            Delete
                                        </button>
                                    </>
                                )}
                            </div>
                        </li>
                    ))}
                </ul>
            )}

            <h4 className="notch-do-settings-heading">Recent runs</h4>
            {runs === null ? null : runs.length === 0 ? (
                <p className="notch-do-settings-empty">No runs yet.</p>
            ) : (
                <ul className="notch-do-settings-list" aria-label="Recent runs">
                    {runs.map((run) => (
                        <li key={run.runId} className="notch-do-settings-item notch-do-settings-run">
                            <div className="notch-do-settings-text">
                                <strong>&ldquo;{run.request}&rdquo;</strong>
                                <span>
                                    {[when(run.at), PLANNED_BY[run.plannedBy] ?? run.plannedBy, outcome(run)]
                                        .filter(Boolean)
                                        .join(" · ")}
                                </span>
                                {run.steps.length > 0 ? <span>{run.steps.join(" · ")}</span> : null}
                            </div>
                            <RunActions
                                run={run}
                                name={names[run.runId] ?? ""}
                                saved={savedFrom[run.runId]}
                                onName={(name) => setNames((current) => ({ ...current, [run.runId]: name }))}
                                onRate={(worked) => void rate(run, worked)}
                                onSave={() => void save(run)}
                            />
                        </li>
                    ))}
                </ul>
            )}
            {problem ? (
                <p className="settings-message settings-message--error" role="alert">
                    {problem}
                </p>
            ) : null}
        </section>
    );
}

function RunActions({
    run,
    name,
    saved,
    onName,
    onRate,
    onSave,
}: {
    run: NotchDoRun;
    name: string;
    saved: string | undefined;
    onName: (name: string) => void;
    onRate: (worked: boolean) => void;
    onSave: () => void;
}) {
    if (run.ok === null) return null;
    if (run.rating === null) {
        return (
            <div className="notch-do-settings-actions" aria-label={`Rate “${run.request}”`}>
                <button type="button" className="ui-action-btn" onClick={() => onRate(true)}>
                    Worked
                </button>
                <button type="button" className="ui-action-btn" onClick={() => onRate(false)}>
                    Didn&apos;t work
                </button>
            </div>
        );
    }
    if (run.rating === "did_not_work") {
        return <p className="notch-do-settings-note">Kept as a test case: the request and its plan, no screen text.</p>;
    }
    if (saved) return <p className="notch-do-settings-note">Saved as &ldquo;{saved}&rdquo;.</p>;
    if (!run.repeatable || run.plannedBy === "skill") return null;
    return (
        <form
            className="notch-do-settings-save"
            onSubmit={(event) => {
                event.preventDefault();
                onSave();
            }}
        >
            <input
                value={name}
                placeholder="Save as a skill, e.g. Monday setup"
                aria-label={`Skill name for “${run.request}”`}
                maxLength={60}
                onChange={(event) => onName(event.target.value)}
            />
            <button type="submit" className="ui-action-btn" disabled={!name.trim()}>
                Save
            </button>
        </form>
    );
}

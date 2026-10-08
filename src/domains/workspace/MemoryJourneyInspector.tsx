import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";

type JourneyMode = "live" | "reconstructed";
type JourneyState = "armed" | "capturing" | "stored" | "querying" | "complete" | "failed" | "expired";
type StageStatus = "observed" | "persisted" | "skipped" | "failed" | "unavailable";

interface JourneyStage {
    name: string;
    status: StageStatus;
    observed_at_ms: number;
    duration_ms: number | null;
    outcome: string;
    details: unknown;
    artifact_ids: string[];
}

interface JourneyArtifact {
    id: string;
    stage: string;
    relative_path: string;
    sha256: string;
    size_bytes: number;
    available: boolean;
}

interface QueryRun {
    id: string;
    path: "search" | "ask";
    kind: "exact" | "paraphrase" | "grounded" | "unsupported";
    query: string;
    duration_ms: number;
    result_ids: string[];
    citation_ids: string[];
    refusal: boolean | null;
}

interface JourneyManifest {
    schema_version: number;
    journey_id: string;
    label: string;
    mode: JourneyMode;
    state: JourneyState;
    created_at_ms: number;
    updated_at_ms: number;
    memory_id: string | null;
    stages: JourneyStage[];
    artifacts: JourneyArtifact[];
    query_runs: QueryRun[];
    pipeline_integrity: Record<string, string | number | boolean | null>;
    human_usefulness: Record<string, string | number | boolean | null>;
    agent_grounding: Record<string, string | number | boolean | null>;
}

interface JourneySummary {
    journey_id: string;
    label: string;
    mode: JourneyMode;
    state: JourneyState;
    memory_id: string | null;
    stage_count: number;
    artifact_count: number;
    query_count: number;
    size_bytes: number;
}

interface JourneyStatus {
    armed: boolean;
    arm_ready_at_ms: number | null;
    handoff_grace_ms: number;
    active_journey_id: string | null;
    active_state: JourneyState | null;
    journeys: JourneySummary[];
    manifests: JourneyManifest[];
    total_bytes: number;
    max_bundles: number;
    max_age_ms: number;
    max_total_bytes: number;
}

interface QueryReceipt {
    manifest: JourneyManifest;
    cards: Array<{ id: string }>;
    answer: string | null;
}

interface QualityLabFixture {
    id: string;
    app_class: string;
    file_name: string;
    expected_text: string;
    cer_budget: number;
    evaluation: QualityLabEvaluationCase | null;
}

interface QualityLabEvaluationCase {
    fixture_id: string;
    required_facts: string[];
    exact_query: string;
    paraphrase_query: string;
    grounded_question: string;
    unsupported_question: string;
}

interface QualityLabFixtureImport {
    fixture_id: string;
    memory_id: string;
}

interface QualityLabCheckResult {
    kind: QueryRun["kind"];
    path: QueryRun["path"];
    query: string;
    target_rank: number | null;
    target_cited: boolean;
    refusal: boolean | null;
    answer: string | null;
    passed: boolean;
}

interface QualityLabBatchResult {
    fixtureId: string;
    checks: QualityLabCheckResult[];
    error: string | null;
}

interface QualityLabBatchProgress {
    total: number;
    completed: number;
    activeFixtureId: string | null;
    results: QualityLabBatchResult[];
}

const EVENT = "memory-journey://status";

function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function conciseDetails(details: unknown): string | null {
    if (!details || typeof details !== "object") return null;
    const entries = Object.entries(details as Record<string, unknown>)
        .filter(([, value]) => typeof value === "string" || typeof value === "number" || typeof value === "boolean")
        .slice(0, 4)
        .map(([key, value]) => `${key.split("_").join(" ")}: ${String(value)}`);
    return entries.length ? entries.join(" · ") : null;
}

function scorecardProgress(scorecard: Record<string, unknown>): string {
    const values = Object.entries(scorecard).filter(([key]) => key !== "notes");
    const scored = values.filter(([, value]) => value !== null && value !== undefined).length;
    return `${scored}/${values.length} evidence-backed fields`;
}

export default function MemoryJourneyInspector() {
    const [status, setStatus] = useState<JourneyStatus | null>(null);
    const [selectedId, setSelectedId] = useState("");
    const [label, setLabel] = useState("");
    const [memoryId, setMemoryId] = useState("");
    const [query, setQuery] = useState("");
    const [queryKind, setQueryKind] = useState<QueryRun["kind"]>("exact");
    const [busy, setBusy] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [lastAnswer, setLastAnswer] = useState<string | null>(null);
    const [fixtures, setFixtures] = useState<QualityLabFixture[]>([]);
    const [fixtureId, setFixtureId] = useState("");
    const [fixtureError, setFixtureError] = useState<string | null>(null);
    const [fixtureChecks, setFixtureChecks] = useState<QualityLabCheckResult[]>([]);
    const [fixtureBatch, setFixtureBatch] = useState<QualityLabBatchProgress | null>(null);

    useTauriEvent<JourneyStatus>(EVENT, (next) => {
        setStatus(next);
        if (!selectedId && next.manifests[0]) setSelectedId(next.manifests[0].journey_id);
    });

    useEffect(() => {
        let active = true;
        void invoke<JourneyStatus>("get_memory_journey_status")
            .then((next) => {
                if (!active) return;
                setStatus(next);
                setSelectedId((current) => current || next.manifests[0]?.journey_id || "");
            })
            .catch((reason: unknown) => {
                if (active) setError(String(reason));
            });
        return () => {
            active = false;
        };
    }, []);

    useEffect(() => {
        let active = true;
        void invoke<QualityLabFixture[]>("get_quality_lab_fixtures")
            .then((next) => {
                if (!active) return;
                const available = Array.isArray(next) ? next : [];
                setFixtures(available);
                setFixtureId((current) => current || available[0]?.id || "");
            })
            .catch((reason: unknown) => {
                if (active) setFixtureError(String(reason));
            });
        return () => {
            active = false;
        };
    }, []);

    const selected = useMemo(
        () => status?.manifests.find((manifest) => manifest.journey_id === selectedId) ?? status?.manifests[0] ?? null,
        [selectedId, status],
    );

    const run = async (name: string, operation: () => Promise<void>) => {
        setBusy(name);
        setError(null);
        try {
            await operation();
        } catch (reason) {
            setError(String(reason));
        } finally {
            setBusy(null);
        }
    };

    const arm = () => run("arm", async () => {
        const next = await invoke<JourneyStatus>("arm_memory_journey", { label });
        setStatus(next);
        setSelectedId(next.active_journey_id ?? "");
    });

    const reconstruct = () => run("reconstruct", async () => {
        const manifest = await invoke<JourneyManifest>("create_reconstructed_memory_journey", { memoryId });
        const next = await invoke<JourneyStatus>("get_memory_journey_status");
        setStatus(next);
        setSelectedId(manifest.journey_id);
    });

    const replayAndEvaluateFixture = async (
        fixture: QualityLabFixture,
        onChecks: (checks: QualityLabCheckResult[]) => void,
    ): Promise<QualityLabCheckResult[]> => {
        const imported = await invoke<QualityLabFixtureImport>("replay_quality_lab_fixture", { fixtureId: fixture.id });
        setMemoryId(imported.memory_id);
        const manifest = await invoke<JourneyManifest>("create_reconstructed_memory_journey", {
            memoryId: imported.memory_id,
        });
        const next = await invoke<JourneyStatus>("get_memory_journey_status");
        setStatus(next);
        setSelectedId(manifest.journey_id);
        const evaluation = fixture.evaluation;
        if (!evaluation) return [];
        const checks: Array<{ kind: QueryRun["kind"]; path: QueryRun["path"]; query: string }> = [
            { kind: "exact", path: "search", query: evaluation.exact_query },
            { kind: "paraphrase", path: "search", query: evaluation.paraphrase_query },
            { kind: "grounded", path: "ask", query: evaluation.grounded_question },
            { kind: "unsupported", path: "ask", query: evaluation.unsupported_question },
        ];
        const results: QualityLabCheckResult[] = [];
        for (const check of checks) {
            const receipt = await invoke<QueryReceipt>("run_memory_journey_query", {
                journeyId: manifest.journey_id,
                query: check.query,
                path: check.path,
                queryKind: check.kind,
            });
            const queryRuns = receipt.manifest.query_runs;
            const queryRun = queryRuns[queryRuns.length - 1];
            const nextResult: QualityLabCheckResult = {
                ...check,
                target_rank: queryRun?.result_ids.includes(imported.memory_id)
                    ? queryRun.result_ids.indexOf(imported.memory_id) + 1
                    : null,
                target_cited: queryRun?.citation_ids.includes(imported.memory_id) ?? false,
                refusal: queryRun?.refusal ?? null,
                answer: receipt.answer,
                passed: check.kind === "exact" || check.kind === "paraphrase"
                    ? queryRun?.result_ids.includes(imported.memory_id) === true
                        && queryRun.result_ids.indexOf(imported.memory_id) < 5
                    : check.kind === "grounded"
                      ? queryRun?.citation_ids.includes(imported.memory_id) === true
                      : queryRun?.refusal === true,
            };
            results.push(nextResult);
            onChecks([...results]);
            if (receipt.answer) setLastAnswer(receipt.answer);
        }
        const completed = await invoke<JourneyStatus>("get_memory_journey_status");
        setStatus(completed);
        setSelectedId(manifest.journey_id);
        return results;
    };

    const replayFixture = () => run("fixture", async () => {
        if (!fixtureId) throw new Error("Choose a synthetic fixture first");
        const fixture = fixtures.find((item) => item.id === fixtureId);
        if (!fixture) throw new Error("The selected fixture is no longer available");
        setFixtureChecks([]);
        setFixtureBatch(null);
        await replayAndEvaluateFixture(fixture, setFixtureChecks);
    });

    const replayAllGoldFixtures = () => run("fixture-batch", async () => {
        const goldFixtures = fixtures.filter((fixture) => fixture.evaluation !== null);
        if (!goldFixtures.length) throw new Error("No positive fixtures have gold evaluation cases");
        setFixtureChecks([]);
        const progress: QualityLabBatchProgress = {
            total: goldFixtures.length,
            completed: 0,
            activeFixtureId: goldFixtures[0].id,
            results: [],
        };
        setFixtureBatch(progress);
        for (const fixture of goldFixtures) {
            setFixtureBatch((current) => current
                ? { ...current, activeFixtureId: fixture.id }
                : current);
            let checks: QualityLabCheckResult[] = [];
            let errorMessage: string | null = null;
            try {
                checks = await replayAndEvaluateFixture(fixture, (nextChecks) => {
                    checks = nextChecks;
                    if (fixture.id === fixtureId) setFixtureChecks(nextChecks);
                });
            } catch (reason) {
                errorMessage = String(reason);
                setError(`Fixture ${fixture.id} failed: ${errorMessage}`);
            }
            progress.completed += 1;
            progress.results.push({ fixtureId: fixture.id, checks, error: errorMessage });
            setFixtureBatch({
                total: progress.total,
                completed: progress.completed,
                activeFixtureId: progress.completed < progress.total ? goldFixtures[progress.completed].id : null,
                results: [...progress.results],
            });
        }
        const finalStatus = await invoke<JourneyStatus>("get_memory_journey_status");
        setStatus(finalStatus);
    });

    const runQuery = (path: "search" | "ask") => run(path, async () => {
        if (!selected) throw new Error("Select or record a journey first");
        const receipt = await invoke<QueryReceipt>("run_memory_journey_query", {
            journeyId: selected.journey_id,
            query,
            path,
            queryKind,
        });
        const next = await invoke<JourneyStatus>("get_memory_journey_status");
        setStatus(next);
        setSelectedId(receipt.manifest.journey_id);
        setLastAnswer(receipt.answer);
    });

    const remove = (journeyId: string) => run("delete", async () => {
        const next = await invoke<JourneyStatus>("delete_memory_journey", { journeyId });
        setStatus(next);
        setSelectedId(next.manifests[0]?.journey_id ?? "");
    });

    const removeAll = () => run("delete-all", async () => {
        const next = await invoke<JourneyStatus>("delete_all_memory_journeys");
        setStatus(next);
        setSelectedId("");
    });

    const exportSelected = () => run("export", async () => {
        if (!selected) throw new Error("Select a completed journey first");
        await invoke("export_memory_journey", { journeyId: selected.journey_id });
    });

    const selectedFixture = fixtures.find((fixture) => fixture.id === fixtureId) ?? null;
    const fixtureBatchSummary = fixtureBatch
        ? (() => {
            const checksObserved = fixtureBatch.results.reduce((sum, result) => sum + result.checks.length, 0);
            const checksPassed = fixtureBatch.results.reduce((sum, result) => sum + result.checks.filter((check) => check.passed).length, 0);
            return {
                checksPassed,
                checksNeedReview: checksObserved - checksPassed,
                checksNotRun: fixtureBatch.total * 4 - checksObserved,
                fixtureErrors: fixtureBatch.results.filter((result) => result.error).length,
            };
        })()
        : null;

    return (
        <section className="pipeline-panel-card memory-journey" aria-labelledby="memory-journey-title">
            <div className="memory-journey__heading">
                <div>
                    <span className="pipeline-header-kicker">Debug builds only · private local evidence</span>
                    <h3 id="memory-journey-title">Memory Journey</h3>
                    <p className="pipeline-muted">
                        Inspect a live capture, a synthetic image import, or one persisted memory. Stages without durable run evidence stay marked unavailable or persisted.
                    </p>
                    <p className="pipeline-muted">
                        Live capture: open the target first, return here to arm, then switch straight back during the handoff. Synthetic fixture imports use the separate image-import path below.
                    </p>
                </div>
                <span className={`memory-journey__state memory-journey__state--${status?.active_state ?? "idle"}`} role="status" aria-live="polite">
                    {status?.active_state?.split("_").join(" ") ?? "idle"}
                </span>
            </div>

            {error && <p className="pipeline-error" role="alert">{error}</p>}

            <section className="pipeline-panel-card memory-journey__fixture" aria-labelledby="quality-lab-fixture-title">
                <h4 id="quality-lab-fixture-title">Synthetic fixture replay</h4>
                <div className="memory-journey__fixture-controls">
                    <label>
                        Positive screen image
                        <select value={fixtureId} onChange={(event) => setFixtureId(event.target.value)} disabled={!fixtures.length}>
                            {fixtures.map((fixture) => (
                                <option key={fixture.id} value={fixture.id}>
                                    {fixture.app_class} · {fixture.file_name}
                                </option>
                            ))}
                        </select>
                    </label>
                    <button
                        type="button"
                        className="ui-action-btn"
                        disabled={Boolean(busy) || !fixtureId}
                        onClick={replayFixture}
                    >
                        {busy === "fixture"
                            ? "Importing and evaluating…"
                            : selectedFixture?.evaluation
                              ? "Import + run 4 checks"
                              : "Import synthetic fixture"}
                    </button>
                    <button
                        type="button"
                        className="ui-action-btn"
                        disabled={Boolean(busy) || !fixtures.some((fixture) => fixture.evaluation !== null)}
                        onClick={replayAllGoldFixtures}
                    >
                        {busy === "fixture-batch"
                            ? `Running gold set ${fixtureBatch?.completed ?? 0}/${fixtureBatch?.total ?? 0}…`
                            : `Run all ${fixtures.filter((fixture) => fixture.evaluation !== null).length} gold cases`}
                    </button>
                </div>
                <p className="pipeline-muted">
                    Runs one positive fixture through FNDR’s existing image import, OCR, extraction/fallback, embedding, and storage path,
                    then opens its persisted evidence in this inspector. Labeled cases also run exact and paraphrase Search plus grounded and unsupported Ask checks serially.
                    This writes only in a marked Quality Lab profile; live screen-capture admission and privacy blocking are not tested. Privacy-negative fixtures are excluded.
                </p>
                {fixtureError && <p className="pipeline-muted" role="status">Fixture replay unavailable: {fixtureError}</p>}
                {fixtureBatch && (
                    <div role="status" aria-live="polite">
                        <p className="pipeline-muted">
                            Gold set {fixtureBatch.completed}/{fixtureBatch.total} fixtures finished
                            {fixtureBatch.activeFixtureId ? ` · running ${fixtureBatch.activeFixtureId}` : " · complete"}.
                            {" "}{fixtureBatchSummary?.checksPassed} checks passed; {fixtureBatchSummary?.checksNeedReview} checks need review; {fixtureBatchSummary?.checksNotRun} checks not run;
                            {" "}{fixtureBatchSummary?.fixtureErrors} fixture errors.
                            Imports are serial and persist in this Quality Lab profile; journey evidence follows the existing six-bundle retention limit.
                        </p>
                        <ul className="pipeline-muted" aria-label="Gold case batch results">
                            {fixtureBatch.results.map((result) => (
                                <li key={result.fixtureId}>
                                    {result.fixtureId}: {result.error ? `failed · ${result.error}` : `${result.checks.filter((check) => check.passed).length}/${result.checks.length} checks passed`}
                                </li>
                            ))}
                        </ul>
                    </div>
                )}
                {selectedFixture && (
                    <details>
                        <summary>Expected OCR text · CER budget {selectedFixture.cer_budget}</summary>
                        <pre>{selectedFixture.expected_text}</pre>
                    </details>
                )}
                {selectedFixture?.evaluation && (
                    <details>
                        <summary>Gold review case · required facts and questions</summary>
                        <ul>
                            {selectedFixture.evaluation.required_facts.map((fact) => <li key={fact}>{fact}</li>)}
                        </ul>
                        <p>Exact: {selectedFixture.evaluation.exact_query}</p>
                        <p>Paraphrase: {selectedFixture.evaluation.paraphrase_query}</p>
                        <p>Grounded Ask: {selectedFixture.evaluation.grounded_question}</p>
                        <p>Unsupported Ask: {selectedFixture.evaluation.unsupported_question}</p>
                    </details>
                )}
                {fixtureChecks.length > 0 && (
                    <>
                        <p className="pipeline-muted" role="status">
                            Checks run {fixtureChecks.length}/4 · {fixtureChecks.filter((check) => check.passed).length} passed · {4 - fixtureChecks.length} not run.
                            Exact/paraphrase require target rank ≤5; grounded requires the target citation; unsupported requires refusal.
                            Review required facts and generated wording separately.
                        </p>
                        <ol className="memory-journey__runs memory-journey__runs--fixture" aria-label="Synthetic fixture evaluation results">
                            {fixtureChecks.map((check) => (
                                <li key={`${check.kind}:${check.query}`}>
                                    <strong>{check.kind} · {check.path}</strong>
                                    <span>{check.query}</span>
                                    <span>
                                        {check.target_rank == null ? "Target memory not returned" : `Target memory rank ${check.target_rank}`}
                                        {check.kind === "grounded" ? (check.target_cited ? " · target cited" : " · target not cited") : ""}
                                        {check.kind === "unsupported" ? (check.refusal ? " · refused" : " · answered") : ""}
                                        {` · ${check.passed ? "Pass" : "Review"}`}
                                    </span>
                                    {check.answer && <small>Answer: {check.answer}</small>}
                                </li>
                            ))}
                        </ol>
                    </>
                )}
            </section>

            <div className="memory-journey__controls">
                <label>
                    Optional case label
                    <input value={label} maxLength={80} onChange={(event) => setLabel(event.target.value)} placeholder="Case 1 · research article" />
                </label>
                <button type="button" className="ui-action-btn" disabled={Boolean(busy) || Boolean(status?.active_state)} onClick={arm}>
                    {busy === "arm" ? "Arming…" : "Record next capture"}
                </button>
                <label>
                    Existing memory ID
                    <input value={memoryId} onChange={(event) => setMemoryId(event.target.value)} placeholder="Paste one explicit memory ID" />
                </label>
                <button type="button" className="ui-action-btn" disabled={Boolean(busy) || !memoryId.trim()} onClick={reconstruct}>
                    {busy === "reconstruct" ? "Reconstructing…" : "Inspect existing memory"}
                </button>
            </div>

            {status?.armed && (
                <p className="pipeline-muted" aria-live="polite">
                    Armed with a {Math.round(status.handoff_grace_ms / 1000)}-second handoff. Switch now to the already-open target; FNDR will ignore capture attempts until the handoff ends.
                </p>
            )}

            <div className="memory-journey__retention" aria-label="Memory Journey retention">
                <span>{status?.journeys.length ?? 0}/{status?.max_bundles ?? 6} bundles</span>
                <span>{formatBytes(status?.total_bytes ?? 0)} / {formatBytes(status?.max_total_bytes ?? 128 * 1024 * 1024)}</span>
                <span>24-hour retention</span>
            </div>

            {status?.journeys.length ? (
                <label className="memory-journey__selector">
                    Journey
                    <select value={selected?.journey_id ?? ""} onChange={(event) => setSelectedId(event.target.value)}>
                        {status.journeys.map((journey) => (
                            <option key={journey.journey_id} value={journey.journey_id}>
                                {journey.label || journey.journey_id.slice(0, 8)} · {journey.mode} · {journey.state}
                            </option>
                        ))}
                    </select>
                </label>
            ) : (
                <p className="pipeline-muted">No journey bundles yet. Arming is consumed by the next capture attempt, including a policy skip.</p>
            )}

            {selected && (
                <>
                    <ol className="memory-journey__stages" aria-label="Observed Memory Journey stages">
                        {selected.stages.map((stage) => (
                            <li key={stage.name} data-status={stage.status}>
                                <span className="memory-journey__stage-dot" aria-hidden="true" />
                                <div>
                                    <strong>{stage.name.split("_").join(" ")}</strong>
                                    <span>{stage.status} · {stage.outcome}{stage.duration_ms == null ? "" : ` · ${stage.duration_ms} ms`}</span>
                                    {conciseDetails(stage.details) && <small>{conciseDetails(stage.details)}</small>}
                                </div>
                                <span>{stage.artifact_ids.length ? `${stage.artifact_ids.length} artifact${stage.artifact_ids.length === 1 ? "" : "s"}` : "no artifact"}</span>
                            </li>
                        ))}
                    </ol>

                    {selected.artifacts.length > 0 && (
                        <details className="memory-journey__artifacts">
                            <summary>Recorded artifacts ({selected.artifacts.length})</summary>
                            <ul aria-label="Memory Journey artifacts">
                                {selected.artifacts.map((artifact) => (
                                    <li key={artifact.id}>
                                        <span>
                                            <strong>{artifact.relative_path}</strong>
                                            <small>{artifact.stage} · {formatBytes(artifact.size_bytes)} · sha256 {artifact.sha256.slice(0, 8)}</small>
                                        </span>
                                        <span>{artifact.available ? "available in export" : "unavailable"}</span>
                                    </li>
                                ))}
                            </ul>
                        </details>
                    )}

                    <div className="memory-journey__query">
                        <label>
                            Exact query, paraphrase, or grounded question
                            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Ask what this memory proves" />
                        </label>
                        <label>
                            Case
                            <select value={queryKind} onChange={(event) => setQueryKind(event.target.value as QueryRun["kind"])}>
                                <option value="exact">Exact</option>
                                <option value="paraphrase">Paraphrase</option>
                                <option value="grounded">Grounded question</option>
                                <option value="unsupported">Unsupported question</option>
                            </select>
                        </label>
                        <button type="button" className="ui-action-btn" disabled={Boolean(busy) || !query.trim()} onClick={() => runQuery("search")}>Run Search</button>
                        <button type="button" className="ui-action-btn" disabled={Boolean(busy) || !query.trim()} onClick={() => runQuery("ask")}>Run Ask</button>
                    </div>

                    {selected.query_runs.length > 0 && (
                        <ul className="memory-journey__runs" aria-label="Production query runs">
                            {selected.query_runs.map((runItem) => (
                                <li key={runItem.id}>
                                    <strong>{runItem.kind} · {runItem.path}</strong>
                                    <span>{runItem.duration_ms} ms · {runItem.result_ids.length} results · {runItem.citation_ids.length} citations{runItem.refusal == null ? "" : runItem.refusal ? " · refused" : " · answered"}</span>
                                </li>
                            ))}
                        </ul>
                    )}
                    <div className="memory-journey__scorecards" aria-label="Separate quality scorecards">
                        <article>
                            <strong>Pipeline integrity</strong>
                            <span>{scorecardProgress(selected.pipeline_integrity ?? {})}</span>
                        </article>
                        <article>
                            <strong>Human usefulness</strong>
                            <span>{scorecardProgress(selected.human_usefulness ?? {})}</span>
                        </article>
                        <article>
                            <strong>Agent grounding</strong>
                            <span>{scorecardProgress(selected.agent_grounding ?? {})}</span>
                        </article>
                    </div>
                    {lastAnswer && <p className="memory-journey__answer"><strong>Ask result:</strong> {lastAnswer}</p>}

                    <div className="memory-journey__actions">
                        <button type="button" className="ui-action-btn" disabled={Boolean(busy)} onClick={exportSelected}>Export .fndrjourney.zip</button>
                        <button type="button" className="ui-action-btn" disabled={Boolean(busy)} onClick={() => remove(selected.journey_id)}>Delete selected</button>
                        <button type="button" className="ui-action-btn" disabled={Boolean(busy)} onClick={removeAll}>Delete all</button>
                    </div>
                </>
            )}
        </section>
    );
}

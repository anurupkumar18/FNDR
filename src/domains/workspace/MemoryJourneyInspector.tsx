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

    return (
        <section className="pipeline-panel-card memory-journey" aria-labelledby="memory-journey-title">
            <div className="memory-journey__heading">
                <div>
                    <span className="pipeline-header-kicker">Debug builds only · private local evidence</span>
                    <h3 id="memory-journey-title">Memory Journey</h3>
                    <p className="pipeline-muted">
                        Follow one real capture through observed pipeline boundaries. This records prompts and content, not model chain-of-thought.
                    </p>
                    <p className="pipeline-muted">
                        Live case: open the target first, return here to arm, then switch straight back during the handoff.
                    </p>
                </div>
                <span className={`memory-journey__state memory-journey__state--${status?.active_state ?? "idle"}`} role="status" aria-live="polite">
                    {status?.active_state?.split("_").join(" ") ?? "idle"}
                </span>
            </div>

            {error && <p className="pipeline-error" role="alert">{error}</p>}

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

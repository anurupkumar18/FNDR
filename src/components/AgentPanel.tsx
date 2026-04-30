import { useCallback, useEffect, useRef, useState } from "react";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { usePolling } from "../hooks/usePolling";
import { createClientId } from "../lib/id";
import {
    type AgentStatus,
    type HermesBridgeStatus,
    getAgentStatus,
    getHermesBridgeStatus,
    installHermesBridge,
    quickSetupOllama,
    saveHermesSetup,
    sendDirectChat,
    sendHermesMessage,
    startHermesGateway,
    stopAgent,
    stopHermesGateway,
    syncHermesBridgeContext,
} from "../api/tauri";
import "./AgentPanel.css";

interface AgentPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type AgentView = "overview" | "hermes";
type HermesProviderKind = "ollama" | "codex" | "openrouter" | "custom";

interface HermesUiMessage {
    role: "user" | "assistant";
    content: string;
}

const HERMES_DOCS_URL = "https://hermes-agent.nousresearch.com/docs/";
const OLLAMA_DOWNLOAD_URL = "https://ollama.com/download";
const DEFAULT_OLLAMA_BASE_URL = "http://127.0.0.1:11434/v1";

function nextConversationId(): string {
    return createClientId("fndr-hermes");
}

function isProviderKind(value: string | null | undefined): value is HermesProviderKind {
    return value === "ollama" || value === "codex" || value === "openrouter" || value === "custom";
}

function inferInitialProvider(hermes: HermesBridgeStatus | null): HermesProviderKind {
    if (isProviderKind(hermes?.provider_kind)) {
        return hermes.provider_kind;
    }
    if (hermes?.ollama_installed && hermes?.ollama_reachable) {
        return "ollama";
    }
    if (hermes?.codex_logged_in) {
        return "codex";
    }
    return "openrouter";
}

function defaultModelForProvider(
    provider: HermesProviderKind,
    hermes: HermesBridgeStatus | null
): string {
    if (provider === "ollama") {
        return hermes?.ollama_models[0] ?? "llama3.2:latest";
    }
    if (provider === "codex") {
        return "gpt-5.3-codex";
    }
    if (provider === "custom") {
        return "gpt-4.1-mini";
    }
    return "openai/gpt-5-mini";
}

function defaultBaseUrlForProvider(
    provider: HermesProviderKind,
    hermes: HermesBridgeStatus | null
): string {
    if (provider === "ollama") {
        return hermes?.ollama_base_url ?? DEFAULT_OLLAMA_BASE_URL;
    }
    if (provider === "custom") {
        return hermes?.provider_kind === "custom" ? hermes.base_url ?? "" : "";
    }
    return "";
}

async function openExternalUrl(url: string): Promise<void> {
    try {
        await shellOpen(url);
        return;
    } catch {
        // ignore
    }
    window.open(url, "_blank", "noopener,noreferrer");
}

function getReadinessStep(hermes: HermesBridgeStatus | null): number {
    if (!hermes) return 0;
    if (!hermes.installed) return hermes.direct_ollama_ready ? 4 : 0;
    if (!hermes.configured) return 1;
    if (hermes.api_server_ready) return 4;
    if (hermes.gateway_running) return 3;
    return 2;
}

function formatTimestamp(timestamp: number | null): string {
    if (!timestamp) return "Not synced";
    return new Date(timestamp).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
    });
}

export function AgentPanel({ isVisible, onClose }: AgentPanelProps) {
    const [activeView, setActiveView] = useState<AgentView>("overview");
    const [status, setStatus] = useState<AgentStatus | null>(null);
    const [hermes, setHermes] = useState<HermesBridgeStatus | null>(null);
    const [busyAction, setBusyAction] = useState<string | null>(null);
    const [hermesError, setHermesError] = useState<string | null>(null);
    const [providerKind, setProviderKind] = useState<HermesProviderKind>("openrouter");
    const [modelName, setModelName] = useState("openai/gpt-5-mini");
    const [apiKey, setApiKey] = useState("");
    const [baseUrl, setBaseUrl] = useState("");
    const [messages, setMessages] = useState<HermesUiMessage[]>([]);
    const [draft, setDraft] = useState("");
    const [conversationId, setConversationId] = useState(() => nextConversationId());
    const [hasSeededForm, setHasSeededForm] = useState(false);
    const [setupExpanded, setSetupExpanded] = useState(false);
    const chatBottomRef = useRef<HTMLDivElement>(null);
    const chatInputRef = useRef<HTMLTextAreaElement>(null);

    const loadAgentWorkspace = useCallback(async (isMounted: () => boolean) => {
        try {
            const [agentStatus, hermesStatus] = await Promise.all([
                getAgentStatus(),
                getHermesBridgeStatus(),
            ]);
            if (isMounted()) {
                setStatus(agentStatus);
                setHermes(hermesStatus);
            }
        } catch (err) {
            console.error("Failed to load agent workspace:", err);
        }
    }, []);
    usePolling(loadAgentWorkspace, 4000, isVisible);

    useEffect(() => {
        if (!hermes || hasSeededForm) return;
        const nextProvider = inferInitialProvider(hermes);
        setProviderKind(nextProvider);
        setModelName(hermes.model_name ?? defaultModelForProvider(nextProvider, hermes));
        setBaseUrl(hermes.base_url ?? defaultBaseUrlForProvider(nextProvider, hermes));
        setHasSeededForm(true);
        if (!hermes.configured) {
            setSetupExpanded(true);
        }
    }, [hasSeededForm, hermes]);

    useEffect(() => {
        chatBottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages, busyAction]);

    const fullAgentConfigured = !!hermes?.installed && !!hermes?.configured;
    const fullAgentReady = !!hermes?.api_server_ready;
    const localFallbackReady = !fullAgentConfigured && !!hermes?.direct_ollama_ready;
    const isHermesReady = fullAgentConfigured || localFallbackReady;

    useEffect(() => {
        if (activeView === "hermes" && isHermesReady) {
            window.setTimeout(() => chatInputRef.current?.focus(), 80);
        }
    }, [activeView, isHermesReady]);

    useEffect(() => {
        if (!isVisible) {
            setHermesError(null);
            setBusyAction(null);
            setActiveView("overview");
            setMessages([]);
            setDraft("");
            setConversationId(nextConversationId());
            setApiKey("");
            setHasSeededForm(false);
            setSetupExpanded(false);
        }
    }, [isVisible]);

    if (!isVisible) return null;

    const handleChooseProvider = (nextProvider: HermesProviderKind) => {
        setProviderKind(nextProvider);
        setHermesError(null);
        setModelName(
            hermes?.provider_kind === nextProvider && hermes.model_name
                ? hermes.model_name
                : defaultModelForProvider(nextProvider, hermes)
        );
        setBaseUrl(
            hermes?.provider_kind === nextProvider && hermes.base_url
                ? hermes.base_url
                : defaultBaseUrlForProvider(nextProvider, hermes)
        );
        if (nextProvider !== "openrouter" && nextProvider !== "custom") {
            setApiKey("");
        }
    };

    const runHermesAction = async (
        action: string,
        fn: () => Promise<HermesBridgeStatus>
    ) => {
        setBusyAction(action);
        setHermesError(null);
        try {
            const next = await fn();
            setHermes(next);
        } catch (err) {
            setHermesError(err instanceof Error ? err.message : String(err));
        } finally {
            setBusyAction(null);
        }
    };

    const handleSaveSetup = async () => {
        await runHermesAction("setup", () =>
            saveHermesSetup({
                provider_kind: providerKind,
                model_name: modelName.trim(),
                api_key:
                    providerKind === "openrouter" || providerKind === "custom"
                        ? apiKey
                        : null,
                base_url:
                    providerKind === "custom" || providerKind === "ollama"
                        ? baseUrl.trim()
                        : null,
            })
        );
        setApiKey("");
        setSetupExpanded(false);
    };

    const handleSend = async () => {
        const input = draft.trim();
        if (!input || busyAction === "send") return;

        const nextMessages = [...messages, { role: "user" as const, content: input }];
        setMessages(nextMessages);
        setDraft("");
        setBusyAction("send");
        setHermesError(null);

        try {
            let replyContent: string;

            if (fullAgentConfigured) {
                const reply = await sendHermesMessage(conversationId, input);
                replyContent = reply.content;
            } else if (hermes?.direct_ollama_ready) {
                // Local fallback mode — available before the full Hermes runtime is online.
                const history = messages.map(m => ({ role: m.role, content: m.content }));
                replyContent = await sendDirectChat(history, input);
            } else {
                throw new Error("Enable the FNDR agent runtime or connect a local Ollama model first.");
            }

            setMessages([...nextMessages, { role: "assistant", content: replyContent }]);
        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            setHermesError(message);
            setMessages([
                ...nextMessages,
                { role: "assistant", content: `Error: ${message}` },
            ]);
        } finally {
            setBusyAction(null);
            window.setTimeout(() => chatInputRef.current?.focus(), 50);
        }
    };

    const readinessStep = getReadinessStep(hermes);
    const currentProviderLabel =
        isProviderKind(hermes?.provider_kind) ? hermes.provider_kind.toUpperCase() : providerKind.toUpperCase();
    const showBaseUrlField = providerKind === "custom" || providerKind === "ollama";
    const showApiKeyField = providerKind === "openrouter" || providerKind === "custom";
    const canSaveSetup = (() => {
        if (busyAction !== null || !modelName.trim()) return false;
        if (providerKind === "openrouter") return apiKey.trim().length > 0;
        if (providerKind === "custom") return baseUrl.trim().length > 0;
        if (providerKind === "ollama") return !!hermes?.ollama_installed;
        return !!hermes?.codex_logged_in;
    })();

    // Show quick-connect banner when Ollama is running with models but not yet configured
    // Works regardless of whether hermes CLI is installed
    const showOllamaBanner =
        hermes !== null &&
        hermes.ollama_reachable &&
        hermes.ollama_models.length > 0 &&
        !hermes.configured;

    const gatewayStatusClass = fullAgentReady || localFallbackReady
        ? "ap-dot-ready"
        : hermes?.gateway_running || fullAgentConfigured
            ? "ap-dot-starting"
            : "ap-dot-off";

    return (
        <div className="ap-root">
            {/* Header */}
            <header className="ap-header">
                <div className="ap-header-left">
                    <div className={`ap-header-dot ${gatewayStatusClass}`} />
                    <span className="ap-header-title">FNDR Agent</span>
                    {hermes?.configured && (
                        <span className="ap-header-badge">
                            {hermes.model_name ?? currentProviderLabel}
                        </span>
                    )}
                </div>
                <button className="ap-close-btn" onClick={onClose} aria-label="Close">
                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                        <path d="M1 1L13 13M13 1L1 13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                    </svg>
                </button>
            </header>

            <div className="ap-layout">
                {/* Sidebar */}
                <nav className="ap-sidebar">
                    <button
                        className={`ap-nav-item ${activeView === "overview" ? "active" : ""}`}
                        onClick={() => setActiveView("overview")}
                    >
                        <svg className="ap-nav-icon" viewBox="0 0 16 16" fill="none">
                            <rect x="1" y="1" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
                            <rect x="9" y="1" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
                            <rect x="1" y="9" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
                            <rect x="9" y="9" width="6" height="6" rx="1.5" stroke="currentColor" strokeWidth="1.2" />
                        </svg>
                        <span>Overview</span>
                    </button>
                    <button
                        className={`ap-nav-item ${activeView === "hermes" ? "active" : ""}`}
                        onClick={() => setActiveView("hermes")}
                    >
                        <svg className="ap-nav-icon" viewBox="0 0 16 16" fill="none">
                            <circle cx="8" cy="8" r="3" stroke="currentColor" strokeWidth="1.2" />
                            <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.05 3.05l1.41 1.41M11.54 11.54l1.41 1.41M3.05 12.95l1.41-1.41M11.54 4.46l1.41-1.41" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
                        </svg>
                        <span>FNDR Agent</span>
                        {readinessStep === 4 && (
                            <span className="ap-nav-ready-dot" />
                        )}
                    </button>
                </nav>

                {/* Content */}
                <main className="ap-content">
                    {activeView === "overview" ? (
                        <OverviewView
                            status={status}
                            hermes={hermes}
                            readinessStep={readinessStep}
                            onStop={() => stopAgent().then(setStatus).catch(console.error)}
                            onOpenHermes={() => setActiveView("hermes")}
                        />
                    ) : (
                        <HermesView
                            hermes={hermes}
                            busyAction={busyAction}
                            hermesError={hermesError}
                            providerKind={providerKind}
                            modelName={modelName}
                            apiKey={apiKey}
                            baseUrl={baseUrl}
                            messages={messages}
                            draft={draft}
                            conversationId={conversationId}
                            hasSeededForm={hasSeededForm}
                            setupExpanded={setupExpanded}
                            isHermesReady={isHermesReady}
                            readinessStep={readinessStep}
                            showOllamaBanner={showOllamaBanner}
                            showBaseUrlField={showBaseUrlField}
                            showApiKeyField={showApiKeyField}
                            canSaveSetup={canSaveSetup}
                            currentProviderLabel={currentProviderLabel}
                            chatBottomRef={chatBottomRef}
                            chatInputRef={chatInputRef}
                            onChooseProvider={handleChooseProvider}
                            onModelNameChange={setModelName}
                            onApiKeyChange={setApiKey}
                            onBaseUrlChange={setBaseUrl}
                            onSaveSetup={handleSaveSetup}
                            onSetupExpanded={setSetupExpanded}
                            onDraftChange={setDraft}
                            onSend={handleSend}
                            onResetConversation={() => {
                                setMessages([]);
                                setConversationId(nextConversationId());
                            }}
                            onInstall={() => runHermesAction("install", installHermesBridge)}
                            onQuickSetupOllama={() => runHermesAction("quick-ollama", quickSetupOllama)}
                            onStart={() => runHermesAction("start", startHermesGateway)}
                            onStop={() => runHermesAction("stop", stopHermesGateway)}
                            onSync={() => runHermesAction("sync", syncHermesBridgeContext)}
                        />
                    )}
                </main>
            </div>
        </div>
    );
}

// ─── Overview View ───────────────────────────────────────────────────────────

interface OverviewViewProps {
    status: AgentStatus | null;
    hermes: HermesBridgeStatus | null;
    readinessStep: number;
    onStop: () => void;
    onOpenHermes: () => void;
}

function OverviewView({ status, hermes, readinessStep, onStop, onOpenHermes }: OverviewViewProps) {
    const isRunning = status?.status === "running";
    const fullAgentReady = !!hermes?.api_server_ready;
    const localFallbackReady = !hermes?.installed && !!hermes?.direct_ollama_ready;
    const runtimeLabel = localFallbackReady ? "Local chat" : "Agent runtime";
    const runtimeValue = localFallbackReady
        ? "Ready"
        : readinessStep === 4
            ? "Ready"
            : readinessStep === 3
                ? "Starting"
                : readinessStep >= 2
                    ? "Stopped"
                    : "Not installed";
    const runtimeDetail = localFallbackReady ? (hermes?.model_name ?? "Ollama") : (hermes?.api_url ?? "");
    const runtimeDotClass = localFallbackReady || fullAgentReady
        ? "ap-dot-ready"
        : readinessStep >= 2
            ? "ap-dot-starting"
            : "ap-dot-off";

    return (
        <div className="ap-section-stack">
            {/* Status node */}
            <div className="ap-overview-hero">
                <div className="ap-overview-node-ring">
                    <div className={`ap-overview-node ${isRunning ? "running" : ""}`}>
                        <svg width="28" height="28" viewBox="0 0 28 28" fill="none">
                            <path d="M14 4C8.48 4 4 8.48 4 14s4.48 10 10 10 10-4.48 10-10S19.52 4 14 4zm-2 14.5v-9l7 4.5-7 4.5z" fill="currentColor" />
                        </svg>
                    </div>
                </div>
                <div className="ap-overview-hero-text">
                    <div className="ap-overview-status-label">
                        {isRunning ? "Agent running" : "Agent idle"}
                    </div>
                    <h3>{status?.task_title ?? "No active task"}</h3>
                    <p>{status?.last_message ?? "Start a task from the command palette or open the FNDR Agent."}</p>
                </div>
                <div className="ap-overview-hero-actions">
                    {isRunning && (
                        <button className="ap-btn ap-btn-danger" onClick={onStop}>
                            Stop task
                        </button>
                    )}
                    <button className="ap-btn" onClick={onOpenHermes}>
                        Open Agent
                    </button>
                </div>
            </div>

            {/* System metrics */}
            <div className="ap-metrics-row">
                <MetricCard
                    label={runtimeLabel}
                    value={runtimeValue}
                    detail={runtimeDetail}
                    dotClass={runtimeDotClass}
                />
                <MetricCard
                    label="Provider"
                    value={hermes?.configured ? (hermes.provider_kind?.toUpperCase() ?? "—") : "Not configured"}
                    detail={hermes?.model_name ?? ""}
                    dotClass={hermes?.configured ? "ap-dot-ready" : "ap-dot-off"}
                />
                <MetricCard
                    label="Context sync"
                    value={hermes?.context_ready ? "Synced" : "Pending"}
                    detail={formatTimestamp(hermes?.last_synced_at ?? null)}
                    dotClass={hermes?.context_ready ? "ap-dot-ready" : "ap-dot-off"}
                />
            </div>

            {/* Recent memories */}
            {(hermes?.recent_memories?.length ?? 0) > 0 && (
                <section className="ap-card">
                    <div className="ap-card-title">Recent FNDR context</div>
                    <div className="ap-memory-list">
                        {hermes!.recent_memories.map((m, i) => (
                            <div key={i} className="ap-memory-row">
                                <div className="ap-memory-app">{m.app_name}</div>
                                <div className="ap-memory-title">{m.title}</div>
                                <div className="ap-memory-summary">{m.summary}</div>
                            </div>
                        ))}
                    </div>
                </section>
            )}
        </div>
    );
}

// ─── Hermes View ─────────────────────────────────────────────────────────────

interface HermesViewProps {
    hermes: HermesBridgeStatus | null;
    busyAction: string | null;
    hermesError: string | null;
    providerKind: HermesProviderKind;
    modelName: string;
    apiKey: string;
    baseUrl: string;
    messages: HermesUiMessage[];
    draft: string;
    conversationId: string;
    hasSeededForm: boolean;
    setupExpanded: boolean;
    isHermesReady: boolean;
    readinessStep: number;
    showOllamaBanner: boolean;
    showBaseUrlField: boolean;
    showApiKeyField: boolean;
    canSaveSetup: boolean;
    currentProviderLabel: string;
    chatBottomRef: React.RefObject<HTMLDivElement>;
    chatInputRef: React.RefObject<HTMLTextAreaElement>;
    onChooseProvider: (p: HermesProviderKind) => void;
    onModelNameChange: (v: string) => void;
    onApiKeyChange: (v: string) => void;
    onBaseUrlChange: (v: string) => void;
    onSaveSetup: () => void;
    onSetupExpanded: (v: boolean) => void;
    onDraftChange: (v: string) => void;
    onSend: () => void;
    onResetConversation: () => void;
    onInstall: () => void;
    onQuickSetupOllama: () => void;
    onStart: () => void;
    onStop: () => void;
    onSync: () => void;
}

function HermesView(props: HermesViewProps) {
    const {
        hermes, busyAction, hermesError, providerKind, modelName, apiKey, baseUrl,
        messages, draft, setupExpanded, isHermesReady, readinessStep, showOllamaBanner,
        showBaseUrlField, showApiKeyField, canSaveSetup,
        chatBottomRef, chatInputRef,
        onChooseProvider, onModelNameChange, onApiKeyChange, onBaseUrlChange,
        onSaveSetup, onSetupExpanded, onDraftChange, onSend, onResetConversation,
        onInstall, onQuickSetupOllama, onStart, onStop, onSync,
    } = props;

    const busy = busyAction !== null;
    const fullAgentConfigured = !!hermes?.installed && !!hermes?.configured;
    const fullAgentReady = !!hermes?.api_server_ready;
    const localFallbackReady = !fullAgentConfigured && !!hermes?.direct_ollama_ready;
    const showInstallCard =
        hermes !== null &&
        !hermes.installed &&
        (!localFallbackReady || hermes.bundled_repo_available);
    const installTitle = hermes?.bundled_repo_available
        ? "Enable full FNDR agent"
        : "Install Hermes";
    const installBody = hermes?.bundled_repo_available
        ? localFallbackReady
            ? "FNDR can already do quick local chat through Ollama, but enabling the bundled Hermes runtime unlocks the full native agent experience: tools, longer-lived conversations, and Hermes-style behavior inside FNDR."
            : "FNDR found the vendored hermes-agent clone in this repo. Enabling it prepares a private runtime inside FNDR so the agent behaves like a built-in feature instead of a separately installed CLI."
        : "FNDR will run the official Hermes installer to set up the local agent runtime.";
    const installButtonLabel = hermes?.bundled_repo_available ? "Enable Agent" : "Install Hermes";
    const emptyStateTitle = fullAgentReady
        ? "FNDR Agent is ready"
        : fullAgentConfigured
            ? "FNDR Agent will start on first message"
        : localFallbackReady
            ? "Local chat is ready"
            : "Start the agent runtime";
    const emptyStateBody = fullAgentReady
        ? "Ask about your FNDR memories, draft something, or run a multi-step task."
        : fullAgentConfigured
            ? "Send a message and FNDR will launch the bundled Hermes runtime automatically."
        : localFallbackReady
            ? "You can chat through Ollama right now. Enable the full FNDR agent runtime for richer Hermes behavior."
            : "Bring the FNDR agent online above, then chat here.";
    const chatTitle = fullAgentConfigured ? "Agent chat" : "Local chat";
    const assistantLabel = "Agent";

    return (
        <div className="ap-section-stack">
            {/* Step progress */}
            <div className="ap-steps-bar">
                {["Install", "Configure", "Start", "Chat"].map((label, i) => (
                    <div key={label} className={`ap-step ${readinessStep > i ? "done" : readinessStep === i ? "active" : ""}`}>
                        <div className="ap-step-dot">
                            {readinessStep > i ? (
                                <svg width="10" height="10" viewBox="0 0 10 10">
                                    <path d="M1.5 5L4 7.5 8.5 2.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" fill="none" />
                                </svg>
                            ) : (
                                <span>{i + 1}</span>
                            )}
                        </div>
                        <span className="ap-step-label">{label}</span>
                        {i < 3 && <div className="ap-step-line" />}
                    </div>
                ))}
            </div>

            {/* Ollama quick-connect banner */}
            {showOllamaBanner && (
                <div className="ap-ollama-banner">
                    <div className="ap-ollama-banner-left">
                        <div className="ap-ollama-pulse" />
                        <div>
                            <div className="ap-ollama-banner-title">Ollama detected</div>
                            <div className="ap-ollama-banner-sub">
                                {hermes!.ollama_models.length} model{hermes!.ollama_models.length !== 1 ? "s" : ""} available — {hermes!.ollama_models[0]}
                            </div>
                        </div>
                    </div>
                    <button
                        className="ap-btn ap-btn-primary"
                        onClick={onQuickSetupOllama}
                        disabled={busy}
                    >
                        {busyAction === "quick-ollama" ? "Connecting..." : "Connect Ollama"}
                    </button>
                </div>
            )}

            {/* Metrics */}
            <div className="ap-metrics-row">
                <MetricCard
                    label="Gateway"
                    value={hermes?.gateway_running ? "Running" : "Stopped"}
                    detail={hermes?.api_url ?? "—"}
                    dotClass={fullAgentReady ? "ap-dot-ready" : hermes?.gateway_running ? "ap-dot-starting" : "ap-dot-off"}
                />
                <MetricCard
                    label="Context"
                    value={hermes?.context_ready ? "Ready" : "Pending sync"}
                    detail={formatTimestamp(hermes?.last_synced_at ?? null)}
                    dotClass={hermes?.context_ready ? "ap-dot-ready" : "ap-dot-off"}
                />
                <MetricCard
                    label="Provider"
                    value={hermes?.configured ? (hermes.provider_kind?.toUpperCase() ?? "—") : "Not set"}
                    detail={hermes?.model_name ?? "No model configured"}
                    dotClass={hermes?.configured ? "ap-dot-ready" : "ap-dot-off"}
                />
            </div>

            {/* Install / enable step */}
            {showInstallCard && (
                <section className="ap-card">
                    <div className="ap-card-title">{installTitle}</div>
                    <p className="ap-card-body">
                        {installBody}
                    </p>
                    {!hermes?.bundled_repo_available && (
                        <div className="ap-terminal-line">
                            <span className="ap-terminal-prompt">$</span>
                            <span>{hermes?.install_command ?? "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash"}</span>
                        </div>
                    )}
                    <div className="ap-inline-actions">
                        <button
                            className="ap-btn ap-btn-primary"
                            onClick={onInstall}
                            disabled={busy}
                        >
                            {busyAction === "install" ? "Preparing..." : installButtonLabel}
                        </button>
                        <button
                            className="ap-btn"
                            onClick={() => void openExternalUrl(HERMES_DOCS_URL)}
                        >
                            Docs
                        </button>
                    </div>
                </section>
            )}

            {/* Configure step */}
            {(hermes?.installed || hermes?.bundled_repo_available) && (
                <section className="ap-card">
                    <button
                        className="ap-card-collapsible-header"
                        onClick={() => onSetupExpanded(!setupExpanded)}
                    >
                        <div>
                            <div className="ap-card-title">Configure provider</div>
                            {hermes.configured && !setupExpanded && (
                                <div className="ap-card-subtitle">
                                    {hermes.provider_kind?.toUpperCase()} · {hermes.model_name}
                                </div>
                            )}
                        </div>
                        <svg
                            className={`ap-chevron ${setupExpanded ? "open" : ""}`}
                            width="14" height="14" viewBox="0 0 14 14" fill="none"
                        >
                            <path d="M3 5L7 9L11 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                        </svg>
                    </button>

                    {setupExpanded && (
                        <div className="ap-setup-body">
                            {/* Provider tabs */}
                            <div className="ap-provider-tabs">
                                {(["ollama", "codex", "openrouter", "custom"] as HermesProviderKind[]).map((option) => (
                                    <button
                                        key={option}
                                        className={`ap-provider-tab ${providerKind === option ? "active" : ""}`}
                                        onClick={() => onChooseProvider(option)}
                                    >
                                        <span className="ap-provider-tab-name">{providerTabLabel(option)}</span>
                                        <span className="ap-provider-tab-status">
                                            {providerTabStatus(option, hermes)}
                                        </span>
                                    </button>
                                ))}
                            </div>

                            {/* Provider note */}
                            <div className="ap-provider-note">
                                {providerDetailNote(providerKind, hermes)}
                                {providerKind === "ollama" && !hermes.ollama_installed && (
                                    <button
                                        className="ap-link-btn"
                                        onClick={() => void openExternalUrl(OLLAMA_DOWNLOAD_URL)}
                                    >
                                        Get Ollama →
                                    </button>
                                )}
                            </div>

                            {/* Form fields */}
                            <div className="ap-form-grid">
                                <label className="ap-field">
                                    <span>Model</span>
                                    {providerKind === "ollama" && (hermes.ollama_models.length ?? 0) > 0 ? (
                                        <select
                                            value={modelName}
                                            onChange={(e) => onModelNameChange(e.target.value)}
                                            disabled={busy}
                                        >
                                            {hermes.ollama_models.map((m) => (
                                                <option key={m} value={m}>{m}</option>
                                            ))}
                                        </select>
                                    ) : (
                                        <input
                                            value={modelName}
                                            onChange={(e) => onModelNameChange(e.target.value)}
                                            placeholder={defaultModelForProvider(providerKind, hermes)}
                                            disabled={busy}
                                        />
                                    )}
                                </label>

                                {showBaseUrlField && (
                                    <label className="ap-field">
                                        <span>{providerKind === "ollama" ? "Ollama URL" : "Base URL"}</span>
                                        <input
                                            value={baseUrl}
                                            onChange={(e) => onBaseUrlChange(e.target.value)}
                                            placeholder={providerKind === "ollama" ? DEFAULT_OLLAMA_BASE_URL : "http://localhost:8000/v1"}
                                            disabled={busy}
                                        />
                                    </label>
                                )}

                                {showApiKeyField && (
                                    <label className="ap-field ap-field-wide">
                                        <span>
                                            {providerKind === "openrouter" ? "OpenRouter API key" : "Endpoint API key (optional)"}
                                        </span>
                                        <input
                                            type="password"
                                            value={apiKey}
                                            onChange={(e) => onApiKeyChange(e.target.value)}
                                            placeholder={
                                                providerKind === "openrouter"
                                                    ? "sk-or-..."
                                                    : "Leave empty if not required"
                                            }
                                            disabled={busy}
                                        />
                                    </label>
                                )}
                            </div>

                            <div className="ap-inline-actions">
                                <button
                                    className="ap-btn ap-btn-primary"
                                    onClick={onSaveSetup}
                                    disabled={!canSaveSetup}
                                >
                                    {busyAction === "setup" ? "Saving..." : "Save configuration"}
                                </button>
                                <button
                                    className="ap-btn"
                                    onClick={onSync}
                                    disabled={busy || !hermes.configured}
                                >
                                    {busyAction === "sync" ? "Syncing..." : "Sync context"}
                                </button>
                            </div>
                        </div>
                    )}
                </section>
            )}

            {/* Gateway controls */}
            {hermes?.installed && hermes.configured && (
                <section className="ap-card ap-gateway-card">
                    <div className="ap-gateway-header">
                        <div className="ap-gateway-status">
                            <div className={`ap-gateway-dot ${fullAgentReady ? "ready" : hermes.gateway_running ? "starting" : "off"}`} />
                            <span className="ap-gateway-label">
                                {fullAgentReady ? "Agent runtime online" : hermes.gateway_running ? "Starting up..." : "Agent runtime offline"}
                            </span>
                            <span className="ap-gateway-url">{hermes.api_url}</span>
                        </div>
                        <div className="ap-inline-actions">
                            <button
                                className="ap-btn ap-btn-primary"
                                onClick={onStart}
                                disabled={busy || fullAgentReady}
                            >
                                {busyAction === "start" ? "Starting..." : "Start"}
                            </button>
                            <button
                                className="ap-btn"
                                onClick={onStop}
                                disabled={busy || !hermes.gateway_running}
                            >
                                {busyAction === "stop" ? "Stopping..." : "Stop"}
                            </button>
                            <button className="ap-btn" onClick={onSync} disabled={busy}>
                                {busyAction === "sync" ? "Syncing..." : "Sync"}
                            </button>
                        </div>
                    </div>
                </section>
            )}

            {/* Chat — available in local fallback mode or full agent mode */}
            {hermes?.configured && (hermes.installed || hermes.direct_ollama_ready) && (
                <section className="ap-card ap-chat-card">
                    <div className="ap-chat-header">
                        <div className="ap-card-title">{chatTitle}</div>
                        <button
                            className="ap-ghost-btn"
                            onClick={onResetConversation}
                            disabled={busyAction === "send"}
                        >
                            New conversation
                        </button>
                    </div>

                    <div className="ap-chat-messages" role="log">
                        {messages.length === 0 && (
                            <div className="ap-chat-empty">
                                <div className="ap-chat-empty-icon">
                                    <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
                                        <circle cx="16" cy="16" r="12" stroke="currentColor" strokeWidth="1.5" opacity="0.4" />
                                        <circle cx="16" cy="16" r="4" fill="currentColor" opacity="0.3" />
                                    </svg>
                                </div>
                                <div className="ap-chat-empty-title">
                                    {emptyStateTitle}
                                </div>
                                <p>{emptyStateBody}</p>
                            </div>
                        )}

                        {messages.map((msg, i) => (
                            <div key={i} className={`ap-chat-row ap-chat-${msg.role}`}>
                                <div className="ap-chat-role">
                                    {msg.role === "user" ? "You" : assistantLabel}
                                </div>
                                <div className="ap-chat-bubble">
                                    {msg.content}
                                </div>
                            </div>
                        ))}

                        {busyAction === "send" && (
                            <div className="ap-chat-row ap-chat-assistant">
                                <div className="ap-chat-role">{assistantLabel}</div>
                                <div className="ap-chat-bubble ap-chat-thinking">
                                    <span /><span /><span />
                                </div>
                            </div>
                        )}
                        <div ref={chatBottomRef} />
                    </div>

                    <div className="ap-chat-input-area">
                        <textarea
                            ref={chatInputRef}
                            className="ap-chat-textarea"
                            placeholder={
                                isHermesReady
                                    ? fullAgentConfigured
                                        ? "Ask the FNDR agent..."
                                        : "Ask local chat..."
                                    : "Set up the agent runtime to send messages"
                            }
                            value={draft}
                            onChange={(e) => onDraftChange(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Enter" && !e.shiftKey) {
                                    e.preventDefault();
                                    void onSend();
                                }
                            }}
                            rows={1}
                            disabled={!isHermesReady || busyAction === "send"}
                        />
                        <button
                            className="ap-chat-send-btn"
                            onClick={onSend}
                            disabled={!isHermesReady || !draft.trim() || busyAction === "send"}
                            aria-label="Send"
                        >
                            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                                <path d="M14 2L2 7.5L7 8.5M14 2L9 14L7 8.5M14 2L7 8.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
                            </svg>
                        </button>
                    </div>
                </section>
            )}

            {/* Error */}
            {(hermesError || hermes?.last_error) && (
                <div className="ap-error-banner">
                    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                        <circle cx="7" cy="7" r="6" stroke="currentColor" strokeWidth="1.2" />
                        <path d="M7 4v3M7 9.5v.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
                    </svg>
                    {hermesError ?? hermes?.last_error}
                </div>
            )}
        </div>
    );
}

// ─── Shared sub-components ────────────────────────────────────────────────────

function MetricCard({ label, value, detail, dotClass }: {
    label: string;
    value: string;
    detail: string;
    dotClass: string;
}) {
    return (
        <div className="ap-metric-card">
            <div className="ap-metric-header">
                <div className={`ap-dot ${dotClass}`} />
                <span className="ap-metric-label">{label}</span>
            </div>
            <div className="ap-metric-value">{value}</div>
            {detail && <div className="ap-metric-detail">{detail}</div>}
        </div>
    );
}

// ─── Provider helpers ─────────────────────────────────────────────────────────

function providerTabLabel(p: HermesProviderKind): string {
    switch (p) {
        case "ollama": return "Ollama";
        case "codex": return "Codex";
        case "openrouter": return "OpenRouter";
        case "custom": return "Custom";
    }
}

function providerTabStatus(p: HermesProviderKind, hermes: HermesBridgeStatus | null): string {
    if (!hermes) return "Checking...";
    switch (p) {
        case "ollama":
            if (!hermes.ollama_installed) return "Not installed";
            if (!hermes.ollama_reachable) return "Not running";
            return hermes.ollama_models.length > 0
                ? `${hermes.ollama_models.length} model${hermes.ollama_models.length !== 1 ? "s" : ""}`
                : "No models";
        case "codex":
            if (!hermes.codex_cli_installed) return "Not found";
            return hermes.codex_logged_in ? "Authenticated" : "Not signed in";
        case "openrouter":
            return "API key required";
        case "custom":
            return "Bring your endpoint";
    }
}

function providerDetailNote(p: HermesProviderKind, hermes: HermesBridgeStatus | null): string {
    switch (p) {
        case "ollama":
            if (!hermes?.ollama_installed) return "Install Ollama to run the FNDR agent fully locally. No API key needed.";
            if (!hermes.ollama_reachable) return "Ollama is installed but not running. Open Ollama or run `ollama serve`.";
            if (hermes.ollama_models.length === 0) return "Ollama is running but has no models. Pull one: `ollama pull llama3.2`";
            return `Running ${hermes.ollama_models.length} local model${hermes.ollama_models.length !== 1 ? "s" : ""}. FNDR can chat locally right away, and the full bundled agent runtime can layer on top of the same Ollama setup.`;
        case "codex":
            return hermes?.codex_logged_in
                ? "FNDR detected your local Codex auth. No extra API key is needed inside FNDR."
                : "Sign in to Codex on this Mac first, then return here.";
        case "openrouter":
            return "Access frontier models through OpenRouter. FNDR stores the key in its contained agent runtime.";
        case "custom":
            return "Point Hermes at any OpenAI-compatible endpoint — self-hosted or private.";
    }
}

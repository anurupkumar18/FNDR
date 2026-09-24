import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
    deleteAgentChat,
    getAgentChat,
    getHermesBridgeStatus,
    installHermesBridge,
    listAgentChats,
    listMemoryCards,
    saveHermesSetup,
    searchMemoryCards,
    sendHermesMessage,
    type AgentChatMessage,
    type AgentChatSummary,
    type AttachedMemory,
    type CodexAccountStatus,
    type HermesBridgeStatus,
    type MemoryCard,
} from "@/shared/ipc/tauri";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { SegmentedControl } from "@/shared/components/SegmentedControl";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import { CodexAccountCard } from "./CodexAccountCard";
import "./AgentWorkspace.css";

type Provider = "codex" | "ollama" | "openrouter" | "custom";

const PROVIDER_LABEL: Record<Provider, string> = {
    codex: "ChatGPT",
    ollama: "Ollama",
    openrouter: "OpenRouter",
    custom: "Custom",
};

/** Mirrors MAX_ATTACHED_MEMORIES in agent_chats.rs. */
const MAX_MEMORIES = 8;
const PICKER_RESULTS = 12;

function newConversationId(): string {
    return `fndr-${crypto.randomUUID()}`;
}

function isProvider(value: string | null | undefined): value is Provider {
    return value === "codex" || value === "ollama" || value === "openrouter" || value === "custom";
}

function relativeTime(ms: number): string {
    const minutes = Math.round((Date.now() - ms) / 60_000);
    if (minutes < 1) return "now";
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours}h`;
    return new Date(ms).toLocaleDateString([], { month: "short", day: "numeric" });
}

function toAttached(card: MemoryCard): AttachedMemory {
    return { id: card.id, title: card.title || card.window_title, appName: card.app_name, timestamp: card.timestamp };
}

interface AgentWorkspaceProps {
    isVisible: boolean;
    onClose: () => void;
}

/**
 * The Agent page: one conversation surface for Hermes. Past chats live in the
 * sidebar, the model lives in a header chip, and FNDR memories can be attached
 * to any message as reference context for the task.
 */
export function AgentWorkspace({ isVisible, onClose }: AgentWorkspaceProps) {
    const [hermes, setHermes] = useState<HermesBridgeStatus | null>(null);
    const [chats, setChats] = useState<AgentChatSummary[]>([]);
    const [conversationId, setConversationId] = useState(newConversationId);
    const [messages, setMessages] = useState<AgentChatMessage[]>([]);
    const [draft, setDraft] = useState("");
    const [attached, setAttached] = useState<AttachedMemory[]>([]);
    const [sending, setSending] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [setupOpen, setSetupOpen] = useState(false);
    const [pickerOpen, setPickerOpen] = useState(false);

    const dialogRef = useRef<HTMLDivElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const threadEndRef = useRef<HTMLDivElement>(null);
    useModalFocus(isVisible, dialogRef, closeButtonRef, onClose);

    const refreshHermes = useCallback(async () => {
        try {
            setHermes(await getHermesBridgeStatus());
        } catch (reason) {
            setError(reason instanceof Error ? reason.message : String(reason));
        }
    }, []);

    const refreshChats = useCallback(async () => {
        try {
            setChats(await listAgentChats());
        } catch {
            setChats([]);
        }
    }, []);

    useEffect(() => {
        if (!isVisible) return;
        void refreshHermes();
        void refreshChats();
    }, [isVisible, refreshChats, refreshHermes]);

    useEffect(() => {
        threadEndRef.current?.scrollIntoView({ block: "end" });
    }, [messages, sending]);

    const configured = !!hermes?.installed && !!hermes.configured;
    const showSetup = hermes !== null && (!configured || setupOpen);
    const modelLabel = configured
        ? `${isProvider(hermes.provider_kind) ? PROVIDER_LABEL[hermes.provider_kind] : hermes.provider_kind} · ${hermes.model_name ?? "default"}`
        : "Not set up";

    const startNewChat = () => {
        setConversationId(newConversationId());
        setMessages([]);
        setAttached([]);
        setError(null);
        setSetupOpen(false);
    };

    const openChat = async (id: string) => {
        setError(null);
        setSetupOpen(false);
        try {
            const chat = await getAgentChat(id);
            if (!chat) return;
            setConversationId(chat.id);
            setMessages(chat.messages);
            setAttached([]);
        } catch (reason) {
            setError(reason instanceof Error ? reason.message : String(reason));
        }
    };

    const removeChat = async (id: string) => {
        await deleteAgentChat(id).catch(() => undefined);
        if (id === conversationId) startNewChat();
        void refreshChats();
    };

    const send = async () => {
        const text = draft.trim();
        if (!text || sending || !configured) return;
        const memories = attached;
        setMessages((current) => [...current, { role: "user", content: text, at: Date.now(), memories }]);
        setDraft("");
        setAttached([]);
        setPickerOpen(false);
        setSending(true);
        setError(null);
        try {
            const reply = await sendHermesMessage(conversationId, text, memories.map((m) => m.id));
            setMessages((current) => [
                ...current,
                { role: "assistant", content: reply.content, at: Date.now(), memories: [] },
            ]);
            void refreshChats();
        } catch (reason) {
            setError(reason instanceof Error ? reason.message : String(reason));
        } finally {
            setSending(false);
        }
    };

    if (!isVisible) return null;

    return (
        <div ref={dialogRef} className="aw-root" role="dialog" aria-modal="true" aria-labelledby="aw-title">
            <aside className="aw-rail" aria-label="Chats">
                <button type="button" className="aw-new-chat" onClick={startNewChat}>
                    <svg viewBox="0 0 16 16" aria-hidden="true">
                        <path d="M8 3v10M3 8h10" />
                    </svg>
                    New chat
                </button>
                <p className="aw-rail-label">Recent</p>
                {chats.length === 0 ? <p className="aw-rail-empty">Your chats with Hermes appear here.</p> : null}
                <ul className="aw-chat-list">
                    {chats.map((chat) => (
                        <li key={chat.id} className={chat.id === conversationId ? "is-current" : undefined}>
                            <button type="button" className="aw-chat-item" onClick={() => void openChat(chat.id)}>
                                <span className="aw-chat-title">{chat.title}</span>
                                <span className="aw-chat-time">{relativeTime(chat.updatedAt)}</span>
                            </button>
                            <button
                                type="button"
                                className="aw-chat-delete"
                                aria-label={`Delete chat "${chat.title}"`}
                                onClick={() => void removeChat(chat.id)}
                            >
                                <svg viewBox="0 0 12 12" aria-hidden="true">
                                    <path d="M3 3l6 6M9 3l-6 6" />
                                </svg>
                            </button>
                        </li>
                    ))}
                </ul>
            </aside>

            <section className="aw-main">
                <PanelHeader
                    title="Agent"
                    titleId="aw-title"
                    subtitle="Hermes, with FNDR memories you choose as context."
                    actions={
                        <button
                            type="button"
                            className={`aw-model-chip${configured ? "" : " is-unset"}`}
                            onClick={() => setSetupOpen((open) => !open)}
                            aria-expanded={showSetup}
                        >
                            <span className={`aw-status-dot${hermes?.api_server_ready ? " is-ready" : ""}`} aria-hidden="true" />
                            {modelLabel}
                        </button>
                    }
                    closeLabel="Close Agent"
                    closeRef={closeButtonRef}
                    onClose={onClose}
                />

                {showSetup && hermes ? (
                    <AgentSetup
                        hermes={hermes}
                        onSaved={async () => {
                            await refreshHermes();
                            setSetupOpen(false);
                        }}
                        onInstalled={refreshHermes}
                    />
                ) : (
                    <div className="aw-thread" aria-live="polite">
                        {messages.length === 0 && !sending ? (
                            <div className="aw-empty">
                                <h3>What should we work on?</h3>
                                <p>
                                    Ask Hermes to plan, draft or research. Use <strong>Memories</strong> to hand it
                                    things you saw on screen, like a doc, a thread or a bug you were looking at.
                                </p>
                            </div>
                        ) : null}
                        {messages.map((message, index) => (
                            <div key={`${message.at}-${index}`} className={`aw-message aw-${message.role}`}>
                                {message.memories.length > 0 ? (
                                    <div className="aw-message-memories">
                                        {message.memories.map((memory) => (
                                            <span key={memory.id} className="aw-memory-chip is-static" title={memory.appName}>
                                                {memory.title}
                                            </span>
                                        ))}
                                    </div>
                                ) : null}
                                <p className="aw-bubble">{message.content}</p>
                            </div>
                        ))}
                        {sending ? (
                            <div className="aw-message aw-assistant" role="status">
                                <p className="aw-bubble aw-thinking">
                                    <ThinkingIndicator state="composing" size="sm" />
                                    {hermes?.api_server_ready ? "Thinking…" : "Starting Hermes…"}
                                </p>
                            </div>
                        ) : null}
                        <div ref={threadEndRef} />
                    </div>
                )}

                {error ? (
                    <p className="aw-error" role="alert">
                        {error}
                    </p>
                ) : null}

                {!showSetup ? (
                    <div className="aw-composer">
                        {pickerOpen ? (
                            <MemoryPicker
                                selected={attached}
                                onToggle={(memory) =>
                                    setAttached((current) =>
                                        current.some((m) => m.id === memory.id)
                                            ? current.filter((m) => m.id !== memory.id)
                                            : current.length >= MAX_MEMORIES
                                              ? current
                                              : [...current, memory],
                                    )
                                }
                                providerLabel={modelLabel.split(" · ")[0]}
                                onClose={() => setPickerOpen(false)}
                            />
                        ) : null}
                        {attached.length > 0 ? (
                            <div className="aw-attached" aria-label="Attached memories">
                                {attached.map((memory) => (
                                    <span key={memory.id} className="aw-memory-chip">
                                        {memory.title}
                                        <button
                                            type="button"
                                            aria-label={`Remove ${memory.title}`}
                                            onClick={() => setAttached((current) => current.filter((m) => m.id !== memory.id))}
                                        >
                                            ×
                                        </button>
                                    </span>
                                ))}
                            </div>
                        ) : null}
                        <div className="aw-input-row">
                            <button
                                type="button"
                                className={`aw-memories-btn${pickerOpen ? " is-open" : ""}`}
                                aria-expanded={pickerOpen}
                                onClick={() => setPickerOpen((open) => !open)}
                                disabled={!configured}
                            >
                                Memories{attached.length > 0 ? ` · ${attached.length}` : ""}
                            </button>
                            <textarea
                                className="aw-input"
                                rows={1}
                                value={draft}
                                placeholder={configured ? "Message Hermes" : "Set up a model to start"}
                                aria-label="Message Hermes"
                                disabled={!configured}
                                onChange={(event) => setDraft(event.target.value)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter" && !event.shiftKey) {
                                        event.preventDefault();
                                        void send();
                                    }
                                }}
                            />
                            <button
                                type="button"
                                className="aw-send"
                                aria-label="Send"
                                disabled={!configured || sending || !draft.trim()}
                                onClick={() => void send()}
                            >
                                <svg viewBox="0 0 16 16" aria-hidden="true">
                                    <path d="M8 13V3M3.5 7.5L8 3l4.5 4.5" />
                                </svg>
                            </button>
                        </div>
                    </div>
                ) : null}
            </section>
        </div>
    );
}

interface MemoryPickerProps {
    selected: AttachedMemory[];
    onToggle: (memory: AttachedMemory) => void;
    providerLabel: string;
    onClose: () => void;
}

function MemoryPicker({ selected, onToggle, providerLabel, onClose }: MemoryPickerProps) {
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<MemoryCard[]>([]);
    const [loading, setLoading] = useState(false);
    const searchSeq = useRef(0);

    useEffect(() => {
        const seq = ++searchSeq.current;
        setLoading(true);
        const handle = window.setTimeout(() => {
            const request = query.trim()
                ? searchMemoryCards(query.trim(), undefined, undefined, PICKER_RESULTS)
                : listMemoryCards(PICKER_RESULTS);
            request
                .then((cards) => seq === searchSeq.current && setResults(cards))
                .catch(() => seq === searchSeq.current && setResults([]))
                .finally(() => seq === searchSeq.current && setLoading(false));
        }, query.trim() ? 200 : 0);
        return () => window.clearTimeout(handle);
    }, [query]);

    const selectedIds = useMemo(() => new Set(selected.map((m) => m.id)), [selected]);

    return (
        <div className="aw-picker" role="dialog" aria-label="Attach memories">
            <div className="aw-picker-head">
                <input
                    className="aw-picker-search"
                    value={query}
                    placeholder="Search your memories"
                    aria-label="Search your memories"
                    onChange={(event) => setQuery(event.target.value)}
                    onKeyDown={(event) => {
                        if (event.key === "Escape") {
                            event.stopPropagation();
                            onClose();
                        }
                    }}
                    autoFocus
                />
                <span className="aw-picker-count">
                    {selected.length}/{MAX_MEMORIES}
                </span>
            </div>
            <ul className="aw-picker-list" role="listbox" aria-multiselectable="true">
                {loading && results.length === 0 ? (
                    <li className="aw-picker-empty">
                        <ThinkingIndicator state="searching" size="sm" />
                    </li>
                ) : null}
                {!loading && results.length === 0 ? <li className="aw-picker-empty">No memories match.</li> : null}
                {results.map((card) => {
                    const checked = selectedIds.has(card.id);
                    return (
                        <li key={card.id}>
                            <button
                                type="button"
                                role="option"
                                aria-selected={checked}
                                className={`aw-picker-row${checked ? " is-checked" : ""}`}
                                disabled={!checked && selected.length >= MAX_MEMORIES}
                                onClick={() => onToggle(toAttached(card))}
                            >
                                <span className="aw-picker-check" aria-hidden="true" />
                                <span className="aw-picker-text">
                                    <span className="aw-picker-title">{card.title || card.window_title}</span>
                                    <span className="aw-picker-meta">
                                        {card.app_name} · {relativeTime(card.timestamp)}
                                    </span>
                                </span>
                            </button>
                        </li>
                    );
                })}
            </ul>
            <p className="aw-picker-note">Attached memories are sent to {providerLabel} with your next message.</p>
        </div>
    );
}

interface AgentSetupProps {
    hermes: HermesBridgeStatus;
    onSaved: () => Promise<void>;
    onInstalled: () => Promise<void>;
}

function AgentSetup({ hermes, onSaved, onInstalled }: AgentSetupProps) {
    const [provider, setProvider] = useState<Provider>(
        isProvider(hermes.provider_kind) ? hermes.provider_kind : "codex",
    );
    const [model, setModel] = useState(hermes.model_name ?? "");
    const [baseUrl, setBaseUrl] = useState(hermes.base_url ?? "");
    const [apiKey, setApiKey] = useState("");
    const [codex, setCodex] = useState<CodexAccountStatus | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const chooseProvider = (next: Provider) => {
        setProvider(next);
        setError(null);
        setModel(hermes.provider_kind === next && hermes.model_name ? hermes.model_name : next === "ollama" ? (hermes.ollama_models[0] ?? "") : "");
        setBaseUrl(hermes.provider_kind === next && hermes.base_url ? hermes.base_url : next === "ollama" ? hermes.ollama_base_url : "");
    };

    const onCodexStatus = useCallback((status: CodexAccountStatus) => {
        setCodex(status);
        setModel((current) =>
            status.models.some((m) => m.id === current)
                ? current
                : (status.models.find((m) => m.isDefault) ?? status.models[0])?.id ?? current,
        );
    }, []);

    const run = async (action: () => Promise<unknown>) => {
        setBusy(true);
        setError(null);
        try {
            await action();
        } catch (reason) {
            setError(reason instanceof Error ? reason.message : String(reason));
        } finally {
            setBusy(false);
        }
    };

    if (!hermes.installed && !hermes.bundled_repo_available) {
        return (
            <div className="aw-setup">
                <h3>Install Hermes</h3>
                <p>FNDR's agent runs on Hermes, an open-source agent runtime. Install it once, then pick a model.</p>
                <code className="aw-code">{hermes.install_command}</code>
                {error ? <p className="aw-error" role="alert">{error}</p> : null}
                <div className="aw-setup-actions">
                    <button type="button" className="aw-primary" disabled={busy} onClick={() => void run(async () => {
                        await installHermesBridge();
                        await onInstalled();
                    })}>
                        {busy ? "Installing…" : "Install Hermes"}
                    </button>
                </div>
            </div>
        );
    }

    const canSave =
        !busy &&
        model.trim().length > 0 &&
        (provider !== "codex" || !!codex?.usableForHermes) &&
        (provider !== "openrouter" || apiKey.trim().length > 0) &&
        (provider !== "custom" || baseUrl.trim().length > 0);

    return (
        <div className="aw-setup">
            <h3>Choose a model</h3>
            <SegmentedControl
                className="aw-provider-toggle"
                ariaLabel="Model provider"
                value={provider}
                onChange={chooseProvider}
                options={(["codex", "ollama", "openrouter", "custom"] as Provider[]).map((value) => ({
                    value,
                    label: PROVIDER_LABEL[value],
                }))}
            />

            {provider === "codex" ? <CodexAccountCard onStatusChange={onCodexStatus} /> : null}
            {provider === "ollama" && !hermes.ollama_reachable ? (
                <p className="aw-hint">
                    {hermes.ollama_installed ? "Ollama isn't running. Open Ollama, then come back." : "Install Ollama from ollama.com to run models on this Mac."}
                </p>
            ) : null}

            <label className="aw-field">
                <span>Model</span>
                {provider === "codex" && codex && codex.models.length > 0 ? (
                    <select value={model} onChange={(event) => setModel(event.target.value)}>
                        {codex.models.map((m) => (
                            <option key={m.id} value={m.id}>
                                {m.displayName}
                            </option>
                        ))}
                    </select>
                ) : provider === "ollama" && hermes.ollama_models.length > 0 ? (
                    <select value={model} onChange={(event) => setModel(event.target.value)}>
                        {hermes.ollama_models.map((m) => (
                            <option key={m} value={m}>
                                {m}
                            </option>
                        ))}
                    </select>
                ) : (
                    <input
                        value={model}
                        placeholder={provider === "codex" ? "Sign in to choose from your plan's models" : provider === "openrouter" ? "openai/gpt-5-mini" : "model name"}
                        onChange={(event) => setModel(event.target.value)}
                    />
                )}
            </label>

            {provider === "custom" || provider === "ollama" ? (
                <label className="aw-field">
                    <span>{provider === "ollama" ? "Ollama URL" : "Base URL"}</span>
                    <input value={baseUrl} placeholder="http://localhost:8000/v1" onChange={(event) => setBaseUrl(event.target.value)} />
                </label>
            ) : null}
            {provider === "openrouter" || provider === "custom" ? (
                <label className="aw-field">
                    <span>{provider === "openrouter" ? "OpenRouter API key" : "API key (optional)"}</span>
                    <input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} />
                </label>
            ) : null}

            {error ? <p className="aw-error" role="alert">{error}</p> : null}
            <div className="aw-setup-actions">
                <button
                    type="button"
                    className="aw-primary"
                    disabled={!canSave}
                    onClick={() =>
                        void run(async () => {
                            await saveHermesSetup({
                                provider_kind: provider,
                                model_name: model.trim(),
                                api_key: provider === "openrouter" || provider === "custom" ? apiKey : null,
                                base_url: provider === "custom" || provider === "ollama" ? baseUrl.trim() : null,
                            });
                            setApiKey("");
                            await onSaved();
                        })
                    }
                >
                    {busy ? "Saving…" : "Save"}
                </button>
            </div>
        </div>
    );
}

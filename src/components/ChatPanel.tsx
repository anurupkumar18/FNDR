import { useEffect, useRef, useState } from "react";
import { ChatMessage, chatWithGemma } from "../api/tauri";
import "./ChatPanel.css";

interface ChatPanelProps {
    isVisible: boolean;
    onClose: () => void;
    modelId?: string | null;
}

export function ChatPanel({ isVisible, onClose, modelId }: ChatPanelProps) {
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [draft, setDraft] = useState("");
    const [isThinking, setIsThinking] = useState(false);
    const bottomRef = useRef<HTMLDivElement>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);

    // Scroll to bottom whenever messages change.
    useEffect(() => {
        bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages, isThinking]);

    // Focus input when panel opens.
    useEffect(() => {
        if (isVisible) {
            setTimeout(() => textareaRef.current?.focus(), 80);
        }
    }, [isVisible]);

    if (!isVisible) return null;

    async function send() {
        const text = draft.trim();
        if (!text || isThinking) return;

        const userMsg: ChatMessage = { role: "user", content: text };
        const nextHistory = [...messages, userMsg];
        setMessages(nextHistory);
        setDraft("");
        setIsThinking(true);

        try {
            const reply = await chatWithGemma(nextHistory);
            setMessages([...nextHistory, { role: "assistant", content: reply }]);
        } catch (err) {
            setMessages([
                ...nextHistory,
                { role: "assistant", content: `Error: ${String(err)}` },
            ]);
        } finally {
            setIsThinking(false);
            setTimeout(() => textareaRef.current?.focus(), 50);
        }
    }

    function handleKey(e: React.KeyboardEvent<HTMLTextAreaElement>) {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            void send();
        }
    }

    // Auto-resize textarea.
    function handleInput(e: React.ChangeEvent<HTMLTextAreaElement>) {
        setDraft(e.target.value);
        const el = e.target;
        el.style.height = "auto";
        el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
    }

    const modelLabel = modelId ?? "Llama";

    return (
        <div className="chat-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
            <div className="chat-panel">
                {/* Header */}
                <div className="chat-header">
                    <div className="chat-header-title">
                        <span className={`chat-model-dot ${isThinking ? "loading" : ""}`} />
                        <div>
                            <h2>Chat with {modelLabel}</h2>
                            <div className="chat-header-sub">Local · private · no cloud</div>
                        </div>
                    </div>
                    <button className="chat-close-btn" onClick={onClose} aria-label="Close">×</button>
                </div>

                {/* Messages */}
                <div className="chat-messages">
                    {messages.length === 0 && !isThinking && (
                        <div className="chat-empty">
                            <div className="chat-empty-icon">💬</div>
                            <p>Ask {modelLabel} anything.<br />Runs fully on-device.</p>
                        </div>
                    )}

                    {messages.map((msg, i) => (
                        <div key={i} className={`chat-msg ${msg.role}`}>
                            <div className="chat-msg-label">
                                {msg.role === "user" ? "You" : modelLabel}
                            </div>
                            <div className="chat-bubble">{msg.content}</div>
                        </div>
                    ))}

                    {isThinking && (
                        <div className="chat-msg assistant chat-typing">
                            <div className="chat-msg-label">{modelLabel}</div>
                            <div className="chat-bubble">
                                <span className="chat-dot" />
                                <span className="chat-dot" />
                                <span className="chat-dot" />
                            </div>
                        </div>
                    )}

                    <div ref={bottomRef} />
                </div>

                {/* Input */}
                <div className="chat-input-row">
                    {messages.length > 0 && (
                        <button
                            className="chat-clear-btn"
                            onClick={() => setMessages([])}
                            title="Clear conversation"
                        >
                            Clear
                        </button>
                    )}
                    <textarea
                        ref={textareaRef}
                        className="chat-textarea"
                        placeholder="Message Llama… (Enter to send, Shift+Enter for newline)"
                        value={draft}
                        onChange={handleInput}
                        onKeyDown={handleKey}
                        disabled={isThinking}
                        rows={1}
                    />
                    <button
                        className="chat-send-btn"
                        onClick={() => void send()}
                        disabled={!draft.trim() || isThinking}
                    >
                        {isThinking ? "…" : "Send"}
                    </button>
                </div>
            </div>
        </div>
    );
}

import { useState } from "react";
import { SearchResult } from "../api/tauri";
import "./MemoryCard.css";

interface MemoryCardProps {
    result: SearchResult;
    query: string;
}

export function MemoryCard({ result, query }: MemoryCardProps) {
    const [isExpanded, setIsExpanded] = useState(false);

    const formatTime = (ts: number) => {
        return new Date(ts).toLocaleTimeString(undefined, {
            hour: "2-digit",
            minute: "2-digit",
        });
    };

    // Highlight the search query in the text
    const renderHighlightedText = (text: string, highlight: string) => {
        if (!highlight.trim()) return text;

        try {
            const parts = text.split(new RegExp(`(${highlight})`, "gi"));
            return parts.map((part, i) =>
                part.toLowerCase() === highlight.toLowerCase() ? (
                    <span key={i} className="highlight">
                        {part}
                    </span>
                ) : (
                    part
                )
            );
        } catch (e) {
            return text;
        }
    };

    // Clean up text for the snippet to make it "friendly"
    // Removes multiple newlines and trims noise
    const cleanSnippet = (text: string) => {
        return text
            .replace(/\n\s*\n/g, "\n") // collapse multiple newlines
            .trim();
    };

    return (
        <div className={`memory-card ${isExpanded ? "expanded" : ""}`}>
            <div className="card-header" onClick={() => setIsExpanded(!isExpanded)}>
                <div className="app-info">
                    <span className="app-icon">📱</span>
                    <div className="app-details">
                        <span className="app-name">{result.app_name}</span>
                        <span className="window-title">{result.window_title}</span>
                    </div>
                </div>
                <div className="card-meta">
                    <span className="timestamp">{formatTime(result.timestamp)}</span>
                    <span className="expand-icon">{isExpanded ? "▼" : "▶"}</span>
                </div>
            </div>

            <div className="card-content">
                {renderHighlightedText(cleanSnippet(result.snippet), query)}
            </div>

            {isExpanded && (
                <div className="card-expanded">
                    <div className="full-text">
                        {renderHighlightedText(result.text, query)}
                    </div>
                    <div className="card-actions">
                        <span className="score">Match: {Math.round(result.score * 100)}%</span>
                        <button
                            className="copy-btn"
                            onClick={() => navigator.clipboard.writeText(result.text)}
                        >
                            Copy Text
                        </button>
                    </div>
                </div>
            )}
        </div>
    );
}

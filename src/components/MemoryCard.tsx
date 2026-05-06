import { MemoryCard as MemoryCardData } from "../api/tauri";
import { splitByAnchorTerms } from "../lib/highlight";
import { extractAnchorTerms, scoreAnchorCoverage } from "../lib/search";
import "./MemoryCard.css";

interface MemoryCardProps {
    result: MemoryCardData;
    query: string;
    onClick: () => void;
    isLarge?: boolean;
}

export function MemoryCard({ result, query, onClick, isLarge }: MemoryCardProps) {
    const formatTime = (ts: number) => {
        return new Date(ts).toLocaleTimeString(undefined, {
            hour: "2-digit",
            minute: "2-digit",
        });
    };

    const anchorTerms = extractAnchorTerms(query);
    const summaryText = result.display_summary ?? result.summary;
    const coverage = result.anchor_coverage_score
        ?? scoreAnchorCoverage(`${result.title} ${summaryText}`, anchorTerms);
    const relevanceLabel = coverage >= 0.8
        ? "Direct match"
        : coverage >= 0.4
            ? "Related"
            : "Contextual";

    // Get app icon based on name
    const getAppIcon = (appName: string) => {
        const name = appName.toLowerCase();
        if (name.includes("chrome") || name.includes("safari") || name.includes("firefox")) return "🌐";
        if (name.includes("code") || name.includes("cursor")) return "💻";
        if (name.includes("slack") || name.includes("discord")) return "💬";
        if (name.includes("mail") || name.includes("outlook")) return "📧";
        if (name.includes("notes") || name.includes("notion")) return "📝";
        if (name.includes("finder")) return "📁";
        if (name.includes("terminal") || name.includes("iterm")) return "⌨️";
        if (name.includes("figma") || name.includes("sketch")) return "🎨";
        return "📱";
    };

    return (
        <article
            className={`memory-card ${isLarge ? "large" : ""}`}
            onClick={onClick}
            tabIndex={0}
            onKeyDown={(e) => e.key === "Enter" && onClick()}
        >
            <header className="card-header">
                <div className="card-app-icon">
                    {getAppIcon(result.app_name)}
                </div>
                <div className="card-meta">
                    <span className="card-app-name">{result.app_name}</span>
                    <time className="card-time">{formatTime(result.timestamp)}</time>
                </div>
                <span className={`card-relevance ${coverage >= 0.8 ? "high" : coverage >= 0.4 ? "medium" : "low"}`}>
                    {relevanceLabel}
                </span>
            </header>

            <div className="card-body">
                <p className="card-text">
                    {splitByAnchorTerms(summaryText, anchorTerms).map((part, index) =>
                        part.highlighted ? (
                            <mark key={index} className="highlight">{part.text}</mark>
                        ) : (
                            <span key={index}>{part.text}</span>
                        )
                    )}
                </p>
            </div>

            <footer className="card-footer">
                <div className="card-window-title" title={result.title}>
                    {result.title}
                </div>
            </footer>
        </article>
    );
}

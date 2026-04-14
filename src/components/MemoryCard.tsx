import { MemoryCard as MemoryCardData } from "../api/tauri";
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

    // Highlight matching text
    const highlightText = (text: string, highlight: string) => {
        if (!highlight.trim()) return text;

        try {
            const regex = new RegExp(`(${highlight.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, "gi");
            const parts = text.split(regex);

            return parts.map((part, i) =>
                regex.test(part) ? (
                    <mark key={i} className="highlight">{part}</mark>
                ) : part
            );
        } catch {
            return text;
        }
    };

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
            </header>

            <div className="card-body">
                <p className="card-text">
                    {highlightText(result.summary, query)}
                </p>
            </div>

            <footer className="card-footer">
                <div className="card-window-title" title={result.title}>
                    {result.title}
                </div>
                {result.url && (
                    <a
                        className="card-url"
                        href={result.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        onClick={(e) => e.stopPropagation()}
                        title={result.url}
                    >
                        🔗 Open Site
                    </a>
                )}
            </footer>
        </article>
    );
}

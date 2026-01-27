import { useState } from "react";
import { askFndr } from "../api/tauri";
import "./AskFndr.css";

export function AskFndr() {
    const [query, setQuery] = useState("");
    const [answer, setAnswer] = useState<string | null>(null);
    const [isLoading, setIsLoading] = useState(false);

    const handleAsk = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!query.trim() || isLoading) return;

        setIsLoading(true);
        setAnswer(null);

        try {
            const res = await askFndr(query);
            setAnswer(res);
        } catch (error) {
            console.error("Ask FNDR failed:", error);
            setAnswer("Sorry, I encountered an error while searching your memories.");
        } finally {
            setIsLoading(false);
        }
    };

    return (
        <div className="ask-fndr">
            <div className="ask-header">
                <span className="ask-icon">🤖</span>
                <h3>Ask FNDR</h3>
                <p>Conversational search through your local history</p>
            </div>

            <form className="ask-form" onSubmit={handleAsk}>
                <input
                    type="text"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder="e.g., What was I watching on YouTube at 8pm yesterday?"
                    className="ask-input"
                />
                <button type="submit" disabled={isLoading} className="ask-button">
                    {isLoading ? <div className="spinner"></div> : "Ask"}
                </button>
            </form>

            {answer && (
                <div className="ask-response">
                    <div className="response-header">Answer</div>
                    <div className="response-body">{answer}</div>
                </div>
            )}
        </div>
    );
}

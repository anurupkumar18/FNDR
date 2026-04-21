// Inspired by Claude Code's skills/ system — pre-built query templates that
// act like named shortcuts, each with a description and an instantiated query.
import "./QuickSkillsPanel.css";

interface Skill {
    id: string;
    label: string;
    description: string;
    query: string;
    timeFilter?: string;
    category: "recall" | "code" | "research" | "focus";
}

const SKILLS: Skill[] = [
    // --- recall ---
    {
        id: "yesterday",
        label: "What did I do yesterday?",
        description: "Surface all significant activity from the previous day",
        query: "",
        timeFilter: "yesterday",
        category: "recall",
    },
    {
        id: "last-hour",
        label: "What was I just doing?",
        description: "Recall the last hour of screen activity",
        query: "",
        timeFilter: "1h",
        category: "recall",
    },
    {
        id: "this-week",
        label: "This week's work",
        description: "Overview of everything captured in the past 7 days",
        query: "",
        timeFilter: "7d",
        category: "recall",
    },

    // --- code ---
    {
        id: "code-i-wrote",
        label: "Code I was writing",
        description: "Find recent coding sessions across all editors",
        query: "function class import export const let var def return",
        category: "code",
    },
    {
        id: "terminal-work",
        label: "Terminal commands",
        description: "Surface shell sessions, git activity, and CLI work",
        query: "git npm brew pip docker kubectl bash zsh terminal",
        category: "code",
    },
    {
        id: "errors-bugs",
        label: "Errors & bugs",
        description: "Find debugging sessions and error messages",
        query: "error exception failed undefined null traceback stack trace",
        category: "code",
    },
    {
        id: "pr-reviews",
        label: "PRs & code review",
        description: "GitHub pull requests, diffs, and review comments",
        query: "pull request review diff merge branch commit github gitlab",
        category: "code",
    },

    // --- research ---
    {
        id: "reading",
        label: "Articles I was reading",
        description: "Recall browser-based reading sessions",
        query: "article blog post paper read reading",
        category: "research",
    },
    {
        id: "ai-ml",
        label: "AI / ML research",
        description: "Model training, papers, and ML experiments",
        query: "model training loss accuracy gradient embedding token attention transformer",
        category: "research",
    },
    {
        id: "docs",
        label: "Documentation",
        description: "API docs, README files, and technical docs browsed",
        query: "documentation docs readme api reference guide tutorial",
        category: "research",
    },
    {
        id: "slack-email",
        label: "Messages & comms",
        description: "Slack, email, and other communication captured",
        query: "slack email message reply thread channel notification",
        category: "research",
    },

    // --- focus ---
    {
        id: "meetings",
        label: "Meeting context",
        description: "Zoom, Google Meet, Teams and other meeting captures",
        query: "meeting zoom meet teams call participant agenda notes",
        category: "focus",
    },
    {
        id: "decisions",
        label: "Decisions I made",
        description: "Find moments of decision-making and planning",
        query: "decision chose decided plan approach strategy option alternative",
        category: "focus",
    },
    {
        id: "links-urls",
        label: "Links I visited",
        description: "All captured URLs and web destinations",
        query: "http https www link url website page",
        category: "focus",
    },
];

const CATEGORY_LABELS: Record<Skill["category"], string> = {
    recall: "Recall",
    code: "Code",
    research: "Research",
    focus: "Focus",
};

interface QuickSkillsPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onRunSkill: (query: string, timeFilter?: string) => void;
}

export function QuickSkillsPanel({ isVisible, onClose, onRunSkill }: QuickSkillsPanelProps) {
    if (!isVisible) return null;

    const categories = (Object.keys(CATEGORY_LABELS) as Skill["category"][]);

    return (
        <div className="qs-page">
            <header className="qs-header">
                <div>
                    <h2>Quick Skills</h2>
                    <p>Pre-built search shortcuts — click to run instantly</p>
                </div>
                <button className="ui-action-btn qs-close-btn" onClick={onClose}>
                    Close
                </button>
            </header>

            <div className="qs-body">
                {categories.map((cat) => {
                    const skills = SKILLS.filter((s) => s.category === cat);
                    return (
                        <section key={cat} className="qs-category">
                            <div className="qs-category-label">{CATEGORY_LABELS[cat]}</div>
                            <div className="qs-skill-grid">
                                {skills.map((skill) => (
                                    <button
                                        key={skill.id}
                                        className="qs-skill-card"
                                        onClick={() => {
                                            onRunSkill(skill.query, skill.timeFilter);
                                            onClose();
                                        }}
                                    >
                                        <span className="qs-skill-label">{skill.label}</span>
                                        <span className="qs-skill-desc">{skill.description}</span>
                                    </button>
                                ))}
                            </div>
                        </section>
                    );
                })}
            </div>
        </div>
    );
}

import "./PanelState.css";

export type PanelStateKind = "loading" | "empty" | "error";

interface PanelStateAction {
    label: string;
    onAction: () => void;
}

interface PanelStateProps {
    kind: PanelStateKind;
    title: string;
    description: string;
    action?: PanelStateAction;
    /** Some panels already announce the same operation through an activity trace. */
    announce?: boolean;
}

/** Shared task-state presentation for mounted FNDR destinations.
 * Data fetching and retry behavior remain owned by each destination. */
export function PanelState({ kind, title, description, action, announce = true }: PanelStateProps) {
    const liveRole = kind === "error" ? "alert" : "status";

    return (
        <section
            className={`panel-state panel-state--${kind}`}
            role={announce ? liveRole : undefined}
            aria-busy={kind === "loading" || undefined}
        >
            <h3>{title}</h3>
            <p>{description}</p>
            {action && kind !== "loading" ? (
                <button type="button" className="ui-action-btn" onClick={action.onAction}>
                    {action.label}
                </button>
            ) : null}
        </section>
    );
}

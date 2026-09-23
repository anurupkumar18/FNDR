import type { ReactNode, Ref } from "react";
import "./PanelHeader.css";

interface PanelHeaderProps {
    title: string;
    titleId?: string;
    subtitle?: ReactNode;
    subtitleId?: string;
    /** Trailing controls shown before the close button. */
    actions?: ReactNode;
    closeLabel: string;
    closeRef?: Ref<HTMLButtonElement>;
    onClose: () => void;
}

/** One header for every full-screen panel: large title, secondary subtitle,
 *  and close in the same place with the same shape on every page. */
export function PanelHeader({
    title,
    titleId,
    subtitle,
    subtitleId,
    actions,
    closeLabel,
    closeRef,
    onClose,
}: PanelHeaderProps) {
    return (
        <header className="fndr-panel-header">
            <div className="fndr-panel-heading">
                <h2 id={titleId} className="fndr-panel-title">{title}</h2>
                {subtitle && (
                    <p id={subtitleId} className="fndr-panel-subtitle">{subtitle}</p>
                )}
            </div>
            <div className="fndr-panel-actions">
                {actions}
                <button
                    ref={closeRef}
                    type="button"
                    className="fndr-panel-close"
                    onClick={onClose}
                    aria-label={closeLabel}
                >
                    <svg viewBox="0 0 12 12" aria-hidden="true">
                        <path d="M3 3l6 6M9 3l-6 6" />
                    </svg>
                </button>
            </div>
        </header>
    );
}

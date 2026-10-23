import type { Ref } from "react";
import "./FndrWrappedPanel.css";

interface ReviewResultsHeaderProps {
    titleId: string;
    heading: string;
    details?: string[];
    backLabel: string;
    onBack: () => void;
    refreshLabel: string;
    refreshingLabel: string;
    /** Only periods that include today can still change. */
    showRefresh: boolean;
    refreshing: boolean;
    onRefresh: () => void;
    exporting: boolean;
    exportDisabled: boolean;
    onExport: () => void;
    closeRef?: Ref<HTMLButtonElement>;
    onClose: () => void;
}

/** Header shared by the Day, Week and Month review results so the buttons
 *  sit in the same place, in the same order, on every period. */
export function ReviewResultsHeader({
    titleId, heading, details = [], backLabel, onBack, refreshLabel, refreshingLabel, showRefresh, refreshing, onRefresh,
    exporting, exportDisabled, onExport, closeRef, onClose,
}: ReviewResultsHeaderProps) {
    return (
        <header className="wrapped-results-header">
            <div className="wrapped-results-title">
                <h2 id={titleId}>FNDR Review</h2>
                <strong>{heading}</strong>
                {details.length > 0 && <div>{details.map((line) => <span key={line}>{line}</span>)}</div>}
            </div>
            <div className="wrapped-results-actions">
                <button type="button" className="ui-action-btn wrapped-change-week-btn" onClick={onBack}>{backLabel}</button>
                {showRefresh && (
                    <button type="button" className="ui-action-btn wrapped-refresh-btn" onClick={onRefresh} disabled={refreshing}>
                        {refreshing ? refreshingLabel : refreshLabel}
                    </button>
                )}
                <button type="button" className="ui-action-btn wrapped-export-btn" onClick={onExport} disabled={exportDisabled || exporting}>
                    {exporting ? "Exporting…" : "Export"}
                </button>
                <button
                    ref={closeRef}
                    type="button"
                    className="ui-action-btn wrapped-close-btn"
                    onClick={onClose}
                    aria-label="Close FNDR Review"
                >
                    <span aria-hidden="true">×</span>
                </button>
            </div>
        </header>
    );
}

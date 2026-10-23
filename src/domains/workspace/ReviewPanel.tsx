import { useEffect, useRef, useState } from "react";
import { getReviewDates } from "@/shared/ipc/tauri";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import { DailySummaryPanel } from "./DailySummaryPanel";
import { FndrWrappedPanel } from "./FndrWrappedPanel";
import { ReviewPeriodNavigation, type ReviewPeriod } from "./ReviewPeriodNavigation";
import "./FndrWrappedPanel.css";

export function reviewChoices(dates: string[], period: ReviewPeriod, today: string) {
    const local = (date: Date) => `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    const choices = new Map<string, { startDate: string; endDate: string }>();
    for (const value of dates) {
        if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value > today) continue;
        const [y, m, d] = value.split("-").map(Number);
        const start = new Date(y, m - 1, d);
        if (local(start) !== value) continue;
        const end = new Date(start);
        if (period === "week") {
            start.setDate(start.getDate() - (start.getDay() + 6) % 7);
            end.setTime(start.getTime());
            end.setDate(end.getDate() + 6);
        } else if (period === "month") {
            start.setDate(1);
            end.setMonth(end.getMonth() + 1, 0);
        }
        const startDate = local(start);
        choices.set(startDate, { startDate, endDate: local(end) > today ? today : local(end) });
    }
    return [...choices.values()].sort((a, b) => b.startDate.localeCompare(a.startDate));
}

export function ReviewPanel({ isVisible, onClose, onOpenMemoryById }: {
    isVisible: boolean; onClose: () => void; onOpenMemoryById: (id: string) => void;
}) {
    const [period, setPeriod] = useState<ReviewPeriod>("day");
    const [dates, setDates] = useState<string[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [retry, setRetry] = useState(0);
    const [selected, setSelected] = useState("");
    const [opened, setOpened] = useState(false);
    const dialog = useRef<HTMLDivElement>(null);
    const close = useRef<HTMLButtonElement>(null);
    useModalFocus(isVisible && !opened, dialog, close, onClose);
    useEffect(() => {
        if (!isVisible) { setOpened(false); return; }
        let active = true;
        setLoading(true); setError(null);
        getReviewDates().then((result) => { if (active) setDates(result); })
            .catch((e) => { if (active) setError(String(e)); })
            .finally(() => { if (active) setLoading(false); });
        return () => { active = false; };
    }, [isVisible, retry]);
    const now = new Date();
    const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
    const options = reviewChoices(dates, period, today);
    const choice = options.find((o) => o.startDate === selected) ?? options[0];
    const back = () => setOpened(false);
    if (!isVisible) return null;
    if (opened && choice) return period === "day"
        ? <DailySummaryPanel isVisible onClose={onClose} onOpenMemoryById={onOpenMemoryById} reviewDate={choice.startDate} onBackToReview={back} />
        : <FndrWrappedPanel isVisible onClose={onClose} reviewRange={choice} reviewPeriod={period} onBackToReview={back} />;
    return <div className="wrapped-page" role="dialog" aria-modal="true" aria-labelledby="review-title" ref={dialog} tabIndex={-1}>
        <main className="wrapped-selection-page">
            <PanelHeader title="FNDR Review" titleId="review-title" subtitle="Review the work captured on this Mac." closeLabel="Close FNDR Review" closeRef={close} onClose={onClose} />
            <div className="review-period-picker"><ReviewPeriodNavigation value={period} onChange={(value) => { setPeriod(value); setSelected(""); }} /></div>
            <section className="wrapped-week-selector">
                <h3>Choose a {period}</h3>
                {loading ? <p role="status">Loading available dates...</p> : error ? <div role="alert">{error}<button onClick={() => setRetry(retry + 1)}>Try again</button></div> : !choice ? <p>No captured activity yet. Reviews will appear after FNDR captures activity.</p> : <>
                    <label className="review-date-picker">Available {period}s <select aria-label={`Available ${period}s`} value={choice.startDate} onChange={(e) => setSelected(e.target.value)}>
                        {options.map((o) => {
                            const dateLabel = (value: string) => new Date(`${value}T12:00:00`).toLocaleDateString(undefined, { year: "numeric", month: "long", ...(period === "month" ? {} : { day: "numeric" as const }) });
                            return <option key={o.startDate} value={o.startDate}>{period === "week" ? `${dateLabel(o.startDate)} to ${dateLabel(o.endDate)}` : dateLabel(o.startDate)}{period !== "day" && o.endDate === today ? " (so far)" : ""}</option>;
                        })}
                    </select></label>
                    <button className="wrapped-start-btn" onClick={() => setOpened(true)}>Generate {period} review</button>
                </>}
            </section>
        </main>
    </div>;
}

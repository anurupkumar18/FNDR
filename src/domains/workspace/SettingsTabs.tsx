import { useCallback, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import "./SettingsTabs.css";

export interface SettingsTab<Id extends string> {
    id: Id;
    label: string;
    /** A small count shown on the tab, such as unreviewed privacy alerts. */
    badge?: number;
}

interface SettingsTabListProps<Id extends string> {
    tabs: readonly SettingsTab<Id>[];
    active: Id;
    onChange: (id: Id) => void;
    idPrefix: string;
    label: string;
}

export const tabDomId = (prefix: string, id: string) => `${prefix}-tab-${id}`;
export const panelDomId = (prefix: string, id: string) => `${prefix}-panel-${id}`;

/**
 * The tab strip of the settings drawer: a sliding highlight under the chosen
 * tab, arrow keys (with Home and End) to move between tabs, and one tab stop.
 */
export function SettingsTabList<Id extends string>({ tabs, active, onChange, idPrefix, label }: SettingsTabListProps<Id>) {
    const listRef = useRef<HTMLDivElement>(null);
    const [thumb, setThumb] = useState<{ left: number; width: number } | null>(null);

    const measure = useCallback(() => {
        const list = listRef.current;
        const current = list?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
        if (!list || !current) return;
        setThumb((previous) =>
            previous && previous.left === current.offsetLeft && previous.width === current.offsetWidth
                ? previous
                : { left: current.offsetLeft, width: current.offsetWidth },
        );
    }, []);

    useLayoutEffect(() => {
        measure();
        const list = listRef.current;
        if (!list || typeof ResizeObserver === "undefined") return;
        const observer = new ResizeObserver(measure);
        observer.observe(list);
        return () => observer.disconnect();
    }, [measure, active, tabs]);

    const move = (event: KeyboardEvent<HTMLButtonElement>) => {
        const index = tabs.findIndex((tab) => tab.id === active);
        let next = index;
        if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
        else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
        else if (event.key === "Home") next = 0;
        else if (event.key === "End") next = tabs.length - 1;
        else return;
        event.preventDefault();
        onChange(tabs[next].id);
        listRef.current?.querySelector<HTMLElement>(`#${tabDomId(idPrefix, tabs[next].id)}`)?.focus();
    };

    return (
        <div className="settings-tabs" role="tablist" aria-label={label} aria-orientation="horizontal" ref={listRef}>
            {thumb && (
                <span
                    className="settings-tabs__thumb"
                    aria-hidden="true"
                    style={{ transform: `translateX(${thumb.left}px)`, width: thumb.width }}
                />
            )}
            {tabs.map((tab) => {
                const selected = tab.id === active;
                return (
                    <button
                        key={tab.id}
                        id={tabDomId(idPrefix, tab.id)}
                        type="button"
                        role="tab"
                        className={`settings-tabs__tab${selected ? " is-active" : ""}`}
                        aria-selected={selected}
                        aria-controls={panelDomId(idPrefix, tab.id)}
                        tabIndex={selected ? 0 : -1}
                        onClick={() => onChange(tab.id)}
                        onKeyDown={move}
                    >
                        {tab.label}
                        {tab.badge ? (
                            <span className="settings-tabs__badge" aria-label={`${tab.badge} to review`}>
                                {tab.badge}
                            </span>
                        ) : null}
                    </button>
                );
            })}
        </div>
    );
}

interface SettingsTabPanelProps {
    idPrefix: string;
    id: string;
    active: boolean;
    /** Which way the page arrives from: the tab list reads left to right. */
    direction: "forward" | "back";
    children: ReactNode;
}

/**
 * One tab's page. Every page stays mounted so a half-typed field, a running
 * download and the privacy count survive a switch; only the chosen one shows,
 * and it glides in from the side the person moved toward.
 */
export function SettingsTabPanel({ idPrefix, id, active, direction, children }: SettingsTabPanelProps) {
    return (
        <div
            id={panelDomId(idPrefix, id)}
            role="tabpanel"
            aria-labelledby={tabDomId(idPrefix, id)}
            className="settings-tabpanel"
            data-direction={direction}
            hidden={!active}
        >
            {children}
        </div>
    );
}

import { useEffect, useState } from "react";
import { dismissRoutineOffer, listNamedSets, routineOffers, type RoutineOffer, type WorkItem } from "@/shared/ipc/tauri";
import { WorkSetOpener } from "./WorkSetOpener";

/** The offer's places: its saved set's items, or its bare memories labeled by what opens. */
async function offerItems(offer: RoutineOffer): Promise<WorkItem[]> {
    if (offer.setId) {
        const set = (await listNamedSets()).find((row) => row.id === offer.setId);
        if (set) return set.items;
    }
    return offer.memoryIds.map((memoryId, index) => ({
        memoryId,
        label: `Saved place ${index + 1}`,
        kind: "url",
        reopenRank: 0,
        appName: "",
        capturedAt: 0,
    }));
}

/** "Start your usual morning set": one card for a routine due now. Always a tap, never automatic. */
export function RoutineOfferCard({ arrange = false }: { arrange?: boolean }) {
    const [offer, setOffer] = useState<RoutineOffer | null>(null);

    useEffect(() => {
        let cancelled = false;
        void routineOffers()
            .then((rows) => { if (!cancelled) setOffer(rows.find((row) => row.dueNow) ?? null); })
            .catch(() => { if (!cancelled) setOffer(null); });
        return () => { cancelled = true; };
    }, []);

    if (!offer) return null;

    return (
        <section className="home-routine" aria-labelledby="home-routine-title">
            <h2 id="home-routine-title">{offer.label}</h2>
            <p className="home-quiet">{offer.reason}</p>
            <div className="home-routine-actions">
                <WorkSetOpener name={offer.label} label="Start" arrange={arrange} load={() => offerItems(offer)} />
                <button
                    type="button"
                    className="home-routine-dismiss"
                    onClick={() => {
                        setOffer(null);
                        void dismissRoutineOffer(offer.id).catch(() => undefined);
                    }}
                >
                    Not now
                </button>
            </div>
        </section>
    );
}

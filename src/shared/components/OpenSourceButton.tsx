import { useState, type MouseEvent } from "react";
import { Button } from "@/shared/components/atoms";
import { reopenMemory } from "@/shared/ipc/tauri";
import { reopenOutcomeMessage } from "@/shared/reopenOutcome";
import "./OpenSourceButton.css";

interface OpenSourceButtonProps {
    memoryId: string;
    label?: string;
    className?: string;
    /** When set, the parent handles status text (for example Quick Find feedback). */
    onOutcome?: (message: string, outcomeKind: string) => void;
}

export function OpenSourceButton({
    memoryId,
    label = "Open source",
    className,
    onOutcome,
}: OpenSourceButtonProps) {
    const [status, setStatus] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);

    const handleClick = async (event: MouseEvent) => {
        event.stopPropagation();
        if (busy) return;
        setBusy(true);
        setStatus(null);
        try {
            const outcome = await reopenMemory(memoryId);
            const message = reopenOutcomeMessage(outcome);
            if (onOutcome) {
                onOutcome(message, outcome.kind);
            } else {
                setStatus(message);
            }
        } catch {
            const message = "Could not reopen this source. Try again.";
            if (onOutcome) {
                onOutcome(message, "error");
            } else {
                setStatus(message);
            }
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className={["open-source-button", className].filter(Boolean).join(" ")}>
            <Button mono variant="secondary" type="button" disabled={busy} onClick={handleClick}>
                {label}
            </Button>
            {!onOutcome && status ? (
                <p className="open-source-button-status" role="status">{status}</p>
            ) : null}
        </div>
    );
}

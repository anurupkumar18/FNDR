import { useState } from "react";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import {
    isSmartFillMessage,
    SMART_FILL_STATE_EVENT,
    smartFillLabel,
    type SmartFillPhase,
    type SmartFillStateEvent,
} from "./smartFillState";
import "./SmartFillGlow.css";

/**
 * The glow that follows the caret of the box Smart Fill is writing. The
 * window is moved by the backend; this page only draws the phase it is told:
 * a breathing halo while ChatGPT reads the screen, a sweeping one while the
 * words go in, a check when it is done.
 */
export function SmartFillGlow() {
    const [phase, setPhase] = useState<SmartFillPhase>("idle");
    const [message, setMessage] = useState<string | null>(null);

    useTauriEvent<SmartFillStateEvent>(SMART_FILL_STATE_EVENT, (event) => {
        setPhase(event.phase);
        setMessage(event.message);
    });

    if (phase === "idle") return null;
    const sentence = isSmartFillMessage(phase);
    return (
        <div className={`sf-glow sf-${phase}`} role="status" aria-live="polite">
            {!sentence && (
                <>
                    <span className="sf-halo" aria-hidden="true" />
                    <span className="sf-caret" aria-hidden="true" />
                </>
            )}
            <span className="sf-chip">{smartFillLabel(phase, message)}</span>
        </div>
    );
}

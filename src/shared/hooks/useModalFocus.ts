import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE_SELECTOR = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    "[tabindex]:not([tabindex='-1'])",
].join(",");

/**
 * Keeps a modal panel keyboard-contained and restores its invoker.
 * When another modal is layered above it, the topmost modal owns Escape/Tab.
 */
export function useModalFocus(
    isOpen: boolean,
    dialogRef: RefObject<HTMLElement>,
    initialFocusRef: RefObject<HTMLElement>,
    onRequestClose: () => void,
) {
    const previouslyFocusedRef = useRef<HTMLElement | null>(null);
    const onRequestCloseRef = useRef(onRequestClose);

    useEffect(() => {
        onRequestCloseRef.current = onRequestClose;
    }, [onRequestClose]);

    useEffect(() => {
        if (!isOpen) return;

        previouslyFocusedRef.current = document.activeElement instanceof HTMLElement
            ? document.activeElement
            : null;
        initialFocusRef.current?.focus();

        const handleKeyDown = (event: globalThis.KeyboardEvent) => {
            const dialog = dialogRef.current;
            if (!dialog) return;

            const eventTarget = event.target;
            const owningModal = eventTarget instanceof Element
                ? eventTarget.closest<HTMLElement>('[role="dialog"][aria-modal="true"]')
                : null;
            if (owningModal && owningModal !== dialog) return;

            if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                onRequestCloseRef.current();
                return;
            }

            if (event.key !== "Tab") return;
            const focusable = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
                .filter((element) => element.getAttribute("aria-hidden") !== "true");
            if (focusable.length === 0) {
                event.preventDefault();
                dialog.focus();
                return;
            }

            const first = focusable[0];
            const last = focusable[focusable.length - 1];
            const activeElement = document.activeElement;
            if (event.shiftKey && (activeElement === first || !dialog.contains(activeElement))) {
                event.preventDefault();
                last.focus();
            } else if (!event.shiftKey && activeElement === last) {
                event.preventDefault();
                first.focus();
            }
        };

        document.addEventListener("keydown", handleKeyDown, true);
        return () => {
            document.removeEventListener("keydown", handleKeyDown, true);
            const previouslyFocused = previouslyFocusedRef.current;
            if (previouslyFocused?.isConnected) {
                previouslyFocused.focus();
            }
            previouslyFocusedRef.current = null;
        };
    }, [dialogRef, initialFocusRef, isOpen]);
}


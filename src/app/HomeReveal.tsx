import { useEffect, useRef, useState, type ReactNode } from "react";

/**
 * A Home section that arrives as the person scrolls to it. Below the fold it
 * waits blurred and faint, so the page reads as one calm view with more behind
 * it; once it comes up the screen it sharpens and settles, and stays.
 * Without scroll observation, or with reduced motion, it is simply there.
 */
export function HomeReveal({ children }: { children: ReactNode }) {
    const ref = useRef<HTMLDivElement>(null);
    const [revealed, setRevealed] = useState(
        () =>
            typeof IntersectionObserver === "undefined" ||
            (typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true),
    );

    useEffect(() => {
        const node = ref.current;
        if (revealed || !node) return;
        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((entry) => entry.isIntersecting)) {
                    setRevealed(true);
                    observer.disconnect();
                }
            },
            { rootMargin: "0px 0px -26% 0px", threshold: 0.01 },
        );
        observer.observe(node);
        return () => observer.disconnect();
    }, [revealed]);

    return (
        <div ref={ref} className={`home-reveal${revealed ? " is-revealed" : ""}`}>
            {children}
        </div>
    );
}

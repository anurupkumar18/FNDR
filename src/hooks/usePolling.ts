import { useEffect } from "react";

export function usePolling(
    callback: (isMounted: () => boolean) => void | Promise<void>,
    intervalMs: number,
    enabled = true
) {
    useEffect(() => {
        if (!enabled) {
            return;
        }

        let mounted = true;

        const isMounted = () => mounted;
        const run = async () => {
            if (!mounted) {
                return;
            }
            await callback(isMounted);
        };

        void run();
        const timer = window.setInterval(() => {
            void run();
        }, intervalMs);

        return () => {
            mounted = false;
            window.clearInterval(timer);
        };
    }, [callback, enabled, intervalMs]);
}

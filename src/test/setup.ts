import "@testing-library/jest-dom/vitest";

if (typeof localStorage.getItem !== "function") {
    const storage = new Map<string, string>();
    Object.defineProperty(globalThis, "localStorage", {
        configurable: true,
        value: {
            clear: () => storage.clear(),
            getItem: (key: string) => storage.get(key) ?? null,
            key: (index: number) => Array.from(storage.keys())[index] ?? null,
            removeItem: (key: string) => storage.delete(key),
            setItem: (key: string, value: string) => storage.set(key, value),
            get length() {
                return storage.size;
            },
        },
    });
}

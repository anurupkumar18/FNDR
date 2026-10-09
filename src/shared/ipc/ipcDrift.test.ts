/**
 * Keeps the IPC surface honest. A backend command nothing calls, or a
 * wrapper nothing imports, is dead surface: it rots, and any script in the
 * webview can still invoke the command.
 *
 * - A call to a command that is not registered fails outright.
 * - Unused wrappers and uncalled commands are listed in
 *   `ipcDriftBaseline.ts`. The list may only shrink: a new one fails,
 *   and so does an entry that is no longer true (delete it from the file).
 */
// Vitest runs this in Node; the app tsconfig leaves Node's types out on purpose.
// @ts-expect-error -- node:fs exists in the test runtime.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { baseline } from "./ipcDriftBaseline";

const ROOT: string = (globalThis as typeof globalThis & { process: { cwd: () => string } }).process.cwd();
const join = (...parts: string[]): string => parts.join("/");
const INVOKE = /invoke(?:<[^(]*>)?\(\s*["']([a-z_0-9]+)["']/gs;

function sourceFiles(dir: string): string[] {
    return (readdirSync(dir) as string[]).flatMap((name) => {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) return sourceFiles(path);
        return /\.tsx?$/.test(name) ? [path] : [];
    });
}

function isTest(path: string): boolean {
    return path.includes(".test.") || path.includes("__tests__");
}

/** Command names inside `tauri::generate_handler![ ... ]` in main.rs. */
function registeredCommands(): Set<string> {
    const main = readFileSync(join(ROOT, "src-tauri/src/main.rs"), "utf8") as string;
    const open = main.indexOf("[", main.indexOf("generate_handler!"));
    let depth = 0;
    let close = open;
    for (let i = open; i < main.length; i += 1) {
        if (main[i] === "[") depth += 1;
        if (main[i] === "]" && --depth === 0) {
            close = i;
            break;
        }
    }
    const names = [...main.slice(open + 1, close).matchAll(/^\s*([a-z_0-9:]+),\s*$/gm)];
    return new Set(names.map((match) => match[1].split("::").pop() as string));
}

const files = sourceFiles(join(ROOT, "src")).filter((path) => !isTest(path));
const wrapperFile = join(ROOT, "src/shared/ipc/tauri.ts");
// The browser preview answers commands by name; it is not a caller.
const callers = files.filter((path) => !path.endsWith("dev/previewIpc.ts"));
const read = (path: string): string => readFileSync(path, "utf8") as string;
const registered = registeredCommands();
const invoked = new Set(
    callers.flatMap((path) => [...read(path).matchAll(INVOKE)].map((match) => match[1])),
);

describe("IPC surface", () => {
    it("finds the registration block", () => {
        expect(registered.size).toBeGreaterThan(100);
    });

    it("never calls a command the backend does not register", () => {
        expect([...invoked].filter((name) => !registered.has(name)).sort()).toEqual([]);
    });

    it("has no backend command without a caller beyond the recorded ones", () => {
        const uncalled = [...registered].filter((name) => !invoked.has(name)).sort();
        expect(uncalled).toEqual([...baseline.uncalledCommands].sort());
    });

    it("has no unused wrapper beyond the recorded ones", () => {
        const wrappers = [
            ...read(wrapperFile).matchAll(/export (?:async )?function (\w+)\(/g),
        ].map((match) => match[1]);
        const elsewhere = callers
            .filter((path) => path !== wrapperFile)
            .map((path) => read(path))
            .join("\n");
        const unused = wrappers.filter((name) => !new RegExp(`\\b${name}\\b`).test(elsewhere)).sort();
        expect(unused).toEqual([...baseline.unusedWrappers].sort());
    });
});

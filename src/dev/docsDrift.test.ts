/**
 * The files agents and teammates read first must not point at things that
 * are gone. Checks every local link and every backticked repo path in the
 * governed files, and that each command `docs/agent.md` lists is registered.
 */
// Vitest runs this in Node; the app tsconfig leaves Node's types out on purpose.
// @ts-expect-error -- node:fs exists in the test runtime.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ROOT: string = (globalThis as typeof globalThis & { process: { cwd: () => string } }).process.cwd();
const read = (path: string): string => readFileSync(`${ROOT}/${path}`, "utf8") as string;
const exists = (path: string): boolean => existsSync(`${ROOT}/${path}`) as boolean;

/** Add a file here when it becomes something people are told to read first. */
const GOVERNED = [
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "docs/README.md",
    "docs/CONTEXT.md",
    "docs/agent.md",
    "docs/team/TEAM.md",
];

function normalize(path: string): string {
    const out: string[] = [];
    for (const part of path.split("/")) {
        if (part === "..") out.pop();
        else if (part !== "." && part !== "") out.push(part);
    }
    return out.join("/");
}

function missingReferences(file: string): string[] {
    const text = read(file);
    const dir = file.includes("/") ? file.slice(0, file.lastIndexOf("/")) : "";
    const missing: string[] = [];
    for (const [, target] of text.matchAll(/\]\(([^)#\s]+)(?:#[^)]*)?\)/g)) {
        if (/^[a-z]+:/.test(target)) continue;
        if (!exists(normalize(`${dir}/${target}`))) missing.push(target);
    }
    const paths = /`([A-Za-z0-9_.\-/]+\/[A-Za-z0-9_.\-/]+\.(?:md|rs|ts|tsx|json|py|sh|yml|toml))`/g;
    for (const [, target] of text.matchAll(paths)) {
        // Docs name code by its path under the crate or the app as often as from the root.
        const roots = ["", `${dir}/`, "src-tauri/src/", "src-tauri/", "src/"];
        if (!roots.some((root) => exists(normalize(`${root}${target}`)))) missing.push(target);
    }
    return [...new Set(missing)].sort();
}

describe("instruction docs", () => {
    it.each(GOVERNED)("%s points only at files that exist", (file) => {
        expect(missingReferences(file)).toEqual([]);
    });

    it("docs/agent.md lists only commands the backend registers", () => {
        const main = read("src-tauri/src/main.rs");
        const table = read("docs/agent.md").split("### Commands the page calls")[1]?.split("\n### ")[0] ?? "";
        const listed = [...table.matchAll(/`([a-z_]+)`/g)]
            .map((match) => match[1])
            .filter((name) => name.includes("_") && !name.endsWith(".rs"));
        expect(listed.length).toBeGreaterThan(5);
        expect(listed.filter((name) => !new RegExp(`::${name},`).test(main))).toEqual([]);
    });
});

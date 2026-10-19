/**
 * The release adds Files search's model helper to the bundle through
 * `src-tauri/tauri.files.conf.json` (ADR 033). Tauri merges config with a
 * JSON merge patch, which replaces arrays whole, so that file repeats every
 * external binary: it must stay the main list plus the helper.
 */
// Vitest runs this in Node; the app tsconfig leaves Node's types out on purpose.
// @ts-expect-error -- node:fs exists in the test runtime.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ROOT: string = (globalThis as typeof globalThis & { process: { cwd: () => string } }).process.cwd();
const externalBin = (file: string): string[] =>
    JSON.parse(readFileSync(`${ROOT}/src-tauri/${file}`, "utf8") as string).bundle.externalBin;

describe("Files search bundle", () => {
    it("adds only the helper to the app's external binaries", () => {
        expect(externalBin("tauri.files.conf.json")).toEqual([
            ...externalBin("tauri.conf.json"),
            "binaries/fndr-files-helper",
        ]);
    });
});

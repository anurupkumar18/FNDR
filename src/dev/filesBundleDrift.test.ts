/**
 * The release adds Files search's model helper to the bundle through
 * `src-tauri/tauri.files.conf.json` (ADR 039). Tauri merges config with a
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
    it("builds the helper before starting the development server", () => {
        const config = JSON.parse(readFileSync(`${ROOT}/src-tauri/tauri.conf.json`, "utf8") as string);
        expect(config.build.beforeDevCommand).toBe(
            "make files-helper && npm run dev -- --host 127.0.0.1 --port 1420",
        );
    });

    it("builds the helper before both release builds and applies the bundle config", () => {
        const release = readFileSync(`${ROOT}/.github/workflows/release.yml`, "utf8") as string;
        const helper = release.indexOf("run: make files-helper");
        const builds = [...release.matchAll(/uses: tauri-apps\/tauri-action@v0/g)];
        expect(helper).toBeGreaterThan(-1);
        expect(builds).toHaveLength(2);
        for (const build of builds) expect(build.index).toBeGreaterThan(helper);
        expect(release.match(/args: --config src-tauri\/tauri.files.conf.json/g)).toHaveLength(2);
    });

    it("adds only the helper to the app's external binaries", () => {
        expect(externalBin("tauri.files.conf.json")).toEqual([
            ...externalBin("tauri.conf.json"),
            "binaries/fndr-files-helper",
        ]);
    });
});

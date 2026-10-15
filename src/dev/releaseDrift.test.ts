/**
 * A tag push must be able to build what `main` builds. On 2026-10-13 the
 * v0.3.5 release failed because the release job ran on an older macOS image
 * than the test job: the speech helper needs the macOS 26 SDK to compile.
 * The public download link and the updater manifest both returned 404.
 */
// Vitest runs this in Node; the app tsconfig leaves Node's types out on purpose.
// @ts-expect-error -- node:fs exists in the test runtime.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ROOT: string = (globalThis as typeof globalThis & { process: { cwd: () => string } }).process.cwd();
const read = (path: string): string => readFileSync(`${ROOT}/${path}`, "utf8") as string;

/** The text of one job, from its key to the next job at the same indent. */
function job(workflow: string, name: string): string {
    const start = workflow.indexOf(`\n  ${name}:\n`);
    if (start < 0) return "";
    const rest = workflow.slice(start + 1);
    const next = rest.slice(1).search(/\n  [a-z][a-z0-9-]*:\n/);
    return next < 0 ? rest : rest.slice(0, next + 1);
}

const runner = (text: string): string => text.match(/runs-on:\s*(\S+)/)?.[1] ?? "";

describe("release workflow", () => {
    const release = read(".github/workflows/release.yml");
    const test = read(".github/workflows/test.yml");

    it("builds the app on the image the Rust tests run on", () => {
        const tested = runner(job(test, "rust"));
        expect(tested).toMatch(/^macos-/);
        expect(runner(job(release, "build-release"))).toBe(tested);
    });

    it("prepares the same native dependencies for tests, retrieval and release", () => {
        const retrieval = read(".github/workflows/retrieval-gate.yml");
        for (const build of [job(test, "rust"), job(retrieval, "gate"), job(release, "build-release")]) {
            expect(build).toContain("bash scripts/bootstrap/prepare-macos-ci.sh");
        }
    });

    it("can be run by hand without publishing", () => {
        expect(release).toMatch(/\n {2}workflow_dispatch:/);
        const build = job(release, "build-release");
        // tauri-action publishes only when it is given a tag, and it gets one only on a tag push.
        const tags = build.match(/tagName:[^\n]*/g) ?? [];
        expect(tags.length).toBe(2);
        for (const tag of tags) expect(tag).toContain("github.event_name == 'push' && github.ref_name || ''");
        const upload = build.split(/\n {6}- name: /).find((step) => step.includes("gh release upload")) ?? "";
        expect(upload).toMatch(/\n {8}if: github\.event_name == 'push'\n/);
    });

    it("refuses a tag that does not name the app version", () => {
        expect(job(release, "verify")).toMatch(/"v\$version" = "\$GITHUB_REF_NAME"/);
    });
});

describe("app version", () => {
    it("is the same in every file that states it", () => {
        const versions = {
            "package.json": JSON.parse(read("package.json")).version,
            "package-lock.json": JSON.parse(read("package-lock.json")).version,
            "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
            "src-tauri/Cargo.toml": read("src-tauri/Cargo.toml").match(/^version = "([^"]+)"/m)?.[1],
            "src-tauri/Cargo.lock": read("src-tauri/Cargo.lock").match(/name = "fndr"\nversion = "([^"]+)"/)?.[1],
        };
        const stated = new Set(Object.values(versions));
        expect(stated.size, JSON.stringify(versions)).toBe(1);
    });
});

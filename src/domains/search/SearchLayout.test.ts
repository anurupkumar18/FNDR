// Vitest executes this test in Node; the application tsconfig intentionally omits Node ambient types.
// @ts-expect-error -- node:fs exists in the test runtime.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const testProcess = (globalThis as typeof globalThis & {
    process: { cwd: () => string };
}).process;
const appCss = readFileSync(`${testProcess.cwd()}/src/app/styles/App.css`, "utf8");
const searchCss = readFileSync(`${testProcess.cwd()}/src/domains/search/SearchBar.css`, "utf8");
const homeCss = readFileSync(`${testProcess.cwd()}/src/app/HomeHero.css`, "utf8");
const timelineCss = readFileSync(`${testProcess.cwd()}/src/domains/timeline/Timeline.css`, "utf8");

describe("search layout containment", () => {
    it("lets the memory mention popup escape the search pill", () => {
        expect(searchCss).toMatch(
            /\.search-bar\s*\{[^}]*overflow:\s*visible\s*;/s,
        );
    });

    it("gives search results a bounded vertical scroll owner", () => {
        expect(appCss).toMatch(
            /\.app-main\s*\{[^}]*display:\s*flex\s*;[^}]*flex-direction:\s*column\s*;/s,
        );
        expect(appCss).toMatch(
            /\.main-layout\s*\{[^}]*flex:\s*1\s*;[^}]*min-height:\s*0\s*;[^}]*overflow-y:\s*auto\s*;/s,
        );
    });

    it("lets the primary recall path reflow without subtracting a desktop sidebar width", () => {
        expect(searchCss).toMatch(/\.search-panel\s*\{[^}]*width:\s*min\(760px,\s*100%\)\s*;/s);
        expect(searchCss).not.toContain("calc(100vw - 260px)");
        expect(homeCss).toContain("@media (max-width: 640px)");
        expect(timelineCss).toContain("@media (max-width: 640px)");
    });

    it("keeps primary recall controls at least 44px tall", () => {
        expect(searchCss).toMatch(/\.search-clear\s*\{[^}]*min-height:\s*44px\s*;/s);
        expect(searchCss).toMatch(/\.filter-select\s*\{[^}]*min-height:\s*44px\s*;/s);
        expect(timelineCss).toMatch(/\.timeline-delete-btn,[\s\S]*?min-height:\s*44px\s*;/);
    });
});

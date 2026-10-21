import { defineConfig } from "@playwright/test";

export default defineConfig({
    testDir: "tests/e2e",
    testMatch: "**/*.e2e.ts",
    timeout: 30_000,
    expect: { timeout: 10_000 },
    workers: 2,
    reporter: "list",
    outputDir: "test-results/e2e",
    use: {
        baseURL: "http://127.0.0.1:4173",
        browserName: "chromium",
        viewport: { width: 1440, height: 900 },
        trace: "retain-on-failure",
    },
    projects: [
        { name: "light-pointer" },
        { name: "dark-keyboard" },
    ],
    webServer: {
        command: "npm run dev -- --port 4173 --strictPort",
        url: "http://127.0.0.1:4173/ui-preview.html",
        reuseExistingServer: false,
        timeout: 120_000,
    },
});

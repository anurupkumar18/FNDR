import { expect, test, type Locator, type Page, type TestInfo } from "@playwright/test";

const theme = (project: string) => project.startsWith("light") ? "light" : "dark";
const keyboard = (project: string) => project.endsWith("keyboard");

async function activate(locator: Locator, withKeyboard: boolean) {
    if (withKeyboard) {
        await locator.focus();
        await locator.press("Enter");
    } else {
        await locator.click();
    }
}

async function openPreview(page: Page, testInfo: TestInfo, query = "") {
    await page.goto(`/ui-preview.html?theme=${theme(testInfo.project.name)}&motion=off${query}`);
}

async function screenshot(page: Page, testInfo: TestInfo, name: string) {
    const path = testInfo.outputPath(`${name}.png`);
    await page.screenshot({ path, fullPage: true, animations: "disabled" });
    await testInfo.attach(name, { path, contentType: "image/png" });
}

test("a person can complete onboarding and reach their workspace", async ({ page }, testInfo) => {
    const keys = keyboard(testInfo.project.name);
    await openPreview(page, testInfo, "&onboarding=welcome");
    await activate(page.getByRole("button", { name: "Get Started" }), keys);
    await expect(page.getByRole("heading", { name: "How FNDR handles your data" })).toBeVisible();
    await activate(page.getByRole("button", { name: "Continue" }), keys);
    await expect(page.getByRole("heading", { name: "Lock FNDR with Touch ID" })).toBeVisible();
    await activate(page.getByRole("button", { name: "Skip for now" }), keys);
    await expect(page.getByRole("heading", { name: "Select your local AI model" })).toBeVisible();
    await activate(page.getByRole("button", { name: /^Skip for now/ }), keys);
    await expect(page.getByRole("heading", { name: "Grant a few permissions" })).toBeVisible();
    await activate(page.getByRole("button", { name: "Skip for now" }), keys);
    await activate(page.getByRole("button", { name: "Skip imports" }), keys);
    await expect(page.getByRole("textbox", { name: "Search your memories" })).toBeVisible();
    await screenshot(page, testInfo, "onboarding-complete");
});

test("a person can find a memory and open it", async ({ page }, testInfo) => {
    await openPreview(page, testInfo);
    const input = page.getByRole("textbox", { name: "Search your memories" });
    await expect(input).toBeVisible();
    if (keyboard(testInfo.project.name)) {
        await input.focus();
        await page.keyboard.type("FNDR home experience");
    } else {
        await input.fill("FNDR home experience");
    }
    await input.press("Enter");
    await expect(page.getByText("Reviewed the FNDR home experience")).toBeVisible();
    await activate(page.getByRole("button", { name: "Open sidebar" }), keyboard(testInfo.project.name));
    await activate(page.getByRole("button", { name: "Memory Vault" }), keyboard(testInfo.project.name));
    await activate(page.getByRole("button", { name: "Open memory: Reviewed the FNDR home experience" }), keyboard(testInfo.project.name));
    await expect(page.getByRole("button", { name: "Close memory details" })).toBeVisible();
    await screenshot(page, testInfo, "memory-open");
});

test("a person can pause capture and see it stay paused", async ({ page }, testInfo) => {
    const keys = keyboard(testInfo.project.name);
    await openPreview(page, testInfo);
    await activate(page.getByRole("button", { name: "Open settings" }), keys);
    await activate(page.getByRole("tab", { name: "Capture" }), keys);
    await activate(page.getByRole("button", { name: "Pause capture" }), keys);
    await expect(page.getByText(/Capture paused. It will remain paused after relaunch./)).toBeVisible();
    await activate(page.getByRole("button", { name: "Close settings" }), keys);
    await activate(page.getByRole("button", { name: "Open settings" }), keys);
    await activate(page.getByRole("tab", { name: "Capture" }), keys);
    await expect(page.getByRole("button", { name: "Resume capture" })).toBeVisible();
    await screenshot(page, testInfo, "capture-paused");
});

test("a person can enable agent notes and copy connection setup", async ({ page }, testInfo) => {
    const keys = keyboard(testInfo.project.name);
    await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
    await openPreview(page, testInfo);
    await activate(page.getByRole("button", { name: "Open settings" }), keys);
    await activate(page.getByRole("tab", { name: "AI" }), keys);
    const notes = page.getByRole("switch", { name: "Let assistants add notes" });
    await expect(notes).toBeVisible();
    if (keys) {
        await notes.focus();
        await notes.press("Space");
    } else {
        await notes.click();
    }
    await expect(notes).toBeChecked();
    const client = page.getByRole("listitem", { name: "Claude Desktop" });
    await activate(client.getByRole("button", { name: "Show setup" }), keys);
    await activate(page.getByRole("button", { name: "Copy setup" }), keys);
    await expect(page.getByText("Setup copied.")).toBeVisible();
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toContain("mcpServers");
    await screenshot(page, testInfo, "agent-setup-copied");
});

test("a person sees listening, partial, and final voice states before searching", async ({ page }, testInfo) => {
    const keys = keyboard(testInfo.project.name);
    await openPreview(page, testInfo);
    await activate(page.getByRole("button", { name: "Start voice input" }), keys);
    await expect(page.getByRole("button", { name: "Stop voice input" })).toBeVisible();
    async function voice(state: Record<string, unknown>) {
        await page.evaluate(async (payload) => {
            const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__;
            await internals.invoke("plugin:event|emit", { event: "voice://state", payload });
        }, { version: 1, sessionId: "preview-voice-1", surface: "home_search", state });
    }
    await voice({ kind: "listening", level: 0.4 });
    await expect(page.getByRole("progressbar", { name: "Microphone input level" })).toHaveAttribute("aria-valuenow", "40");
    await voice({ kind: "partial", text: "atlas planning" });
    await expect(page.getByRole("textbox", { name: "Search your memories" })).toHaveValue("atlas planning");
    await activate(page.getByRole("button", { name: "Stop voice input" }), keys);
    await voice({ kind: "final", text: "atlas planning notes" });
    await expect(page.getByRole("textbox", { name: "Search your memories" })).toHaveValue("atlas planning notes");
    await expect(page.getByText("Transcript ready: atlas planning notes")).toBeVisible();
    await screenshot(page, testInfo, "voice-transcript-ready");
});

test("a person can open the Daily Brief from To-dos", async ({ page }, testInfo) => {
    const keys = keyboard(testInfo.project.name);
    await openPreview(page, testInfo);
    await activate(page.getByRole("button", { name: "Open sidebar" }), keys);
    await activate(page.getByRole("button", { name: "To-dos" }), keys);
    await expect(page.getByText("Today's Briefing")).toBeVisible();
    await expect(page.getByText(/finish the keyboard walkthrough/i)).toBeVisible();
    await screenshot(page, testInfo, "daily-briefing");
});

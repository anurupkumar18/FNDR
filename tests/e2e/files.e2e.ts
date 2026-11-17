import { expect, test, type Locator } from "@playwright/test";

async function activate(locator: Locator, keyboard: boolean) {
    if (keyboard) {
        await locator.focus();
        await locator.press("Enter");
    } else {
        await locator.click();
    }
}

for (const accept of [false, true]) {
    test(`Files onboarding can be ${accept ? "accepted" : "declined"} without blocking memory setup`, async ({ page }, info) => {
        const theme = info.project.name.startsWith("dark") ? "dark" : "light";
        await page.goto(`/ui-preview.html?onboarding=search_files&theme=${theme}&motion=off`);
        await expect(page.getByRole("heading", { name: "Search your files" })).toBeVisible();
        await expect(page.getByText(/EmbeddingGemma 2 \(865 MB/)).toBeVisible();
        await activate(page.getByRole("button", { name: accept ? "Search these folders" : "Not now" }), info.project.name.endsWith("keyboard"));
        await expect(page.getByRole("textbox", { name: "Search your memories" })).toBeVisible();
        const state = await page.evaluate(async () => {
            const api = (window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string) => Promise<unknown> } }).__TAURI_INTERNALS__;
            return { settings: await api.invoke("files_settings_get"), status: await api.invoke("files_status") };
        });
        expect(state.settings).toMatchObject({ enabled: accept });
        expect(state.status).toMatchObject({ model_ready: accept });
        await page.screenshot({ path: info.outputPath(`files-${accept ? "accepted" : "declined"}.png`), fullPage: true });
    });
}

test("Files search filters PDF and code results and previews the matched passage", async ({ page }, info) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const theme = info.project.name.startsWith("dark") ? "dark" : "light";
    const keys = info.project.name.endsWith("keyboard");
    await page.goto(`/ui-preview.html?theme=${theme}&motion=off`);
    const input = page.getByRole("textbox", { name: "Search your memories" });
    await input.fill("atlas");
    await input.press("Enter");
    const files = page.getByRole("region", { name: "Files", exact: true });
    await expect(files).toBeVisible();
    await activate(files.getByRole("tab", { name: /^PDFs/ }), keys);
    await expect(files.getByRole("option")).toHaveCount(1);
    await expect(files.getByRole("option")).toContainText("atlas-plan.pdf");
    await activate(files.getByRole("button", { name: "Preview", exact: true }), keys);
    const preview = files.getByRole("region", { name: "Preview of atlas-plan.pdf" });
    await expect(preview.locator(".file-preview-text")).toContainText("Atlas planning: the launch review");
    await expect(preview).toContainText("p. 3");
    await activate(files.getByRole("tab", { name: /^Code/ }), keys);
    await expect(files.getByRole("option")).toHaveCount(1);
    await expect(files.getByRole("option")).toContainText("search.ts");
    await expect(files.getByRole("region", { name: "Preview of atlas-plan.pdf" })).toHaveCount(0);
    const search = page.getByRole("textbox", { name: "Search memories", exact: true });
    await search.fill("file: atlas");
    await search.press("Enter");
    await expect(files.getByRole("tab", { name: /^All/ })).toHaveAttribute("aria-selected", "true");
    await expect(files.getByRole("option")).toHaveCount(5);
    await search.fill("code: atlas");
    await search.press("Enter");
    await expect(files.getByRole("option")).toHaveCount(1);
    await expect(files.getByRole("option")).toContainText("search.ts");
    await page.screenshot({ path: info.outputPath("files-code-scope.png"), fullPage: true });
    expect(errors).toEqual([]);
});

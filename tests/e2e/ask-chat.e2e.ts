import { expect, test } from "@playwright/test";

test("Search & Ask follows the latest app radius and composer treatments", async ({ page }) => {
    await page.goto("/ui-preview.html?panel=ask&theme=light&motion=off");
    await expect(page.locator(".ask-form")).toHaveCSS("border-radius", "16px");
    await expect(page.locator(".ask-new-chat")).toHaveCSS("border-radius", "10px");
    await expect(page.locator(".ask-submit")).toHaveCSS("border-radius", "10px");
    await expect(page.locator(".ask-input")).toHaveCSS("resize", "none");
    const accent = await page.locator(".ask-submit").evaluate((el) => {
        const probe = document.createElement("span");
        probe.style.color = "var(--accent)";
        el.append(probe);
        const color = getComputedStyle(probe).color;
        probe.remove();
        return color;
    });
    await expect(page.locator(".ask-submit")).toHaveCSS("background-color", accent);
    await page.getByRole("textbox").fill("How does the browser preview work?");
    await page.getByRole("textbox").press("Enter");
    await expect(page.locator(".ask-user-message")).toHaveCSS("border-radius", "16px");
    await expect(page.locator(".ask-source").first()).toHaveCSS("border-radius", "10px");
    await expect(page.locator(".ask-source").first()).toHaveCSS("border-top-width", "0px");
});

test("Search & Ask keeps a cited conversation with a bottom composer", async ({ page }, info) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const theme = info.project.name.startsWith("dark") ? "dark" : "light";
    await page.goto(`/ui-preview.html?panel=ask&theme=${theme}&motion=off`);
    await expect(page.getByRole("heading", { name: "Where did we leave off?" })).toBeVisible();
    const input = page.getByRole("textbox", { name: "Search or ask FNDR" });
    await input.fill("How does the browser preview work?");
    await input.press("Enter");
    await expect(page.locator(".ask-answer")).toHaveCount(1);
    await input.fill("What did the FNDR design review cover?");
    await input.press("Enter");
    await expect(page.locator(".ask-answer")).toHaveCount(2);
    await expect(page.getByText("How does the browser preview work?", { exact: true })).toBeVisible();
    await expect(input).toHaveValue("");
    await expect(page.locator(".ask-source").first()).toBeVisible();
    await page.locator(".ask-source").first().click();
    await expect(page.getByRole("heading", { name: "Memory Vault", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Close Memory Vault", exact: true }).click();
    await expect(page.locator(".ask-answer")).toHaveCount(2);
    await page.screenshot({ path: info.outputPath("ask-conversation.png"), fullPage: true });
    for (const width of [1440, 768, 390, 320]) {
        await page.setViewportSize({ width, height: 900 });
        await expect(input).toBeVisible();
        const box = await page.locator(".ask-composer-area").boundingBox();
        expect(box!.y + box!.height).toBeLessThanOrEqual(901);
        expect(await page.locator(".ask-page").evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(true);
        expect(await page.getByRole("button", { name: "Search & Ask", exact: true }).evaluate((el) => {
            const rect = el.getBoundingClientRect();
            return el.contains(document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2));
        })).toBe(true);
    }
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.getByRole("button", { name: "New chat", exact: true }).click();
    await expect(page.locator(".ask-exchange")).toHaveCount(0);
    await expect(input).toBeFocused();
    expect(errors).toEqual([]);
});

test("the app sidebar opens the same Search & Ask conversation", async ({ page }) => {
    await page.goto("/ui-preview.html?motion=off");
    await page.getByRole("button", { name: "Open sidebar" }).click();
    await page.getByRole("button", { name: "Search & Ask FNDR", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Where did we leave off?" })).toBeVisible();
    await expect(page.getByRole("button", { name: "New chat", exact: true })).toBeVisible();
});

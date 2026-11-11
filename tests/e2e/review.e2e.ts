import { expect, test } from "@playwright/test";

test("reviews a day, week, and month from one destination", async ({ page }, info) => {
    await page.goto(`/ui-preview.html?motion=off&theme=${info.project.name.startsWith("dark") ? "dark" : "light"}`);
    await page.getByRole("button", { name: "Open sidebar" }).click();
    await page.getByRole("button", { name: "FNDR Review", exact: true }).click();
    for (const period of ["day", "week", "month"]) {
        await page.getByRole("button", { name: period[0].toUpperCase() + period.slice(1), exact: true }).click();
        await page.getByRole("button", { name: `Generate ${period} review` }).click();
        if (period === "day") await expect(page.getByRole("heading", { name: /Summary for/ })).toBeVisible();
        else await expect(page.getByText(`${period.toUpperCase()} OVERVIEW`).first()).toBeVisible();
        await page.screenshot({ path: info.outputPath(`${period}.png`), fullPage: true });
        await page.getByRole("button", { name: /Back to review selection/ }).click();
        await expect(page.getByRole("button", { name: "Day", exact: true })).toBeVisible();
    }
});

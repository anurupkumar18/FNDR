import assert from "node:assert/strict";
import { chromium, expect } from "@playwright/test";

const base = process.env.FNDR_SITE_URL ?? "http://127.0.0.1:5195/";
const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const external = [];
  page.on("request", (request) => { if (!request.url().startsWith(new URL(base).origin)) external.push(request.url()); });
  await page.goto(new URL("tutorials.html", base).href);
  const demoBox = await page.locator(".tour-window iframe").boundingBox();
  assert.ok(demoBox.width >= 1200 && demoBox.height >= 800, "desktop demo must be large enough to use");
  const app = page.frameLocator(".tour-window iframe");
  await expect(app.getByRole("textbox", { name: "Search your memories" })).toBeVisible({ timeout: 20000 });
  await expect(app.getByText(/Good (Morning|Afternoon|Evening|Night), Visitor!/)).toBeVisible();
  await expect(page.locator("[data-tour-status]")).toContainText("Search", { timeout: 25000 });
  await page.getByRole("button", { name: "Pause tour" }).click();
  const chapters = page.getByRole("combobox", { name: "Tutorial chapter" });
  for (const [value, heading] of [
    ["ask", "Search & Ask FNDR"], ["vault", "Memory Vault"], ["review", "FNDR Review"],
    ["todos", "To-dos"], ["outputs", "Daily outputs"], ["stats", "Activity Stats"],
    ["guide", "Screen Guide"], ["agent", "Agent"], ["activity", "Privacy activity"],
    ["settings", "Settings"], ["capture", "Settings"], ["privacy", "Settings"],
    ["voice", "Settings"], ["integrations", "Settings"],
  ]) {
    await chapters.selectOption(value);
    await expect(app.getByRole("heading", { name: heading, exact: true })).toBeVisible({ timeout: 15000 });
    const tab = { settings: "General", capture: "Capture", privacy: "Privacy", voice: "Voice", integrations: "AI" }[value];
    if (tab) await expect(app.getByRole("tab", { name: tab, selected: true })).toBeVisible({ timeout: 15000 });
    await expect(page.locator("[data-tour-status]")).not.toContainText("could not");
  }
  await page.getByRole("button", { name: "Restart tour" }).click();
  await expect(app.getByRole("textbox", { name: "Search your memories" })).toBeVisible({ timeout: 20000 });
  await app.getByRole("textbox", { name: "Search your memories" }).click();
  await expect(page.locator("[data-tour-status]")).toContainText("Tour paused");
  await app.getByRole("textbox", { name: "Search your memories" }).fill("FNDR home experience");
  await app.getByRole("textbox", { name: "Search your memories" }).press("Enter");
  await expect(app.getByText("Reviewed the FNDR home experience")).toBeVisible();
  const child = page.frames().find((frame) => frame.url().includes("demo-preview.html"));
  assert.equal(await child.evaluate(async () => {
    try { await fetch("https://example.invalid/private"); return false; } catch { return true; }
  }), true);
  assert.deepEqual(external, []);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(new URL("tutorials.html", base).href);
  await expect(page.locator("[data-tour-status]")).toContainText("demo is ready");
  await expect(page.getByRole("button", { name: "Restart tour" })).toBeVisible();
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  console.log("PASS: guided chapters, all app destinations, interruption, sample search and no external requests");
} finally {
  await browser.close();
}

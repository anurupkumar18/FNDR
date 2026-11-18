import assert from "node:assert/strict";
import { chromium, expect } from "@playwright/test";

const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  await page.goto(process.env.FNDR_SITE_URL ?? "http://127.0.0.1:5193/team.html");
  for (const name of ["Anurup Kumar", "Minh Le", "Felipe Marin", "Kunj Rathod"]) {
    const card = page.getByRole("button", { name: new RegExp(name) });
    await card.click({ force: true });
    const dialog = page.getByRole("dialog", { name });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("What they worked on")).toBeVisible();
    for (const link of ["LinkedIn", "GitHub", "Email"]) await expect(dialog.getByRole("link", { name: link })).toHaveAttribute("href", /^(https:\/\/|mailto:)/);
    const frames = await dialog.locator(".team-dialog-card").evaluate((node) => node.getAnimations().flatMap((animation) => animation.effect.getKeyframes()));
    assert.ok(frames.some((frame) => frame.transform?.includes("translate(")), "profile should fly from the portrait");
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(card).toBeFocused();
  }
  await page.emulateMedia({ reducedMotion: "reduce" });
  assert.equal(await page.locator(".team-photo.portrait").first().evaluate((node) => getComputedStyle(node).animationName), "none");
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  console.log("PASS: team profiles open, close with Escape, restore focus, and respect reduced motion on mobile");
} finally {
  await browser.close();
}

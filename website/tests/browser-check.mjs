// Optional real-browser acceptance check. Serve website/ on port 5193 first.
// Uses the repo's existing Playwright dependency, not the static Pages job.
import assert from "node:assert/strict";
import { chromium, expect } from "@playwright/test";

const base = process.env.FNDR_SITE_URL ?? "http://127.0.0.1:5193/";
const browser = await chromium.launch({ channel: "chrome", headless: true });
const releaseUrl = "https://api.github.com/repos/anurupkumar18/FNDR/releases/latest";
const assetUrl = "https://github.com/anurupkumar18/FNDR/releases/download/v0.4.0/FNDR-latest.dmg";
const errors = [];
const release = { tag_name: "v0.4.0", draft: false, prerelease: false, published_at: "2026-11-16T00:00:00Z", body: "Signed with Developer ID and notarized by Apple.", assets: [{ name: "FNDR-latest.dmg", size: 42_000_000, browser_download_url: assetUrl }] };
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
page.on("pageerror", (error) => errors.push(error.message));
const requests = [];
page.on("request", (request) => requests.push(request.url()));

try {
  await page.route(releaseUrl, (route) => route.fulfill({ json: release }));
  await page.goto(base);
  await expect(page.locator("[data-release-status]")).toHaveText("v0.4.0 is ready");
  for (const link of await page.locator("[data-download]").all()) await expect(link).toHaveAttribute("href", assetUrl);

  const card = page.locator(".slot-c");
  await card.focus();
  await page.keyboard.press("Enter");
  await expect(card).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Enter");
  await expect(card).toHaveAttribute("aria-pressed", "false");
  await expect.poll(() => card.evaluate((node) => getComputedStyle(node.querySelector(".card-turn")).transform)).toBe("none");
  const box = await card.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2 + 30, { steps: 10 });
  await expect.poll(() => card.evaluate((node) => parseFloat(node.style.getPropertyValue("--drag-x")))).toBeGreaterThan(30);
  await page.mouse.up();
  await expect.poll(() => card.evaluate((node) => Math.abs(parseFloat(node.style.getPropertyValue("--drag-x"))))).toBeLessThan(.5);
  await expect(card).toHaveAttribute("aria-pressed", "false");
  console.log("PASS: keyboard flip, pointer drag, spring return and drag/click separation");

  await expect(page.getByRole("link", { name: "Team", exact: true })).toHaveAttribute("href", "team.html");
  await page.locator("#how-it-works").scrollIntoViewIfNeeded();
  await expect(page.locator(".floating-dock")).toHaveClass(/is-visible/);
  const input = page.getByRole("searchbox", { name: "What do you remember?" });
  await input.fill("meeting");
  await expect(page.locator("[data-demo-results] article")).toHaveCount(1);
  await expect(page.locator("[data-demo-results] h3")).toHaveText("The prototype review");
  await input.fill("unfindable-zebra");
  await expect(page.locator("[data-demo-results] article")).toHaveCount(0);
  await input.fill("<img src=x onerror=alert(1)>");
  await expect(page.locator("[data-demo-results] img")).toHaveCount(0);
  await expect(page.locator("[data-demo-results] [onerror]")).toHaveCount(0);
  await page.getByRole("button", { name: "The draft", exact: true }).click();
  await expect(page.locator("[data-demo-results] h3")).toHaveText("The draft, right where it stopped");
  await page.getByRole("button", { name: "Next possibility" }).click();
  await expect.poll(() => page.locator("[data-memory-rail]").evaluate((node) => node.scrollLeft)).toBeGreaterThan(50);
  await page.locator("#privacy details").first().locator("summary").click();
  await expect(page.locator("#privacy details").first()).toHaveAttribute("open", "");
  const remote = requests.filter((url) => !url.startsWith(new URL(base).origin));
  assert.deepEqual([...new Set(remote)], [releaseUrl]);
  console.log("PASS: team link, dock, sample search, injection safety, carousel, disclosure and no demo egress");

  const scrollPage = async (fraction) => {
    await page.evaluate((progress) => window.scrollTo({
      top: (document.documentElement.scrollHeight - innerHeight) * progress,
      behavior: "instant",
    }), fraction);
    await expect(page.locator(".floating-dock")).toHaveClass(/is-visible/);
    return page.locator(".floating-dock").evaluate((node) => node.getBoundingClientRect().x);
  };
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto(base);
    await expect(page.locator("[data-release-status]")).toHaveText("v0.4.0 is ready");
    const left = await scrollPage(.2);
    await scrollPage(.6);
    await expect.poll(() => page.locator(".floating-dock").evaluate((node) => node.getBoundingClientRect().x)).toBeGreaterThan(left + 5);
    const middle = await page.locator(".floating-dock").evaluate((node) => node.getBoundingClientRect().x);
    await scrollPage(1);
    await expect.poll(() => page.locator(".floating-dock").evaluate((node) => node.getBoundingClientRect().x)).toBeGreaterThan(middle + 5);
    const right = await page.locator(".floating-dock").boundingBox();
    assert.ok(right.x + right.width <= width - 11, "Download dock leaves the viewport at the bottom");
    await scrollPage(.2);
    await expect.poll(() => page.locator(".floating-dock").evaluate((node) => node.getBoundingClientRect().x)).toBeLessThan(middle);
  }
  console.log("PASS: dock moves left to right with scrolling, reverses upward and stays onscreen on desktop/mobile");

  for (const width of [320, 390, 768, 1024, 1440, 1920]) {
    await page.setViewportSize({ width, height: 1000 });
    await page.goto(base);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    const headingRight = await page.locator("h1").evaluate((node) => {
      const range = document.createRange();
      range.selectNodeContents(node);
      return range.getBoundingClientRect().right;
    });
    assert.ok(headingRight <= width + 1, `${width}px headline clips at ${headingRight}px`);
  }
  console.log("PASS: responsive widths 320, 390, 768, 1024, 1440 and 1920; headline remains visible");

  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto(base);
  await expect(page.locator("body")).toHaveAttribute("data-motion", "paused");
  assert.equal(await card.evaluate((node) => getComputedStyle(node).animationName), "none");
  const reducedLeft = await scrollPage(.3);
  await scrollPage(1);
  assert.equal(await page.locator(".floating-dock").evaluate((node) => node.getBoundingClientRect().x), reducedLeft);
  await page.emulateMedia({ reducedMotion: "no-preference" });

  await page.unroute(releaseUrl);
  await page.route(releaseUrl, (route) => route.fulfill({ status: 404, json: {} }));
  await page.goto(base);
  await expect(page.locator("[data-release-status]")).toHaveText("No public release yet");
  for (const link of await page.locator("[data-download]").all()) await expect(link).toHaveText("View releases");
  await page.unroute(releaseUrl);
  await page.route(releaseUrl, (route) => route.fulfill({ status: 503, body: "Unavailable" }));
  await page.goto(base);
  await expect(page.locator("[data-download]").first()).toHaveAttribute("href", /releases\/latest\/download\/FNDR-latest\.dmg$/);
  console.log("PASS: OS reduced motion, no-release state and API failure fallback");

  const noScript = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
  const staticPage = await noScript.newPage();
  await staticPage.goto(base);
  await expect(staticPage.locator("h1")).toHaveText("Find your way back.");
  await expect(staticPage.locator("[data-download]").first()).toHaveAttribute("href", /FNDR-latest\.dmg$/);
  await expect(staticPage.locator(".install-steps li")).toHaveCount(3);
  await staticPage.locator(".gatekeeper-note summary").click();
  await expect(staticPage.locator(".gatekeeper-note p")).toBeVisible();
  await noScript.close();
  assert.deepEqual(errors, []);
  console.log("PASS: no-JavaScript download, installation guidance and native disclosure; no uncaught browser errors");
} finally {
  await browser.close();
}

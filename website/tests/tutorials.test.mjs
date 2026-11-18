import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

test("tutorials are reachable and presentation images are packaged", async () => {
  const home = await readFile(new URL("../index.html", import.meta.url), "utf8");
  const page = await readFile(new URL("../tutorials.html", import.meta.url), "utf8");
  const workflow = await readFile(new URL("../../.github/workflows/pages.yml", import.meta.url), "utf8");
  assert.match(home, /href="tutorials\.html"[^>]*>Tutorials</);
  assert.match(home, /images\/fndr-home\.png/);
  assert.doesNotMatch(page, /<video|fndr-demo-redacted/);
  assert.match(page, /<iframe[^>]*src="demo\/demo-preview\.html/);
  assert.match(page, /data-tour-toggle/);
  assert.match(workflow, /npm ci/);
  assert.match(workflow, /vite\.demo\.config/);
  for (const file of ["images/fndr-home.png", "images/fndr-recent-work.png"]) {
    assert.ok(page.includes(file));
    await access(new URL(`../${file}`, import.meta.url));
  }
  assert.match(workflow, /website\/tutorials\.html/);
});

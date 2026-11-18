import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

test("tutorials retain the guided demo without the screenshot gallery", async () => {
  const home = await readFile(new URL("../index.html", import.meta.url), "utf8");
  const page = await readFile(new URL("../tutorials.html", import.meta.url), "utf8");
  const workflow = await readFile(new URL("../../.github/workflows/pages.yml", import.meta.url), "utf8");
  assert.match(home, /href="tutorials\.html"[^>]*>Tutorials</);
  assert.match(home, /images\/fndr-home\.png/);
  assert.doesNotMatch(page, /<video|fndr-demo-redacted/);
  assert.match(page, /<iframe[^>]*src="demo\/demo-preview\.html/);
  assert.match(page, /data-tour-toggle/);
  assert.match(page, /class="quick-guide"/);
  assert.doesNotMatch(page, /A look inside FNDR|tutorial-gallery/);
  assert.match(workflow, /npm ci/);
  assert.match(workflow, /vite\.demo\.config/);
  await access(new URL("../images/fndr-home.png", import.meta.url));
  assert.match(workflow, /website\/tutorials\.html/);
});

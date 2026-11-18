import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { DEMO_MEMORIES, searchMemories, springStep } from "../experience.js";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("memory cards have keyboard controls", async () => {
  const html = await read("../index.html");
  assert.match(html, /<button[^>]*data-memory-card/);
  assert.match(html, /data-deck/);
});

test("the sample search is labelled and works without collecting input", async () => {
  const html = await read("../index.html");
  assert.match(html, /<label[^>]*for="demo-search"/);
  assert.match(html, /type="search"[^>]*id="demo-search"/);
  assert.match(html, /data-demo-results/);
  assert.match(html, /sample memories/i);
  assert.doesNotMatch(html, /<form\b/);
});

test("Pages includes the interaction module, not a new runtime dependency", async () => {
  const html = await read("../index.html");
  const workflow = await read("../../.github/workflows/pages.yml");
  assert.match(html, /type="module" src="experience.js"/);
  assert.match(workflow, /website\/experience\.js/);
});

test("sample search ranks matching memories without inventing results", () => {
  assert.equal(searchMemories("the article about local search")[0].app, "Safari");
  assert.equal(searchMemories("meeting")[0].app, "Notes");
  assert.equal(searchMemories("draft")[0].app, "Preview");
  assert.deepEqual(searchMemories("unfindable-zebra"), []);
  assert.deepEqual(searchMemories("<script>alert(1)</script>"), []);
  assert.deepEqual(searchMemories(""), DEMO_MEMORIES);
  assert.deepEqual(searchMemories("the and for"), []);
});

test("a dragged card settles back to its origin at different frame rates", () => {
  for (const frequency of [30, 60, 120]) {
    let state = { position: 180, velocity: 0 };
    for (let i = 0; i < frequency * 3; i++) state = springStep(state.position, state.velocity, 0, 1 / frequency);
    assert.ok(Math.abs(state.position) < .001, `${frequency} Hz position did not settle`);
    assert.ok(Math.abs(state.velocity) < .001, `${frequency} Hz velocity did not settle`);
  }
  const longFrame = springStep(200, 0, 0, 10);
  assert.ok(Number.isFinite(longFrame.position) && Math.abs(longFrame.position) < 200);
});

test("demo and motion have no persistence or network path", async () => {
  const source = await read("../experience.js");
  assert.doesNotMatch(source, /\b(fetch|XMLHttpRequest|WebSocket|localStorage|sessionStorage)\b/);
  assert.doesNotMatch(source, /\.innerHTML\s*=/);
});

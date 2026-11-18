import assert from "node:assert/strict";
import { readFile, access } from "node:fs/promises";
import test from "node:test";

test("the team page is reachable and ships all five photographs", async () => {
  const home = await readFile(new URL("../index.html", import.meta.url), "utf8");
  const team = await readFile(new URL("../team.html", import.meta.url), "utf8");
  const workflow = await readFile(new URL("../../.github/workflows/pages.yml", import.meta.url), "utf8");
  assert.match(home, /href="team\.html"[^>]*>Team</);
  assert.doesNotMatch(home, /data-motion-toggle|A memory for the things on your Mac/);
  assert.match(team, /<h1[^>]*>[^<]*Team/i);
  for (const name of ["portrait-1.jpg", "portrait-2.jpg", "portrait-3.jpg", "portrait-4.jpg", "team.jpg"]) {
    assert.match(team, new RegExp(name.replace(".", "\\.")));
    await access(new URL(`../images/${name}`, import.meta.url));
  }
  assert.match(workflow, /website\/team\.html/);
  assert.match(workflow, /website\/images/);
});

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  FALLBACK_DOWNLOAD_URL,
  RELEASES_URL,
  describeRelease,
  selectDmgAsset,
  selectLatestFullRelease,
} from "../release.js";

const root = new URL("../../", import.meta.url);
const read = (path) => readFile(new URL(path, root), "utf8");

test("selects the first full v-tag release and ignores drafts and prereleases", () => {
  const releases = [
    { tag_name: "v9.0.0-beta", draft: false, prerelease: true },
    { tag_name: "nightly", draft: false, prerelease: false },
    { tag_name: "v0.4.0", draft: false, prerelease: false },
    { tag_name: "v0.3.0", draft: false, prerelease: false },
  ];

  assert.equal(selectLatestFullRelease(releases), releases[2]);
});

test("prefers the stable DMG and never selects updater archives", () => {
  const release = {
    assets: [
      { name: "FNDR.app.tar.gz", browser_download_url: "https://example.test/updater" },
      { name: "FNDR_0.4.0_aarch64.dmg", browser_download_url: "https://example.test/versioned" },
      { name: "FNDR-latest.dmg", browser_download_url: "https://example.test/stable" },
      { name: "latest.json", browser_download_url: "https://example.test/manifest" },
    ],
  };

  assert.equal(selectDmgAsset(release)?.browser_download_url, "https://example.test/stable");
  assert.equal(selectDmgAsset({ assets: release.assets.slice(0, 2) })?.name, "FNDR_0.4.0_aarch64.dmg");
});

test("release copy reports compatibility and controlled signing state", () => {
  const signed = describeRelease({
    tag_name: "v0.4.0",
    published_at: "2026-10-07T00:00:00Z",
    body: "Signed with Developer ID and notarized by Apple.",
  });
  const adHoc = describeRelease({ tag_name: "v0.4.0", body: "Ad-hoc build." });

  assert.match(signed.trustLabel, /notarized/i);
  assert.match(adHoc.trustLabel, /manual approval/i);
  assert.match(signed.compatibility, /macOS 13\+.*Apple Silicon/i);
});

test("static page keeps essential copy and usable release fallbacks in HTML", async () => {
  const html = await read("website/index.html");

  assert.match(html, /<main[\s>]/);
  assert.match(html, /Download for Mac/);
  assert.match(html, /macOS 13\+/);
  assert.match(html, /Apple Silicon/);
  assert.match(html, /System Settings[\s\S]*Privacy &amp; Security[\s\S]*Open Anyway/);
  assert.match(html, /local/i);
  assert.match(html, /pause capture/i);
  assert.match(html, /blocklist/i);
  assert.match(html, /delete/i);
  assert.match(html, new RegExp(FALLBACK_DOWNLOAD_URL.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.match(html, /github\.com\/anurupkumar18\/FNDR\/releases/);
  assert.doesNotMatch(html, /\bv\d+\.\d+\.\d+\b/);
  assert.equal(RELEASES_URL, "https://api.github.com/repos/anurupkumar18/FNDR/releases/latest");
});

test("accessibility fallbacks are explicit in CSS", async () => {
  const css = await read("website/styles.css");

  assert.match(css, /:focus-visible/);
  assert.match(css, /prefers-reduced-motion:\s*reduce/);
  assert.match(css, /prefers-contrast:\s*more/);
  assert.match(css, /prefers-reduced-transparency:\s*reduce/);
  assert.match(css, /min-height:\s*44px/);
});

test("workflows preserve one release path and deploy Pages with least privileges", async () => {
  const release = await read(".github/workflows/release.yml");
  const pages = await read(".github/workflows/pages.yml");

  assert.match(release, /tags:\s*\[\s*"v\*"\s*\]/);
  assert.match(release, /includeUpdaterJson:\s*true/g);
  assert.match(release, /FNDR-latest\.dmg/);
  assert.match(release, /gh release upload/);
  assert.match(pages, /actions\/configure-pages@v5/);
  assert.match(pages, /actions\/upload-pages-artifact@v4/);
  assert.match(pages, /actions\/deploy-pages@v4/);
  assert.match(pages, /contents:\s*read/);
  assert.match(pages, /pages:\s*write/);
  assert.match(pages, /id-token:\s*write/);
  assert.doesNotMatch(pages, /contents:\s*write/);
});

export const REPOSITORY_URL = "https://github.com/anurupkumar18/FNDR";
export const RELEASES_PAGE_URL = `${REPOSITORY_URL}/releases`;
export const RELEASES_URL = "https://api.github.com/repos/anurupkumar18/FNDR/releases/latest";
export const FALLBACK_DOWNLOAD_URL = `${REPOSITORY_URL}/releases/latest/download/FNDR-latest.dmg`;

const NOTARIZED_MARKER = "Signed with Developer ID and notarized by Apple.";

export function selectLatestFullRelease(releases) {
  if (!Array.isArray(releases)) return null;

  return releases.find((release) => (
    release
    && release.draft !== true
    && release.prerelease !== true
    && typeof release.tag_name === "string"
    && release.tag_name.startsWith("v")
  )) ?? null;
}

export function selectDmgAsset(release) {
  const assets = Array.isArray(release?.assets) ? release.assets : [];
  const stable = assets.find((asset) => asset?.name === "FNDR-latest.dmg");
  if (stable) return stable;

  return assets.find((asset) => (
    typeof asset?.name === "string"
    && asset.name.toLowerCase().endsWith(".dmg")
  )) ?? null;
}

function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return null;
  const megabytes = bytes / 1_000_000;
  return `${new Intl.NumberFormat("en", { maximumFractionDigits: 1 }).format(megabytes)} MB`;
}

function formatDate(value) {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return null;

  return new Intl.DateTimeFormat("en", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  }).format(date);
}

export function describeRelease(release, asset = selectDmgAsset(release)) {
  const isNotarized = typeof release?.body === "string"
    && release.body.includes(NOTARIZED_MARKER);

  return {
    version: release?.tag_name || "Latest release",
    published: formatDate(release?.published_at),
    size: formatBytes(asset?.size),
    compatibility: "macOS 13+ · Apple Silicon",
    isNotarized,
    trustLabel: isNotarized
      ? "Developer ID signed · Apple notarized"
      : "Manual approval may be required",
  };
}

function setText(selector, value) {
  const element = document.querySelector(selector);
  if (element && value) element.textContent = value;
}

function renderRelease(release) {
  const downloads = document.querySelectorAll("[data-download]");
  const status = document.querySelector("[data-release-status]");
  const details = document.querySelector("[data-release-details]");
  const asset = selectDmgAsset(release);
  const description = describeRelease(release, asset);

  if (status) {
    status.dataset.state = asset ? "ready" : "waiting";
    status.parentElement.dataset.state = asset ? "ready" : "waiting";
  }
  setText("[data-release-version]", description.version);
  setText("[data-release-compatibility]", description.compatibility);
  setText("[data-release-trust]", description.trustLabel);
  setText("[data-release-size]", description.size);
  setText("[data-release-date]", description.published);
  if (details) details.hidden = false;

  if (!downloads.length || !status) return;

  if (asset?.browser_download_url) {
    downloads.forEach((download) => {
      download.href = asset.browser_download_url;
      download.textContent = "Download for Mac";
    });
    status.textContent = `${description.version} is ready`;
    return;
  }

  downloads.forEach((download) => {
    download.href = release.html_url || `${REPOSITORY_URL}/releases/latest`;
    download.textContent = "View latest release";
  });
  status.textContent = `${description.version} is published; the DMG is still processing`;
}

function renderNoRelease() {
  const downloads = document.querySelectorAll("[data-download]");
  const status = document.querySelector("[data-release-status]");
  downloads.forEach((download) => {
    download.href = RELEASES_PAGE_URL;
    download.textContent = "View releases";
  });
  if (status) {
    status.dataset.state = "waiting";
    status.parentElement.dataset.state = "waiting";
    status.textContent = "No public release yet";
  }
}

async function enhanceRelease() {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), 4500);

  try {
    const response = await fetch(RELEASES_URL, {
      headers: { Accept: "application/vnd.github+json" },
      signal: controller.signal,
    });
    if (response.status === 404) {
      renderNoRelease();
      return;
    }
    if (!response.ok) throw new Error(`GitHub returned ${response.status}`);

    const release = selectLatestFullRelease([await response.json()]);
    if (release) renderRelease(release);
    else renderNoRelease();
  } catch {
    // Static copy and the stable latest-asset URL remain fully usable.
  } finally {
    window.clearTimeout(timeout);
  }
}

function enableMemoryField() {
  const finePointer = window.matchMedia?.("(pointer: fine)").matches;
  const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  if (!finePointer || reduceMotion) return;

  let frame = 0;
  let x = 50;
  let y = 18;
  const render = () => {
    frame = 0;
    document.documentElement.style.setProperty("--pointer-x", `${x}%`);
    document.documentElement.style.setProperty("--pointer-y", `${y}%`);
  };

  window.addEventListener("pointermove", (event) => {
    x = (event.clientX / window.innerWidth) * 100;
    y = (event.clientY / window.innerHeight) * 100;
    if (!frame) frame = window.requestAnimationFrame(render);
  }, { passive: true });
}

if (typeof document !== "undefined") {
  enableMemoryField();
  enhanceRelease();
}

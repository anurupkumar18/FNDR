const frame = document.querySelector(".tour-window iframe");
const status = document.querySelector("[data-tour-status]");
const toggle = document.querySelector("[data-tour-toggle]");
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
let runId = 0;
let running = false;
let restartRequested = false;
let visibilityObserver;

function stop(message) {
  runId += 1;
  running = false;
  visibilityObserver?.disconnect();
  toggle.textContent = "Restart tour";
  status.textContent = message;
}

function wait(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }

async function find(doc, step, id) {
  for (let attempt = 0; attempt < 100 && id === runId; attempt += 1) {
    const element = step.textButton
      ? [...doc.querySelectorAll("button")].find((button) => button.textContent.trim() === step.textButton && button.getClientRects().length)
      : doc.querySelector(step.selector);
    if (element && element.getClientRects().length) return element;
    await wait(150);
  }
  return null;
}

async function start() {
  const id = ++runId;
  running = true;
  toggle.textContent = "Pause tour";
  const doc = frame.contentDocument;
  if (!doc) { stop("The demo could not be loaded. Refresh the page to try again."); return; }
  const steps = [
    { text: "This is FNDR Home, with sample memories ready to explore.", selector: '[aria-label="Open sidebar"]' },
    { text: "Open the sidebar to find the Memory Vault.", selector: '[aria-label="Open sidebar"]', click: true },
    { text: "The Memory Vault brings your work together.", textButton: "Memory Vault", click: true },
    { text: "Open a memory to see its context.", selector: '[role="button"][aria-label^="Open memory:"]', click: true },
  ];
  for (const step of steps) {
    if (id !== runId) return;
    status.textContent = step.text;
    const element = await find(doc, step, id);
    if (id !== runId) return;
    if (!element) { stop("The tour stopped. You can still explore the demo yourself."); return; }
    await wait(3000);
    if (id !== runId) return;
    if (step.click) element.click();
  }
  if (id === runId) stop("Tour complete. Explore the sample memories yourself.");
}

frame.addEventListener("load", () => {
  const doc = frame.contentDocument;
  if (!doc) { stop("The demo could not be loaded. Refresh the page to try again."); return; }
  for (const event of ["pointerdown", "keydown"]) {
    doc.addEventListener(event, (input) => {
      if (input.isTrusted && running) stop("Tour paused. Explore the demo at your own pace.");
    }, true);
  }
  if (!reducedMotion.matches || restartRequested) {
    restartRequested = false;
    running = true;
    status.textContent = "Scroll to the demo to start the guided tour.";
    visibilityObserver?.disconnect();
    visibilityObserver = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting) && running) {
        visibilityObserver.disconnect();
        void start();
      }
    }, { threshold: .2 });
    visibilityObserver.observe(frame);
  } else stop("The demo is ready. Start the tour or explore on your own.");
});

toggle.addEventListener("click", () => {
  if (running) stop("Tour paused. Explore the demo at your own pace.");
  else {
    restartRequested = true;
    status.textContent = "Restarting the demo…";
    frame.src = frame.getAttribute("src");
  }
});

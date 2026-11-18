const frame = document.querySelector(".tour-window iframe");
const status = document.querySelector("[data-tour-status]");
const description = document.querySelector("[data-tour-description]");
const chapterSelect = document.querySelector("[data-tour-chapter]");
const toggle = document.querySelector("[data-tour-toggle]");
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

// The tour navigates the real preview UI; its IPC data is synthetic.
const chapters = [
  { id: "home", title: "Home", text: "Start here to search memories and pick up recent work. The greeting and activity in this demo are fictional." },
  { id: "search", title: "Search memories", text: "Enter a topic, app, person, or remembered phrase on Home. Open a result to see its captured context." },
  { id: "files", title: "Search files", text: "FNDR can also search selected local folders. File and code search require setup in the Mac app; this demo has no access to your files." },
  { id: "ask", title: "Search & Ask FNDR", target: "Search & Ask FNDR", text: "Ask a question about your saved context. Review the supporting memories and sources alongside the answer." },
  { id: "vault", title: "Memory Vault", target: "Memory Vault", text: "Browse, search and filter saved memories. Open a memory to inspect its details and associated context." },
  { id: "review", title: "FNDR Review", target: "FNDR Review", text: "Review your recent activity over time and find the work you want to return to." },
  { id: "todos", title: "To-dos", target: "To-dos", text: "Review task suggestions and manage your to-dos. Check a suggestion against its source before relying on it." },
  { id: "outputs", title: "Daily outputs", target: "Daily outputs", text: "Explore summaries and outputs built from captured context. Review content before sharing or sending it." },
  { id: "stats", title: "Stats", target: "Stats", text: "See activity patterns and usage over time. The figures in this preview are examples, not your activity." },
  { id: "guide", title: "Screen Guide", target: "Screen Guide", text: "Screen Guide is an experimental way to get help with what is on screen. Real screen access is available only in the Mac app with permission." },
  { id: "agent", title: "Hermes Agent", target: "Hermes Agent", text: "Hermes Agent is an experimental workspace for working with selected context. External services and actions require separate configuration." },
  { id: "notch", title: "Notch Do & voice", text: "On a supported Mac, Notch Do and voice offer additional ways to interact with FNDR. Voice and device actions are not available in this browser preview." },
  { id: "activity", title: "Privacy Activity", target: "Privacy Activity", text: "Inspect privacy-related activity and understand how your settings affect capture." },
  { id: "settings", title: "Settings: General", target: "Trust & Settings", tab: "General", text: "Use Settings to configure the app. General contains everyday preferences; the following chapters cover capture, privacy, voice and AI." },
  { id: "capture", title: "Settings: Capture", target: "Trust & Settings", tab: "Capture", text: "Choose how capture works or pause it. Screen Recording permission is managed by macOS, not this demo." },
  { id: "privacy", title: "Settings: Privacy", target: "Trust & Settings", tab: "Privacy", text: "Review privacy controls, including exclusions and retention. Choose what FNDR is allowed to remember." },
  { id: "voice", title: "Settings: Voice", target: "Trust & Settings", tab: "Voice", text: "Configure voice features in the Mac app. Microphone access and speech are not enabled in this browser demo." },
  { id: "integrations", title: "Settings: AI & integrations", target: "Trust & Settings", tab: "AI", text: "Review model and AI configuration. MCP connections, external services and actions must be configured separately; check their permissions before connecting." },
];
for (const chapter of chapters) chapterSelect.add(new Option(chapter.title, chapter.id));

let index = 0;
let running = false;
let generation = 0;
let observer;
let restartRequested = false;
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function pause(message) {
  generation += 1;
  running = false;
  observer?.disconnect();
  toggle.textContent = "Resume tour";
  if (message) status.textContent = message;
}

function visible(element) {
  return element && element.getClientRects().length && !element.closest('[inert], [aria-hidden="true"]') && getComputedStyle(element).visibility !== "hidden";
}

async function find(doc, selector, id, text) {
  for (let attempt = 0; attempt < 80 && id === generation; attempt += 1) {
    const element = [...doc.querySelectorAll(selector)].find((item) => visible(item) && (!text || item.textContent.trim() === text));
    if (element) return element;
    await wait(100);
  }
  return null;
}

async function navigate(chapter, id) {
  const doc = frame.contentDocument;
  if (!doc) return false;
  for (let count = 0; count < 5 && id === generation; count += 1) {
    const dialogs = [...doc.querySelectorAll('[role="dialog"]')].filter(visible);
    if (!dialogs.length) break;
    const close = [...dialogs[dialogs.length - 1].querySelectorAll('button[aria-label^="Close "]')].find(visible);
    if (!close) break;
    close.click();
    await wait(180);
  }
  if (!chapter.target) return id === generation;
  const sidebar = await find(doc, '[aria-label="Open sidebar"]', id);
  if (!sidebar) return false;
  sidebar.click();
  const destination = await find(doc, "button", id, chapter.target);
  if (!destination) return false;
  destination.click();
  if (chapter.tab) {
    const tab = await find(doc, '[role="tab"]', id, chapter.tab);
    if (!tab) return false;
    tab.click();
  }
  return id === generation;
}

async function showChapter(nextIndex, id) {
  index = nextIndex;
  const chapter = chapters[index];
  chapterSelect.value = chapter.id;
  status.textContent = `${index + 1} / ${chapters.length} · ${chapter.title}`;
  description.textContent = chapter.text;
  if (!reducedMotion.matches) {
    description.getAnimations().forEach((animation) => animation.cancel());
    description.animate([{ opacity: 0, transform: "translateY(8px)" }, { opacity: 1, transform: "translateY(0)" }], { duration: 420, easing: "cubic-bezier(.22, 1, .36, 1)" });
  }
  const success = await navigate(chapter, id);
  if (!success && id === generation) pause("The tour could not open this page. You can still explore the demo yourself.");
}

async function play() {
  const id = ++generation;
  running = true;
  toggle.textContent = "Pause tour";
  for (let step = index; step < chapters.length && id === generation; step += 1) {
    await showChapter(step, id);
    await wait(8500);
  }
  if (id === generation) pause("Tour complete. Choose a chapter or explore the demo yourself.");
}

function choose(nextIndex) {
  pause();
  void showChapter(Math.max(0, Math.min(chapters.length - 1, nextIndex)), generation);
}

frame.addEventListener("load", () => {
  const doc = frame.contentDocument;
  if (!doc) { pause("The demo could not be loaded. Refresh the page to try again."); return; }
  for (const event of ["pointerdown", "keydown"]) doc.addEventListener(event, (input) => {
    if (input.isTrusted && running) pause("Tour paused. Explore the demo at your own pace.");
  }, true);
  index = 0;
  if (reducedMotion.matches && !restartRequested) {
    pause("The demo is ready. Start the tour or explore on your own.");
    description.textContent = chapters[0].text;
    return;
  }
  restartRequested = false;
  running = true;
  toggle.textContent = "Pause tour";
  status.textContent = "Scroll to the demo to start the guided tour.";
  observer?.disconnect();
  observer = new IntersectionObserver((entries) => {
    if (entries.some((entry) => entry.isIntersecting) && running) { observer.disconnect(); void play(); }
  }, { threshold: .2 });
  observer.observe(frame);
});

toggle.addEventListener("click", () => running ? pause("Tour paused. Explore the demo at your own pace.") : void play());
chapterSelect.addEventListener("change", () => choose(chapters.findIndex((chapter) => chapter.id === chapterSelect.value)));
document.querySelector("[data-tour-previous]").addEventListener("click", () => choose(index - 1));
document.querySelector("[data-tour-next]").addEventListener("click", () => choose(index + 1));
document.querySelector("[data-tour-restart]").addEventListener("click", () => {
  pause("Restarting the demo…");
  restartRequested = true;
  frame.src = frame.getAttribute("src");
});

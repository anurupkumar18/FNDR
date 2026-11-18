// Presentation only. The demo uses public, synthetic memories and no network.
export const DEMO_MEMORIES = [
  { app: "Safari", glyph: "↗", kind: "safari", time: "Yesterday, 3:42 pm", title: "Small models, local search", text: "Research on keeping semantic search responsive and private on-device.", context: "Research / Local embeddings", keywords: "article browser page reading embeddings machine private" },
  { app: "Notes", glyph: "≡", kind: "notes", time: "Monday, 10:18 am", title: "The prototype review", text: "Meeting notes: keep the first version small and compare exact identifiers with meaning-based retrieval.", context: "Product review / Meeting notes", keywords: "decision conversation team monday" },
  { app: "Preview", glyph: "▤", kind: "document", time: "Tuesday, 2:06 pm", title: "The draft, right where it stopped", text: "An architecture document connecting capture, local memory and source-aware results.", context: "Work in progress / Architecture draft", keywords: "pdf diagram document design thought" },
];

const STOP_WORDS = new Set(["the", "a", "an", "about", "that", "from", "with", "on", "for", "my", "i", "of", "it", "was", "in", "to", "and"]);
const words = (value) => value.toLowerCase().normalize("NFKD").replace(/[\u0300-\u036f]/g, "").match(/[a-z0-9]+/g) ?? [];

export function searchMemories(query) {
  if (!query.trim()) return [...DEMO_MEMORIES];
  const terms = words(query).filter((word) => !STOP_WORDS.has(word));
  if (!terms.length) return [];
  return DEMO_MEMORIES.map((memory) => {
    const text = words(`${memory.app} ${memory.title} ${memory.text} ${memory.context} ${memory.keywords}`).join(" ");
    return { memory, score: terms.filter((term) => text.includes(term)).length };
  }).filter(({ score }) => score > 0).sort((a, b) => b.score - a.score).map(({ memory }) => memory);
}

// Semi-implicit spring integration; long frames are bounded to prevent jumps.
export function springStep(position, velocity, target, seconds) {
  const dt = Math.max(0, Math.min(seconds, 1 / 30));
  const nextVelocity = velocity + ((target - position) * 170 - velocity * 24) * dt;
  return { position: position + nextVelocity * dt, velocity: nextVelocity };
}

function element(tag, className, text) {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function enableDemo(signal) {
  const input = document.querySelector("#demo-search");
  const results = document.querySelector("[data-demo-results]");
  const count = document.querySelector("[data-demo-count]");
  if (!input || !results || !count) return;
  const render = () => {
    const memories = searchMemories(input.value);
    const fragment = document.createDocumentFragment();
    for (const memory of memories) {
      const article = element("article", "demo-result");
      const source = element("span", "source");
      source.append(element("span", `app-glyph ${memory.kind}`, memory.glyph), document.createTextNode(memory.app), element("time", "", memory.time));
      article.append(source, element("h3", "", memory.title), element("p", "", memory.text), element("span", "result-context", memory.context));
      fragment.append(article);
    }
    if (!memories.length) fragment.append(element("p", "demo-disclaimer", "No sample matched. Try “meeting”, “local search”, or “draft”. FNDR searches permitted memories on your Mac; this is only a small demo."));
    results.replaceChildren(fragment);
    count.textContent = `${memories.length} sample ${memories.length === 1 ? "memory" : "memories"}`;
  };
  input.addEventListener("input", render, { signal });
  document.querySelectorAll("[data-query]").forEach((button) => {
    button.addEventListener("click", () => { input.value = button.dataset.query; render(); }, { signal });
  });
  render();
}

function enableExperience() {
  const events = new AbortController();
  const { signal } = events;
  enableDemo(signal);
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
  const finePointer = window.matchMedia("(pointer: fine)");
  const deck = document.querySelector("[data-deck]");
  const hero = document.querySelector(".hero");
  const dock = document.querySelector(".floating-dock");
  const scenes = [...document.querySelectorAll("[data-scroll-scene]")];
  const rail = document.querySelector("[data-memory-rail]");
  let paused = reducedMotion.matches;
  let frame = 0;
  let lastTime = 0;
  let scrollDirty = true;
  const tilt = { x: 0, y: 0, vx: 0, vy: 0, tx: 0, ty: 0 };
  const cards = [...document.querySelectorAll("[data-memory-card]")].map((node) => ({ node, x: 0, y: 0, vx: 0, vy: 0, active: false }));
  const schedule = () => { if (!frame && !document.hidden) frame = window.requestAnimationFrame(render); };

  function render(now) {
    frame = 0;
    const dt = lastTime ? (now - lastTime) / 1000 : 1 / 60;
    lastTime = now;
    let moving = false;
    if (!paused && deck) {
      const x = springStep(tilt.x, tilt.vx, tilt.tx, dt);
      const y = springStep(tilt.y, tilt.vy, tilt.ty, dt);
      Object.assign(tilt, { x: x.position, vx: x.velocity, y: y.position, vy: y.velocity });
      deck.style.setProperty("--deck-x", `${tilt.x}deg`);
      deck.style.setProperty("--deck-y", `${tilt.y}deg`);
      moving = Math.abs(tilt.x - tilt.tx) + Math.abs(tilt.y - tilt.ty) + Math.abs(tilt.vx) + Math.abs(tilt.vy) > .005;
      for (const card of cards) {
        if (!card.active) {
          const cx = springStep(card.x, card.vx, 0, dt);
          const cy = springStep(card.y, card.vy, 0, dt);
          Object.assign(card, { x: cx.position, vx: cx.velocity, y: cy.position, vy: cy.velocity });
        }
        card.node.style.setProperty("--drag-x", `${card.x}px`);
        card.node.style.setProperty("--drag-y", `${card.y}px`);
        if (Math.abs(card.x) + Math.abs(card.y) + Math.abs(card.vx) + Math.abs(card.vy) > .1) moving = true;
      }
    }
    if (scrollDirty) {
      scrollDirty = false;
      const heroRect = hero?.getBoundingClientRect();
      dock?.classList.toggle("is-visible", (heroRect?.bottom ?? 0) < window.innerHeight * .65);
      if (dock) {
        const viewport = document.documentElement.clientWidth;
        const edge = Math.max(12, viewport * .03);
        const travel = Math.max(0, viewport - dock.offsetWidth - edge * 2);
        const start = Math.max(0, heroRect ? window.scrollY + heroRect.bottom - window.innerHeight * .65 : 0);
        const end = document.documentElement.scrollHeight - window.innerHeight;
        const progress = Math.max(0, Math.min(1, (window.scrollY - start) / Math.max(1, end - start)));
        const current = parseFloat(dock.style.getPropertyValue("--dock-x")) || 0;
        dock.style.setProperty("--dock-x", `${paused ? Math.min(current, travel) : progress * travel}px`);
      }
      const retreat = heroRect ? Math.max(0, Math.min(1, -heroRect.top / heroRect.height)) : 0;
      deck?.style.setProperty("--deck-scroll", `${paused ? 0 : retreat * 85}px`);
      deck?.style.setProperty("--deck-scroll-tilt", `${paused ? 0 : retreat * 14}deg`);
      for (const scene of scenes) {
        const rect = scene.getBoundingClientRect();
        if (rect.bottom < 0 || rect.top > window.innerHeight) continue;
        const progress = Math.max(-1, Math.min(1, (window.innerHeight / 2 - (rect.top + rect.height / 2)) / window.innerHeight));
        scene.style.setProperty("--scene-lift", `${paused ? 0 : progress * -40}px`);
        scene.querySelectorAll("[data-parallax-object]").forEach((node) => node.style.setProperty("--object-tilt", `${paused ? 0 : progress * -9}deg`));
      }
    }
    if (moving && !paused) schedule();
    else lastTime = 0;
  }

  function setMotion() {
    paused = reducedMotion.matches;
    document.body.dataset.motion = paused ? "paused" : "on";
    if (paused) {
      Object.assign(tilt, { x: 0, y: 0, vx: 0, vy: 0, tx: 0, ty: 0 });
      deck?.style.setProperty("--deck-x", "0deg");
      deck?.style.setProperty("--deck-y", "0deg");
      cards.forEach((card) => {
        Object.assign(card, { x: 0, y: 0, vx: 0, vy: 0, active: false });
        card.node.classList.remove("is-dragging");
        card.node.style.setProperty("--drag-x", "0px");
        card.node.style.setProperty("--drag-y", "0px");
      });
      document.querySelectorAll("[data-parallax-object]").forEach((node) => node.style.setProperty("--object-tilt", "0deg"));
      scenes.forEach((scene) => scene.style.setProperty("--scene-lift", "0px"));
    }
    scrollDirty = true;
    schedule();
  }
  reducedMotion.addEventListener("change", setMotion, { signal });
  hero?.addEventListener("pointermove", (event) => {
    if (paused || !finePointer.matches) return;
    const rect = hero.getBoundingClientRect();
    tilt.tx = ((event.clientY - rect.top) / rect.height - .5) * -6;
    tilt.ty = (event.clientX / window.innerWidth - .5) * 9;
    schedule();
  }, { passive: true, signal });
  hero?.addEventListener("pointerleave", () => { tilt.tx = 0; tilt.ty = 0; schedule(); }, { signal });

  for (const card of cards) {
    let start = null;
    let moved = false;
    let suppressClickUntil = 0;
    card.node.addEventListener("pointerdown", (event) => {
      if (paused || event.button !== 0 || start) return;
      start = { x: event.clientX - card.x, y: event.clientY - card.y, pointer: event.pointerId };
      moved = false;
      card.active = true;
      card.node.setPointerCapture(event.pointerId);
    }, { signal });
    card.node.addEventListener("pointermove", (event) => {
      if (!start || paused || start.pointer !== event.pointerId) return;
      card.x = Math.max(-220, Math.min(220, event.clientX - start.x));
      card.y = Math.max(-180, Math.min(180, event.clientY - start.y));
      moved ||= Math.abs(card.x) + Math.abs(card.y) > 6;
      card.node.classList.toggle("is-dragging", moved);
      schedule();
    }, { signal });
    const release = (event) => {
      if (!start || event.pointerId !== start.pointer) return;
      if (moved) suppressClickUntil = performance.now() + 400;
      start = null;
      card.active = false;
      card.node.classList.remove("is-dragging");
      schedule();
    };
    card.node.addEventListener("pointerup", release, { signal });
    card.node.addEventListener("pointercancel", release, { signal });
    card.node.addEventListener("lostpointercapture", release, { signal });
    card.node.addEventListener("click", (event) => {
      if (event.detail !== 0 && performance.now() < suppressClickUntil) return;
      card.node.setAttribute("aria-pressed", String(card.node.getAttribute("aria-pressed") !== "true"));
    }, { signal });
  }

  const updateRail = () => {
    if (!rail) return;
    document.querySelectorAll("[data-rail-step]").forEach((button) => {
      const next = Number(button.dataset.railStep) > 0;
      button.disabled = next ? rail.scrollLeft + rail.clientWidth >= rail.scrollWidth - 3 : rail.scrollLeft < 3;
    });
  };
  const stepRail = (direction) => {
    if (!rail) return;
    const width = rail.firstElementChild.getBoundingClientRect().width + 24;
    rail.scrollBy({ left: width * direction, behavior: paused ? "instant" : "smooth" });
  };
  document.querySelectorAll("[data-rail-step]").forEach((button) => button.addEventListener("click", () => stepRail(Number(button.dataset.railStep)), { signal }));
  rail?.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    stepRail(event.key === "ArrowRight" ? 1 : -1);
  }, { signal });
  rail?.addEventListener("scroll", updateRail, { passive: true, signal });
  window.addEventListener("scroll", () => { scrollDirty = true; schedule(); }, { passive: true, signal });
  window.addEventListener("resize", () => { scrollDirty = true; updateRail(); schedule(); }, { passive: true, signal });
  // Release text and expanded content can change the available travel distance.
  const layoutObserver = new ResizeObserver(() => { scrollDirty = true; schedule(); });
  layoutObserver.observe(document.body);
  if (dock) layoutObserver.observe(dock);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) { window.cancelAnimationFrame(frame); frame = 0; lastTime = 0; }
    else { scrollDirty = true; schedule(); }
  }, { signal });
  window.addEventListener("pagehide", (event) => {
    if (!event.persisted) { events.abort(); layoutObserver.disconnect(); window.cancelAnimationFrame(frame); }
  }, { once: true });
  setMotion();
  updateRail();
}

if (typeof document !== "undefined") enableExperience();

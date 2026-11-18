const dialog = document.querySelector(".team-dialog");
const name = document.querySelector("#person-name");
const work = document.querySelector("#person-work");
const portrait = dialog.querySelector(".team-dialog-front img");
const dialogCard = dialog.querySelector(".team-dialog-card");
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

document.querySelectorAll("[data-person]").forEach((card) => {
  card.addEventListener("click", () => {
    const origin = card.getBoundingClientRect();
    name.textContent = card.dataset.person;
    work.textContent = card.dataset.work;
    portrait.src = card.querySelector("img").src;
    for (const kind of ["linkedin", "github", "email"]) {
      dialog.querySelector(`[data-contact="${kind}"]`).href = kind === "email" ? `mailto:${card.dataset.email}` : card.dataset[kind];
    }
    dialog.showModal();
    if (!reducedMotion.matches) {
      const target = dialogCard.getBoundingClientRect();
      const x = origin.left + origin.width / 2 - (target.left + target.width / 2);
      const y = origin.top + origin.height / 2 - (target.top + target.height / 2);
      const angle = Math.atan2(new DOMMatrix(getComputedStyle(card).transform).b, new DOMMatrix(getComputedStyle(card).transform).a) * 180 / Math.PI;
      dialogCard.animate([
        { transform: `translate(${x}px, ${y}px) scale(${origin.width / target.width}, ${origin.height / target.height}) rotateZ(${angle}deg) rotateY(0deg)`, offset: 0 },
        { transform: "translate(0px, 0px) scale(1) rotateZ(0deg) rotateY(0deg)", offset: .55 },
        { transform: "translate(0px, 0px) scale(1) rotateZ(0deg) rotateY(180deg)", offset: 1 },
      ], { duration: 950, easing: "cubic-bezier(.22, 1, .36, 1)" });
    }
  });
});

dialog.querySelector(".team-dialog-close").addEventListener("click", () => dialog.close());
dialog.addEventListener("click", (event) => {
  if (event.target === dialog) dialog.close();
});

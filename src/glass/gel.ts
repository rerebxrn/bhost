// Les petits mouvements du verre :
//  - gel : un bouton marqué data-gel s’étire vers le pointeur quand on appuie, puis revient en ressort ;
//  - reflet : sur tout élément en verre (.g), un reflet doux suit la souris.

const calme = window.matchMedia("(prefers-reduced-motion: reduce)");

export function demarrerGel(racine: HTMLElement) {
  let gel: { el: HTMLElement; x0: number; y0: number; l: number; h: number } | null = null;

  const appui = (e: PointerEvent) => {
    if (calme.matches) return;
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-gel]");
    if (!el || (el as HTMLButtonElement).disabled) return;
    const r = el.getBoundingClientRect();
    gel = { el, x0: e.clientX, y0: e.clientY, l: r.width, h: r.height };
    // Le bouton grossit à partir de l’endroit où on a appuyé.
    el.style.transformOrigin = `${(((e.clientX - r.left) / r.width) * 100).toFixed(0)}% ${(((e.clientY - r.top) / r.height) * 100).toFixed(0)}%`;
    el.classList.add("gel-on");
    el.style.transform = "scale(1.07)";
  };

  const mouvement = (e: PointerEvent) => {
    // Reflet qui suit la souris sur le verre
    const verre = (e.target as HTMLElement).closest?.<HTMLElement>(".g");
    if (verre) {
      const r = verre.getBoundingClientRect();
      verre.style.setProperty("--mx", `${(((e.clientX - r.left) / r.width) * 100).toFixed(1)}%`);
      verre.style.setProperty("--my", `${(((e.clientY - r.top) / r.height) * 100).toFixed(1)}%`);
    }
    if (!gel) return;
    // Le bouton suit un peu le pointeur et s’étire dans sa direction, comme de la gelée.
    const dx = e.clientX - gel.x0;
    const dy = e.clientY - gel.y0;
    const frein = (d: number, max: number) => max * Math.tanh(d / (max * 4));
    const ex = Math.min(Math.abs(dx) / gel.l, 0.8) * 0.22;
    const ey = Math.min(Math.abs(dy) / gel.h, 0.8) * 0.16;
    gel.el.style.transform =
      `translate(${frein(dx, 9).toFixed(1)}px,${frein(dy, 6).toFixed(1)}px) ` +
      `scale(${(1.07 + ex - ey * 0.5).toFixed(3)},${(1.07 + ey - ex * 0.5).toFixed(3)})`;
  };

  const relache = () => {
    if (!gel) return;
    gel.el.classList.remove("gel-on");
    gel.el.style.transform = "";
    gel = null;
  };

  racine.addEventListener("pointerdown", appui);
  window.addEventListener("pointermove", mouvement);
  window.addEventListener("pointerup", relache);
  window.addEventListener("pointercancel", relache);
  return () => {
    racine.removeEventListener("pointerdown", appui);
    window.removeEventListener("pointermove", mouvement);
    window.removeEventListener("pointerup", relache);
    window.removeEventListener("pointercancel", relache);
  };
}

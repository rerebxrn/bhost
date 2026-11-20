// Moteur Liquid Glass.
// Pour chaque élément marqué data-lens, on fabrique à sa taille exacte :
//  - une carte de déplacement : le bord bombé du verre, qui courbe ce qui passe dessous
//    (profil « squircle » + loi de Snell, indice 1,5) ;
//  - une carte de reflets : un liseré lumineux, fort en haut à gauche, doux en bas à droite ;
// puis un filtre SVG : flou léger → réfraction rouge / vert / bleu décalée → saturation → reflets.
// Le filtre est posé en backdrop-filter. Ça marche dans WebView2 (le moteur de Chrome).

type Reglage = { bezel: number; scale: number; blur: number; blurT: number; spec: number; sat: number; ca: number };

// Un réglage par genre d'élément : barre flottante, petit bouton, panneau.
const LENS: Record<string, Reglage> = {
  bar: { bezel: 18, scale: 34, blur: 0.4, blurT: 7, spec: 0.95, sat: 1.9, ca: 0.09 },
  btn: { bezel: 10, scale: 18, blur: 0.4, blurT: 4, spec: 0.9, sat: 1.8, ca: 0.07 },
  panel: { bezel: 16, scale: 28, blur: 5, blurT: 10, spec: 0.8, sat: 1.7, ca: 0.05 },
};

const NS = "http://www.w3.org/2000/svg";
const ATTENTE = 140; // ms sans changement de taille avant de recalculer
const SURFACE_MAX = 1_600_000; // au-delà (en pixels), on garde le flou simple

// De combien le verre dévie la lumière, du bord (0) vers l'intérieur (1).
function courbe(): Float32Array {
  const N = 256;
  const f = (t: number) => Math.pow(1 - Math.pow(1 - t, 4), 0.25);
  const out = new Float32Array(N + 1);
  let max = 0;
  for (let i = 0; i <= N; i++) {
    const t = Math.min(Math.max(i / N, 0.02), 1);
    const pente = (f(Math.min(t + 1e-3, 1)) - f(Math.max(t - 1e-3, 0))) / 2e-3;
    const a1 = Math.atan(pente);
    const a2 = Math.asin(Math.sin(a1) / 1.5);
    out[i] = i >= N ? 0 : Math.tan(a1 - a2) * f(t);
    if (out[i] > max) max = out[i];
  }
  for (let i = 0; i <= N; i++) out[i] /= max || 1;
  return out;
}
const COURBE = courbe();

// Dessine les deux cartes (déplacement et reflets) d'un rectangle arrondi L × H de rayon R.
function cartes(L: number, H: number, R: number, bezel: number) {
  const cd = document.createElement("canvas");
  const cs = document.createElement("canvas");
  cd.width = cs.width = L;
  cd.height = cs.height = H;
  const xd = cd.getContext("2d")!;
  const xs = cs.getContext("2d")!;
  const D = xd.createImageData(L, H);
  const S = xs.createImageData(L, H);
  const hw = L / 2;
  const hh = H / 2;
  const Lx = -0.55; // la lumière vient d'en haut à gauche
  const Ly = -0.83;
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < L; x++) {
      const px = x + 0.5 - hw;
      const py = y + 0.5 - hh;
      const qx = Math.abs(px) - (hw - R);
      const qy = Math.abs(py) - (hh - R);
      let d: number; // distance au bord
      let nx: number; // direction vers l'extérieur
      let ny: number;
      if (qx > 0 && qy > 0) {
        const l = Math.hypot(qx, qy) || 1;
        d = R - l;
        nx = (Math.sign(px) * qx) / l;
        ny = (Math.sign(py) * qy) / l;
      } else if (qx > qy) {
        d = hw - Math.abs(px);
        nx = Math.sign(px);
        ny = 0;
      } else {
        d = hh - Math.abs(py);
        nx = 0;
        ny = Math.sign(py);
      }
      const k = (y * L + x) * 4;
      const t = d / bezel;
      const m = t >= 1 || d <= 0 ? 0 : COURBE[Math.min(256, Math.max(0, Math.round(t * 256)))];
      D.data[k] = 128 - nx * m * 127;
      D.data[k + 1] = 128 - ny * m * 127;
      D.data[k + 2] = 128;
      D.data[k + 3] = 255;
      const face = nx * Lx + ny * Ly;
      const lisere = d > 0 ? Math.exp(-Math.pow(d / 1.4, 2)) * 0.95 + Math.exp(-Math.pow(d / 6, 2)) * 0.22 : 0;
      const a = lisere * (Math.pow(Math.max(face, 0), 1.6) + 0.42 * Math.pow(Math.max(-face, 0), 1.6) + 0.1);
      S.data[k] = S.data[k + 1] = S.data[k + 2] = 255;
      S.data[k + 3] = Math.min(255, a * 255);
    }
  }
  xd.putImageData(D, 0, 0);
  xs.putImageData(S, 0, 0);
  return { deplacement: cd.toDataURL("image/png"), reflets: cs.toDataURL("image/png") };
}

function balise(nom: string, attributs: Record<string, string | number>) {
  const e = document.createElementNS(NS, nom);
  for (const a in attributs) e.setAttribute(a, String(attributs[a]));
  return e;
}

function filtre(id: string, L: number, H: number, R: number, P: Reglage, t: number, reflet: number) {
  const c = cartes(L, H, R, Math.min(P.bezel, H / 2, L / 2));
  const f = balise("filter", {
    id,
    x: 0,
    y: 0,
    width: L,
    height: H,
    filterUnits: "userSpaceOnUse",
    primitiveUnits: "userSpaceOnUse",
    "color-interpolation-filters": "sRGB",
  });
  const image = (href: string, result: string) =>
    balise("feImage", { href, x: 0, y: 0, width: L, height: H, preserveAspectRatio: "none", result });
  const deplacer = (echelle: number, result: string) =>
    balise("feDisplacementMap", {
      in: "bl",
      in2: "map",
      scale: echelle.toFixed(1),
      xChannelSelector: "R",
      yChannelSelector: "G",
      result,
    });
  const garder = (values: string, source: string, result: string) =>
    balise("feColorMatrix", { in: source, type: "matrix", values, result });
  f.append(
    balise("feGaussianBlur", { in: "SourceGraphic", stdDeviation: (P.blur + t * P.blurT).toFixed(2), result: "bl" }),
    image(c.deplacement, "map"),
    // Le rouge, le vert et le bleu sont déviés un peu différemment : c'est l'irisation du bord.
    deplacer(P.scale, "dr"),
    deplacer(P.scale * (1 - P.ca), "dg"),
    deplacer(P.scale * (1 - 2 * P.ca), "db"),
    garder("1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0", "dr", "cr"),
    garder("0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 1 0", "dg", "cg"),
    garder("0 0 0 0 0 0 0 0 0 0 0 0 1 0 0 0 0 0 1 0", "db", "cb"),
    balise("feBlend", { in: "cr", in2: "cg", mode: "screen", result: "rg" }),
    balise("feBlend", { in: "rg", in2: "cb", mode: "screen", result: "rgb" }),
    balise("feColorMatrix", { in: "rgb", type: "saturate", values: P.sat, result: "sat" }),
    image(c.reflets, "sp"),
    balise("feComposite", { in: "sat", in2: "sp", operator: "arithmetic", k1: 0, k2: 1, k3: (P.spec * reflet).toFixed(2), k4: 0 }),
  );
  return f;
}

// Met le moteur en route sur toute la page. Renvoie de quoi l'arrêter et le rafraîchir.
export function demarrerVerre(racine: HTMLElement) {
  const svg = balise("svg", { width: 0, height: 0, "aria-hidden": "true" }) as SVGSVGElement;
  svg.style.cssText = "position:absolute;width:0;height:0;overflow:hidden";
  const defs = balise("defs", {});
  svg.append(defs);
  document.body.append(svg);

  // Un filtre par taille. On compte combien d'éléments s'en servent, pour le jeter ensuite.
  const filtres = new Map<string, { id: string; f: Element; n: number }>();
  const suivis = new Set<HTMLElement>();
  const minuteries = new WeakMap<HTMLElement, number>();
  let numero = 0;

  const lacher = (el: HTMLElement) => {
    const cle = el.dataset.lk;
    if (!cle) return;
    delete el.dataset.lk;
    el.style.removeProperty("--lens");
    const x = filtres.get(cle);
    if (x && --x.n <= 0) {
      x.f.remove();
      filtres.delete(cle);
    }
  };

  const cleDe = (el: HTMLElement) => {
    const genre = el.dataset.lens;
    if (!genre || !el.isConnected) return null;
    const L = Math.round(el.offsetWidth);
    const H = Math.round(el.offsetHeight);
    if (L < 8 || H < 8 || L * H > SURFACE_MAX) return null;
    const R = Math.min(parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0, H / 2, L / 2);
    const style = getComputedStyle(el);
    // --t : intensité du verre. --lg-spec : force des reflets (plus faible sur le verre sombre).
    const t = Math.round((parseFloat(style.getPropertyValue("--t")) || 0) * 10) / 10;
    const reflet = parseFloat(style.getPropertyValue("--lg-spec")) || 1;
    return { cle: `${genre}${L}x${H}r${Math.round(R)}t${t}s${reflet}`, L, H, R, t, reflet, P: LENS[genre] ?? LENS.btn };
  };

  const poser = (el: HTMLElement) => {
    const k = cleDe(el);
    if (!k) return lacher(el);
    if (el.dataset.lk === k.cle) return;
    lacher(el);
    let x = filtres.get(k.cle);
    if (!x) {
      const id = "lg" + ++numero;
      const f = filtre(id, k.L, k.H, k.R, k.P, k.t, k.reflet);
      defs.append(f);
      x = { id, f, n: 0 };
      filtres.set(k.cle, x);
    }
    x.n++;
    el.dataset.lk = k.cle;
    el.style.setProperty("--lens", `url(#${x.id})`);
  };

  const prevoir = (el: HTMLElement, delai: number) => {
    window.clearTimeout(minuteries.get(el));
    minuteries.set(el, window.setTimeout(() => poser(el), delai));
  };

  // Quand un élément change de taille : flou simple pendant le mouvement, puis nouveau filtre.
  const tailles = new ResizeObserver((entrees) => {
    for (const e of entrees) {
      const el = e.target as HTMLElement;
      const k = cleDe(el);
      if (k && el.dataset.lk === k.cle) continue;
      lacher(el);
      prevoir(el, ATTENTE);
    }
  });

  const parcourir = () => {
    for (const el of suivis) {
      if (!el.isConnected || !el.dataset.lens) {
        tailles.unobserve(el);
        lacher(el);
        suivis.delete(el);
      }
    }
    racine.querySelectorAll<HTMLElement>("[data-lens]").forEach((el) => {
      if (suivis.has(el)) return;
      suivis.add(el);
      tailles.observe(el);
      prevoir(el, 0);
    });
  };

  // Quand des éléments arrivent ou partent (changement de page, menu qui s'ouvre…),
  // ou qu'une classe change (verre sombre au-dessus de la console) : on revérifie, une fois par image.
  let prevu = 0;
  const changements = new MutationObserver(() => {
    if (prevu) return;
    prevu = requestAnimationFrame(() => {
      prevu = 0;
      parcourir();
      for (const el of suivis) {
        const k = cleDe(el);
        // Seulement les éléments déjà posés dont le réglage a changé (pas ceux en plein mouvement).
        if (k && el.dataset.lk && el.dataset.lk !== k.cle) prevoir(el, 0);
      }
    });
  });
  changements.observe(racine, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["data-lens", "class"],
  });
  parcourir();

  return {
    // À appeler quand l'intensité du verre ou le thème change.
    rafraichir: () => suivis.forEach((el) => prevoir(el, 0)),
    arreter: () => {
      cancelAnimationFrame(prevu);
      changements.disconnect();
      tailles.disconnect();
      suivis.forEach((el) => {
        window.clearTimeout(minuteries.get(el));
        lacher(el);
      });
      suivis.clear();
      svg.remove();
    },
  };
}

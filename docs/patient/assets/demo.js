// The interactive chapter: one simulated day, re-run live with the
// breakfast carb ratio from the slider. The simulator runs in the page
// through the wasm binding; figures.js draws the report panels above.
import init, { run_day } from "./sim/tir_sim.js";

const lang = document.documentElement.lang === "de" ? "de" : "en";
const T = {
  en: {
    asSet: "as set",
    changed: "with your change",
    inRange: "in range",
    below: "below",
    above: "above",
    bolus: "breakfast bolus",
    measure: "reading",
    virtual: "virtual patient · model output",
    error: "The interactive day could not load. Check your connection and reload the page.",
  },
  de: {
    asSet: "wie eingestellt",
    changed: "mit Ihrer Änderung",
    inRange: "im Ziel",
    below: "unter",
    above: "über",
    bolus: "Frühstücksbolus",
    measure: "Wert",
    virtual: "virtueller Patient · Modellausgabe",
    error: "Der interaktive Tag konnte nicht geladen werden. Prüfen Sie die Verbindung und laden Sie die Seite neu.",
  },
}[lang];

const NS = "http://www.w3.org/2000/svg";
const MG_PER_MMOL = 18.0156;
const G_MIN = 40;
const G_MAX = 280;
const SEED = 1;
const DEBOUNCE_MS = 150;
const UNIT = lang === "de" ? "IE" : "U";
const RATIO_UNIT = lang === "de" ? "g/IE" : "g/U";

const panel = document.getElementById("demo");
const slider = document.getElementById("cr");
const out = document.getElementById("cr-out");

const el = (name, attrs = {}, parent) => {
  const e = document.createElementNS(NS, name);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.append(e);
  return e;
};
const text = (parent, x, y, s, cls, anchor = "start") => {
  const t = el("text", { x, y, class: cls, "text-anchor": anchor }, parent);
  t.textContent = s;
  return t;
};
const fmt = (v, d = 0) => v.toLocaleString(lang, { maximumFractionDigits: d, minimumFractionDigits: d });
const pct = (v) => `${fmt(v)}${lang === "de" ? "\u00a0%" : "%"}`;
const clock = (h) => `${String(Math.floor(h)).padStart(2, "0")}:${String(Math.round((h % 1) * 60)).padStart(2, "0")}`;

// The site's one glucose scale, so the day compares against every chapter.
const lineAttrs = (muted, w) => ({
  stroke: muted ? "var(--muted)" : "var(--ink)", "stroke-width": w, "stroke-linejoin": "round",
});

let base = null;
let variant = null;
let timer = 0;

function plot(host) {
  const W = host.clientWidth;
  const H = Math.round(W < 520 ? W * 0.72 : W * 0.48);
  const m = { l: 46, r: 12, t: 26, b: 34 };
  const sx = (h) => m.l + (h / 24) * (W - m.l - m.r);
  const sy = (v) => m.t + (1 - (Math.min(Math.max(v, G_MIN), G_MAX) - G_MIN) / (G_MAX - G_MIN)) * (H - m.t - m.b);
  const svg = el("svg", { viewBox: `0 0 ${W} ${H}`, width: W, height: H, "aria-hidden": "true" });
  el("rect", { x: m.l, y: sy(180), width: W - m.l - m.r, height: sy(70) - sy(180), fill: "var(--band)" }, svg);
  for (const [v, cls] of [[250, "grid"], [180, "limit-high"], [70, "limit-low"]]) {
    el("line", { x1: m.l, x2: W - m.r, y1: sy(v), y2: sy(v), class: cls }, svg);
    text(svg, m.l - 6, sy(v) + 1, fmt(v), "tick", "end");
    text(svg, m.l - 6, sy(v) + 11, fmt(v / MG_PER_MMOL, 1), "tick-sub", "end");
  }
  text(svg, 2, 12, lang === "de" ? "mg/dl" : "mg/dL", "tick-sub");
  for (let h = 0; h <= 24; h += 6) {
    el("line", { x1: sx(h), x2: sx(h), y1: H - m.b, y2: H - m.b + 4, class: "edge" }, svg);
    text(svg, sx(h), H - m.b + 16, clock(h), "tick", "middle");
  }
  el("line", { x1: m.l, x2: W - m.r, y1: H - m.b, y2: H - m.b, class: "edge" }, svg);
  for (const [min, g] of base.meals) {
    el("line", { x1: sx(min / 60), x2: sx(min / 60), y1: H - m.b, y2: H - m.b - 8, stroke: "var(--ink)", "stroke-width": 2 }, svg);
    text(svg, sx(min / 60), H - m.b - 11, `${fmt(g)} g`, "tick-sub", "middle");
  }
  const path = (day) => day.glucose_mg_dl.map((v, i) => `${i ? "L" : "M"}${sx(day.t_min[i] / 60).toFixed(1)},${sy(v).toFixed(1)}`).join("");
  el("path", { d: path(base), fill: "none", ...lineAttrs(true, 1.4) }, svg);
  el("path", { d: path(variant), fill: "none", ...lineAttrs(false, 2.8) }, svg);
  return svg;
}

function legend() {
  const ul = document.createElement("ul");
  ul.className = "legend";
  for (const [muted, name] of [[true, T.asSet], [false, T.changed]]) {
    const li = document.createElement("li");
    const svg = el("svg", { width: 28, height: 10, viewBox: "0 0 28 10", "aria-hidden": "true" });
    el("line", { x1: 1, x2: 27, y1: 5, y2: 5, ...lineAttrs(muted, muted ? 1.4 : 2.8) }, svg);
    li.append(svg, name);
    ul.append(li);
  }
  return ul;
}

function table() {
  const wrap = document.createElement("div");
  wrap.className = "demo-table";
  const row = (label, a, b) =>
    `<tr><th scope="row">${label}</th><td>${a}</td><td>${b}</td></tr>`;
  const bolus = (day) => `${fmt(day.bolus_u_breakfast, 1)} ${UNIT} (${fmt(60 / day.bolus_u_breakfast, 1)} ${RATIO_UNIT})`;
  wrap.innerHTML = `
    <table>
      <thead><tr><th scope="col"><span class="sr">${T.measure}</span></th><th scope="col">${T.asSet}</th><th scope="col">${T.changed}</th></tr></thead>
      <tbody>
        ${row(T.inRange, pct(base.tir_pct), pct(variant.tir_pct))}
        ${row(`${T.below} 70`, pct(base.tbr_pct), pct(variant.tbr_pct))}
        ${row(`${T.above} 180`, pct(base.tar_pct), pct(variant.tar_pct))}
        ${row(T.bolus, bolus(base), bolus(variant))}
      </tbody>
    </table>`;
  return wrap;
}

function draw() {
  panel.querySelectorAll(".plot, .demo-table").forEach((e) => e.remove());
  const host = document.createElement("div");
  host.className = "plot";
  panel.insertBefore(host, panel.querySelector(".virtual"));
  host.append(plot(host));
  panel.insertBefore(table(), panel.querySelector(".virtual"));
}

function updateOutput() {
  out.textContent = `${fmt(Number(slider.value), 1)} ${RATIO_UNIT}`;
}

function day(ratio) {
  const request = ratio == null ? { seed: SEED } : { seed: SEED, breakfast_cr_g_per_u: ratio };
  return JSON.parse(run_day(JSON.stringify(request)));
}

function onInput() {
  updateOutput();
  panel.classList.add("calculating");
  clearTimeout(timer);
  timer = setTimeout(() => {
    variant = day(Number(slider.value));
    draw();
    panel.classList.remove("calculating");
  }, DEBOUNCE_MS);
}

function fail() {
  slider.disabled = true;
  const p = document.createElement("p");
  p.className = "demo-error";
  p.setAttribute("role", "status");
  p.textContent = T.error;
  panel.append(p);
}

async function start() {
  try {
    await init();
  } catch {
    fail();
    return;
  }
  base = day(null);
  slider.value = Math.min(20, Math.max(5, Math.round((60 / base.bolus_u_breakfast) * 2) / 2));
  updateOutput();
  variant = day(Number(slider.value));
  panel.append(legend());
  draw();
  const v = document.createElement("div");
  v.className = "virtual";
  v.textContent = T.virtual;
  panel.append(v);
  slider.addEventListener("input", onInput);
  let w = innerWidth;
  let frame = 0;
  addEventListener("resize", () => {
    if (innerWidth === w) return;
    w = innerWidth;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(draw);
  });
}

updateOutput();
start();

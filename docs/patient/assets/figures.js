// Draws the report panels from the virtual patient's data. Each
// <figure class="panel" data-fig="..."> names its figure; the page text
// supplies the title, caption and line names (data-lines, "|"-separated).

const lang = document.documentElement.lang === "de" ? "de" : "en";
const T = {
  en: {
    numbers: "Show the numbers",
    virtual: "Virtual patient · model output",
    hoursAfter: "hours",
    time: "Time",
    median: "Median",
    middle: "Middle half of days",
    inRange: "in range",
    below: "below",
    above: "above",
    mgdl: "mg/dL",
    gph: "g/h",
    share: "% of peak",
  },
  de: {
    numbers: "Zahlen anzeigen",
    virtual: "Virtueller Patient · Modellergebnis",
    hoursAfter: "Stunden",
    time: "Zeit",
    median: "Median",
    middle: "Mittlere Hälfte der Tage",
    inRange: "im Zielbereich",
    below: "darunter",
    above: "darüber",
    mgdl: "mg/dl",
    gph: "g/h",
    share: "% vom Gipfel",
  },
}[lang];

const MG_PER_MMOL = 18.0156;
// One glucose scale for every glucose figure, so curves compare across pages.
const GLUCOSE = { min: 40, max: 280, ticks: [70, 180, 250] };
const STYLES = [
  { w: 2.8, dash: "" },
  { w: 2, dash: "7 4" },
  { w: 2.2, dash: "1 4", cap: "round" },
  { w: 1.4, dash: "", muted: true },
];
const NS = "http://www.w3.org/2000/svg";
const fmt = (v, d = 0) => v.toLocaleString(lang, { maximumFractionDigits: d, minimumFractionDigits: d });
const clock = (h) => `${String(Math.floor(h) % 24 || (h >= 24 ? 24 : 0)).padStart(2, "0")}:${String(Math.round((h % 1) * 60)).padStart(2, "0")}`;

// What each figure plots. `x` is either clock time or hours after t=0.
const FIGURES = {
  day: (d) => ({
    t: d.day.t_h, x: "clock", glucose: true,
    spread: [d.day.auto.p5, d.day.auto.p25, d.day.auto.p75, d.day.auto.p95],
    lines: [d.day.auto.p50], meals: d.day.meals, bins: [d.day.auto_bins],
    table: { cols: [T.median, T.middle], rows: (i) => [fmt(d.day.auto.p50[i]), `${fmt(d.day.auto.p25[i])}–${fmt(d.day.auto.p75[i])}`], every: 8 },
  }),
  compare: (d) => ({
    t: d.day.t_h, x: "clock", glucose: true,
    spread: [null, d.day.auto.p25, d.day.auto.p75, null],
    lines: [d.day.auto.p50, d.day.basal.p50], meals: d.day.meals,
    bins: [d.day.auto_bins, d.day.basal_bins],
  }),
  fasting: (d) => ({ t: d.fasting.t_h, glucose: true, lines: [d.fasting.basal, d.fasting.none] }),
  gut: (d) => ({ t: d.gut.t_h, y: { min: 0, max: 25, ticks: [0, 5, 10, 15, 20, 25], unit: T.gph }, lines: [d.gut.g20, d.gut.g50, d.gut.g100] }),
  insulin: (d) => ({
    t: d.insulin.t_h, y: { min: 0, max: 100, ticks: [0, 25, 50, 75, 100], unit: T.share },
    lines: [d.insulin.blood, d.insulin.liver, d.insulin.disposal, d.insulin.transport].map((s) => s.map((v) => 100 * v)),
  }),
  dosing: (d) => ({ t: d.dosing.t_h, glucose: true, lines: [d.dosing.weak, d.dosing.right, d.dosing.strong] }),
  sensor: (d) => ({
    t: d.sensor.t_h, glucose: true, lines: [d.sensor.blood, d.sensor.tissue],
    dots: { t: d.sensor.reading_t_h, v: d.sensor.reading },
  }),
};

function el(name, attrs = {}, parent) {
  const e = document.createElementNS(NS, name);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.append(e);
  return e;
}

function text(parent, x, y, s, cls, anchor = "start") {
  const t = el("text", { x, y, class: cls, "text-anchor": anchor }, parent);
  t.textContent = s;
  return t;
}

function plot(host, f, names) {
  const W = host.clientWidth;
  const H = Math.round(W < 520 ? W * 0.72 : W * 0.48);
  const m = { l: f.glucose ? 46 : 40, r: 12, t: 26, b: 34 };
  const y = f.glucose ? GLUCOSE : f.y;
  const t0 = f.t[0], t1 = f.t[f.t.length - 1];
  const sx = (t) => m.l + ((t - t0) / (t1 - t0)) * (W - m.l - m.r);
  const sy = (v) => m.t + (1 - (Math.min(Math.max(v, y.min), y.max) - y.min) / (y.max - y.min)) * (H - m.t - m.b);
  const svg = el("svg", { viewBox: `0 0 ${W} ${H}`, width: W, height: H, "aria-hidden": "true" });

  if (f.glucose) {
    el("rect", { x: m.l, y: sy(180), width: W - m.l - m.r, height: sy(70) - sy(180), fill: "var(--band)" }, svg);
  }
  for (const v of y.ticks) {
    const cls = f.glucose && v === 180 ? "limit-high" : f.glucose && v === 70 ? "limit-low" : "grid";
    el("line", { x1: m.l, x2: W - m.r, y1: sy(v), y2: sy(v), class: cls }, svg);
    text(svg, m.l - 6, sy(v) + (f.glucose ? 1 : 4), fmt(v), "tick", "end");
    if (f.glucose) text(svg, m.l - 6, sy(v) + 11, fmt(v / MG_PER_MMOL, 1), "tick-sub", "end");
  }
  text(svg, 2, 12, f.glucose ? T.mgdl : y.unit, "tick-sub");

  const step = f.x === "clock" ? 6 : t1 - t0 > 6 ? 2 : 1;
  for (let h = Math.ceil(t0); h <= t1 + 1e-9; h += step) {
    el("line", { x1: sx(h), x2: sx(h), y1: H - m.b, y2: H - m.b + 4, class: "edge" }, svg);
    text(svg, sx(h), H - m.b + 16, f.x === "clock" ? clock(h) : fmt(h), "tick", "middle");
  }
  if (f.x !== "clock") text(svg, W - m.r, H - 4, T.hoursAfter, "tick-sub", "end");
  el("line", { x1: m.l, x2: W - m.r, y1: H - m.b, y2: H - m.b, class: "edge" }, svg);

  const path = (vals) => vals.map((v, i) => `${i ? "L" : "M"}${sx(f.t[i]).toFixed(1)},${sy(v).toFixed(1)}`).join("");
  const area = (lo, hi) => `${path(hi)}L${[...lo.keys()].reverse().map((i) => `${sx(f.t[i]).toFixed(1)},${sy(lo[i]).toFixed(1)}`).join("L")}Z`;
  if (f.spread) {
    const [p5, p25, p75, p95] = f.spread;
    if (p5) el("path", { d: area(p5, p95), fill: "var(--spread-outer)" }, svg);
    el("path", { d: area(p25, p75), fill: "var(--spread-inner)", opacity: 0.85 }, svg);
  }
  for (const [h, g] of f.meals ?? []) {
    el("line", { x1: sx(h), x2: sx(h), y1: H - m.b, y2: H - m.b - 8, stroke: "var(--ink)", "stroke-width": 2 }, svg);
    text(svg, sx(h), H - m.b - 11, `${g} g`, "tick-sub", "middle");
  }
  f.lines.forEach((vals, k) => {
    const s = STYLES[k];
    el("path", {
      d: path(vals), fill: "none", stroke: s.muted ? "var(--muted)" : "var(--ink)", "stroke-width": s.w,
      "stroke-dasharray": s.dash, "stroke-linecap": s.cap ?? "butt", "stroke-linejoin": "round",
    }, svg);
  });
  if (f.dots) {
    f.dots.t.forEach((t, i) => el("circle", { cx: sx(t), cy: sy(f.dots.v[i]), r: 3.2, fill: "var(--paper)", stroke: "var(--ink)", "stroke-width": 1.6 }, svg));
  }
  return svg;
}

function legend(f, names) {
  const items = names.slice(0, f.lines.length + (f.dots ? 1 : 0));
  if (items.length < 2 && !f.spread) return null;
  const ul = document.createElement("ul");
  ul.className = "legend";
  items.forEach((name, k) => {
    const li = document.createElement("li");
    const svg = el("svg", { width: 28, height: 10, viewBox: "0 0 28 10", "aria-hidden": "true" });
    if (k < f.lines.length) {
      const s = STYLES[k];
      el("line", { x1: 1, x2: 27, y1: 5, y2: 5, stroke: s.muted ? "var(--muted)" : "var(--ink)", "stroke-width": s.w, "stroke-dasharray": s.dash, "stroke-linecap": s.cap ?? "butt" }, svg);
    } else {
      el("circle", { cx: 14, cy: 5, r: 3.2, fill: "var(--paper)", stroke: "var(--ink)", "stroke-width": 1.6 }, svg);
    }
    li.append(svg, name);
    ul.append(li);
  });
  if (f.spread) {
    const li = document.createElement("li");
    const svg = el("svg", { width: 28, height: 10, viewBox: "0 0 28 10", "aria-hidden": "true" });
    el("rect", { x: 1, y: 0, width: 26, height: 10, fill: "var(--spread-inner)" }, svg);
    li.append(svg, T.middle);
    ul.append(li);
  }
  return ul;
}

function bars(f, names) {
  const div = document.createElement("div");
  div.className = "bars";
  f.bins.forEach((b, k) => {
    const row = document.createElement("div");
    const label = f.bins.length > 1 ? `${names[k]}: ` : "";
    const pct = (v) => `${fmt(v)}${lang === "de" ? "\u00a0%" : "%"}`;
    row.innerHTML = `${label}<strong>${pct(b[2])}</strong> ${T.inRange} · ${pct(b[0] + b[1])} ${T.below} · ${pct(b[3] + b[4])} ${T.above}`;
    const bar = document.createElement("div");
    bar.className = "range";
    bar.setAttribute("aria-hidden", "true");
    ["vlow", "low", "in", "high", "vhigh"].forEach((c, i) => {
      const seg = document.createElement("i");
      seg.className = c;
      seg.style.flex = b[i];
      bar.append(seg);
    });
    row.append(bar);
    div.append(row);
  });
  return div;
}

function table(f, names) {
  const det = document.createElement("details");
  det.innerHTML = `<summary>${T.numbers}</summary>`;
  const cols = f.table?.cols ?? names.slice(0, f.lines.length);
  const every = f.table?.every ?? Math.max(1, Math.round(f.t.length / 12));
  const rows = [];
  for (let i = 0; i < f.t.length; i += every) {
    const cells = f.table ? f.table.rows(i) : f.lines.map((l) => fmt(l[i], f.glucose ? 0 : 1));
    rows.push(`<tr><td>${f.x === "clock" ? clock(f.t[i]) : `${fmt(f.t[i], 1)} h`}</td>${cells.map((c) => `<td>${c}</td>`).join("")}</tr>`);
  }
  const unit = f.glucose ? ` (${T.mgdl})` : f.y ? ` (${f.y.unit})` : "";
  det.innerHTML += `<div class="scroll"><table><thead><tr><th>${T.time}</th>${cols.map((c) => `<th>${c}${unit}</th>`).join("")}</tr></thead><tbody>${rows.join("")}</tbody></table></div>`;
  return det;
}

function draw(fig, data) {
  const f = FIGURES[fig.dataset.fig](data);
  const names = (fig.dataset.lines ?? "").split("|").filter(Boolean);
  fig.querySelectorAll(".plot, .legend, .bars, details, .virtual").forEach((e) => e.remove());
  const host = document.createElement("div");
  host.className = "plot";
  fig.append(host);
  const lg = legend(f, names);
  if (lg) fig.insertBefore(lg, host);
  host.append(plot(host, f, names));
  if (f.bins) fig.append(bars(f, names));
  fig.append(table(f, names));
  const v = document.createElement("div");
  v.className = "virtual";
  v.textContent = T.virtual;
  fig.append(v);
}

// The chapter strip remembers the last chapter read.
function markStopped() {
  const here = Number(document.body.dataset.chapter);
  const last = Number(localStorage.getItem("chapter") || 0);
  if (last && last !== here) document.querySelector(`.chapters a[data-n="${last}"]`)?.classList.add("stopped");
  localStorage.setItem("chapter", here);
}

function mermaidBlocks() {
  const blocks = document.querySelectorAll("pre code.language-mermaid");
  if (!blocks.length) return;
  blocks.forEach((b) => {
    const div = document.createElement("div");
    div.className = "mermaid";
    // Wide left-to-right charts shrink to unreadable text on a phone.
    div.textContent = innerWidth < 600 ? b.textContent.replace(/^flowchart LR/m, "flowchart TD") : b.textContent;
    (b.closest("div.language-mermaid") ?? b.parentElement).replaceWith(div);
  });
  window.addEventListener("load", () => {
    if (!window.mermaid) return;
    mermaid.initialize({
      startOnLoad: false,
      // On a phone the diagram keeps its natural size and scrolls sideways.
      flowchart: { useMaxWidth: innerWidth >= 600 },
      theme: "base",
      themeVariables: {
        fontFamily: "Public Sans, system-ui, sans-serif",
        fontSize: "15px",
        primaryColor: "#ffffff",
        primaryBorderColor: "#1d1d1b",
        primaryTextColor: "#1d1d1b",
        lineColor: "#1d1d1b",
        secondaryColor: "#e9f4e7",
        tertiaryColor: "#ffffff",
        edgeLabelBackground: "#ffffff",
      },
    });
    mermaid.run();
  });
}

try { markStopped(); } catch {}
mermaidBlocks();
const figs = document.querySelectorAll("figure.panel[data-fig]");
if (figs.length) {
  fetch(`${document.body.dataset.assets}figures.json`)
    .then((r) => r.json())
    .then((data) => {
      figs.forEach((fig) => draw(fig, data));
      let w = window.innerWidth;
      window.addEventListener("resize", () => {
        if (window.innerWidth === w) return;
        w = window.innerWidth;
        figs.forEach((fig) => draw(fig, data));
      });
    });
}

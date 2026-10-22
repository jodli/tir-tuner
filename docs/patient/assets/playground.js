// The playground chapter: the reader composes a day on the strip under the
// chart - each marker is a moment with its carbs and the bolus that belongs
// to it - and the simulator answers live. All physics stays in the wasm
// bindings; this file only moves state.
import init, { run_scenario, suggest_bolus } from "./sim/tir_sim.js";

const lang = document.documentElement.lang === "de" ? "de" : "en";
const UNIT = lang === "de" ? "IE" : "U";
const GDL = lang === "de" ? "mg/dl" : "mg/dL";
const T = {
  en: {
    mealAt: (t) => `Meal at ${t}`,
    corrAt: (t) => `Correction at ${t}`,
    day: "Your day",
    mealTime: "Meal time",
    carbs: "Carbs",
    dose: "Dose",
    less: "Less",
    more: "More",
    bolus: "Bolus",
    none: "None",
    calc: (u) => `Calculator ${u}`,
    calcLow: "Calculator: too low",
    noDose: "Calculator: no dose",
    own: "My own",
    ownDose: "Own dose",
    addCorrection: "Add a correction",
    remove: "Remove",
    done: "Done",
    laneHint: "Tap here to add a meal at that time",
    empty: "Tap the strip under the chart to add a meal at that time, or start from a ready-made day above.",
    marker: (e) => `${e.g ? `Meal at ${clock(e.min)}, ${fmt(e.g)} g` : `Correction at ${clock(e.min)}`}, ${dose(e) ? `bolus ${units(dose(e))}` : "no bolus"}. Arrow keys move it, Delete removes it.`,
    auto: "The pump recalculates the insulin every 15 minutes.",
    basal: "The pump runs a fixed basal rate and pauses it only when glucose runs low.",
    before: "Before your last change",
    now: "Now",
    beforeCol: "Before",
    keyWords: ["in range", "below", "above"],
    inRange: "In range 70-180",
    below: "Below 70",
    above: "Above 180",
    bolusRow: "Bolus insulin",
    meanRow: "Mean glucose",
    measure: "Measure",
    virtual: "Virtual patient. The same model as the report chapters, nothing added.",
    error: "The playground could not be loaded. Check the connection and reload the page.",
    broke: "The playground stopped. Reload the page to start again.",
  },
  de: {
    mealAt: (t) => `Mahlzeit um ${t}`,
    corrAt: (t) => `Korrektur um ${t}`,
    day: "Ihr Tag",
    mealTime: "Uhrzeit",
    carbs: "Kohlenhydrate",
    dose: "Dosis",
    less: "Weniger",
    more: "Mehr",
    bolus: "Bolus",
    none: "Keiner",
    calc: (u) => `Rechner ${u}`,
    calcLow: "Rechner: zu tief",
    noDose: "Rechner: keine Dosis",
    own: "Eigene",
    ownDose: "Eigene Dosis",
    addCorrection: "Korrektur hinzufügen",
    remove: "Entfernen",
    done: "Fertig",
    laneHint: "Tippen Sie hier, um zu dieser Zeit eine Mahlzeit hinzuzufügen",
    empty: "Tippen Sie auf den Streifen unter der Kurve, um zu dieser Zeit eine Mahlzeit hinzuzufügen, oder starten Sie oben mit einem fertigen Tag.",
    marker: (e) => `${e.g ? `Mahlzeit um ${clock(e.min)}, ${fmt(e.g)} g` : `Korrektur um ${clock(e.min)}`}, ${dose(e) ? `Bolus ${units(dose(e))}` : "kein Bolus"}. Pfeiltasten bewegen ihn, Entfernen löscht ihn.`,
    auto: "Die Pumpe berechnet das Insulin alle 15 Minuten neu.",
    basal: "Die Pumpe gibt eine feste Basalrate und pausiert sie nur, wenn der Zucker zu tief fällt.",
    before: "Vor Ihrer letzten Änderung",
    now: "Jetzt",
    beforeCol: "Vorher",
    keyWords: ["im Ziel", "unter", "über"],
    inRange: "Im Bereich 70-180",
    below: "Unter 70",
    above: "Über 180",
    bolusRow: "Bolus-Insulin",
    meanRow: "Mittlerer Blutzucker",
    measure: "Messgröße",
    virtual: "Virtueller Patient. Dasselbe Modell wie in den Berichtskapiteln, nichts hinzugefügt.",
    error: "Die Spielwiese konnte nicht geladen werden. Prüfen Sie die Verbindung und laden Sie die Seite neu.",
    broke: "Die Spielwiese ist stehen geblieben. Laden Sie die Seite neu, um von vorn zu beginnen.",
  },
}[lang];

const NS = "http://www.w3.org/2000/svg";
const MG_PER_MMOL = 18.0156;
const G_MIN = 40;
const G_MAX = 280;
const SEED = 1;
const ADMIT = 100;
const DEBOUNCE_MS = 120;
const STEP_MIN = 15;
const LAST_MIN = 1425;
const M = { l: 46, r: 12, t: 26, b: 34 };

const panel = document.getElementById("play");
const view = document.getElementById("view");
const totals = document.getElementById("totals");
const editor = document.getElementById("inspector");
const dayTitle = document.getElementById("day-title");
const modeAuto = document.getElementById("mode-auto");
const modeBasal = document.getElementById("mode-basal");
const modeHint = document.getElementById("mode-hint");
const presets = [...panel.querySelectorAll("[data-preset]")];
const dials = Object.fromEntries(["cr", "isf", "tgt", "res"].map((id) => [id, document.getElementById(id)]));
const DIAL_DEFAULTS = { cr: 11, isf: 50, tgt: 100, res: 1 };

let nextId = 1;
const entry = (min, g, bolus, u = 0) => ({ id: nextId++, min, g, bolus, u });
const PRESETS = {
  forgot: { controller: false, entries: () => [entry(360, 60, "none"), entry(600, 50, "own", 4.5), entry(840, 70, "own", 6.5)] },
  untreated: { controller: false, entries: () => [entry(360, 60, "none"), entry(600, 50, "none"), entry(840, 70, "none")] },
  stacked: { controller: false, entries: () => [entry(720, 50, "own", 4.5), entry(840, 0, "own", 2), entry(960, 0, "own", 2)] },
};

const svgEl = (name, attrs = {}, parent) => {
  const e = document.createElementNS(NS, name);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  if (parent) parent.append(e);
  return e;
};
const text = (parent, x, y, s, cls, anchor = "start") => {
  const t = svgEl("text", { x, y, class: cls, "text-anchor": anchor }, parent);
  t.textContent = s;
  return t;
};
const el = (name, cls, parent) => {
  const e = document.createElement(name);
  if (cls) e.className = cls;
  if (parent) parent.append(e);
  return e;
};
const icon = (d) => {
  const s = svgEl("svg", { viewBox: "0 0 12 12", class: "ico", "aria-hidden": "true" });
  svgEl("path", { d }, s);
  return s;
};
const PLUS = "M6 2v8M2 6h8";
const MINUS = "M2 6h8";

const fmt = (v, d = 0) => v.toLocaleString(lang, { maximumFractionDigits: d, minimumFractionDigits: d });
const pct = (v) => `${fmt(v)}${lang === "de" ? "\u00a0%" : "%"}`;
const units = (u) => `${fmt(u, 1)}\u00a0${UNIT}`;
const clock = (min) => `${String(Math.floor(min / 60)).padStart(2, "0")}:${String(min % 60).padStart(2, "0")}`;
const snap = (min) => Math.min(LAST_MIN, Math.max(0, Math.round(min / STEP_MIN) * STEP_MIN));

let controller = true;
let entries = [];
let selected = null;
let current = null;
let before = null;
let timer = 0;
let geom = null;
let drag = null;
let lanePiles = new Map();

function setMode(on) {
  controller = on;
  modeAuto.setAttribute("aria-pressed", String(on));
  modeBasal.setAttribute("aria-pressed", String(!on));
  modeHint.textContent = on ? T.auto : T.basal;
}

function setPreset(name) {
  for (const b of presets) b.setAttribute("aria-pressed", String(b.dataset.preset === name));
}

// The day on screen becomes the grey "before" curve; any edit leaves the
// preset. Edits inside one pending run count as one change.
function mark() {
  if (current && !panel.classList.contains("calculating")) before = current;
  setPreset(null);
}

function suggestion(e) {
  if (!e.g) return { refused: false, total_u: 0 };
  const i = current ? Math.min(current.glucose_mg_dl.length - 1, Math.round(e.min / STEP_MIN)) : 0;
  try {
    return JSON.parse(suggest_bolus(JSON.stringify({
      cr_g_per_u: dials.cr.valueAsNumber,
      isf_mg_dl_per_u: dials.isf.valueAsNumber,
      target_mg_dl: dials.tgt.valueAsNumber,
      at_min: e.min,
      carbs_g: e.g,
      // The calculator reads the day's glucose at the meal's minute.
      glucose_now_mg_dl: current ? current.glucose_mg_dl[i] : null,
    })));
  } catch {
    // The calculator refuses below its minimum glucose, as the pump app does.
    return { refused: true, total_u: 0 };
  }
}

const dose = (e) => (e.bolus === "own" ? e.u : e.calcU ?? 0);

function events() {
  const out = [];
  for (const e of entries) {
    if (e.g > 0) out.push({ kind: "meal", min: e.min, g: e.g });
    const u = dose(e);
    if (u >= 0.05) out.push({ kind: "bolus", min: e.min, u });
  }
  return out;
}

function updateOutputs() {
  document.getElementById("cr-out").textContent = `${fmt(dials.cr.valueAsNumber, 1)} ${lang === "de" ? "g/IE" : "g/U"}`;
  document.getElementById("isf-out").textContent = `${fmt(dials.isf.valueAsNumber)} ${GDL}`;
  document.getElementById("tgt-out").textContent = `${fmt(dials.tgt.valueAsNumber)} ${GDL}`;
  document.getElementById("res-out").textContent = `× ${fmt(dials.res.valueAsNumber, 1)}`;
}

function plot(host) {
  const W = host.clientWidth;
  const H = Math.round(W < 520 ? W * 0.62 : W * 0.42);
  if (W < M.l + M.r + 40) {
    geom = { W: Math.max(W, 1), sx: () => 0 };
    return svgEl("svg", { viewBox: `0 0 ${geom.W} 1`, width: geom.W, height: 1, "aria-hidden": "true" });
  }
  const sx = (min) => M.l + (min / 1440) * (W - M.l - M.r);
  // The site's one glucose scale, so the day compares against every chapter.
  const sy = (v) => M.t + (1 - (Math.min(Math.max(v, G_MIN), G_MAX) - G_MIN) / (G_MAX - G_MIN)) * (H - M.t - M.b);
  const axis = H - M.b;
  const svg = svgEl("svg", { viewBox: `0 0 ${W} ${H}`, width: W, height: H, "aria-hidden": "true" });
  svgEl("rect", { x: M.l, y: sy(180), width: W - M.l - M.r, height: sy(70) - sy(180), fill: "var(--band)" }, svg);
  for (const [v, cls] of [[250, "grid"], [180, "limit-high"], [70, "limit-low"]]) {
    svgEl("line", { x1: M.l, x2: W - M.r, y1: sy(v), y2: sy(v), class: cls }, svg);
    text(svg, M.l - 6, sy(v) + 1, fmt(v), "tick", "end");
    text(svg, M.l - 6, sy(v) + 11, fmt(v / MG_PER_MMOL, 1), "tick-sub", "end");
  }
  text(svg, 2, 12, GDL, "tick-sub");
  for (let h = 0; h <= 24; h += 6) {
    svgEl("line", { x1: sx(h * 60), x2: sx(h * 60), y1: axis, y2: axis + 4, class: "edge" }, svg);
    text(svg, sx(h * 60), axis + 16, clock(h * 60), "tick", "middle");
  }
  svgEl("line", { x1: M.l, x2: W - M.r, y1: axis, y2: axis, class: "edge" }, svg);
  // Each moment drops a hairline from the chart down to its marker.
  for (const e of [...entries].sort((a, b) => a.min - b.min)) {
    svgEl("line", { x1: sx(e.min), x2: sx(e.min), y1: M.t, y2: axis, class: e.id === selected ? "moment on" : "moment" }, svg);
  }
  const path = (day) => day.glucose_mg_dl.map((v, i) => `${i ? "L" : "M"}${sx(day.t_min[i]).toFixed(1)},${sy(v).toFixed(1)}`).join("");
  if (before) svgEl("path", { d: path(before), fill: "none", stroke: "var(--muted)", "stroke-width": 1.4, "stroke-linejoin": "round" }, svg);
  svgEl("path", { d: path(current), fill: "none", stroke: "var(--ink)", "stroke-width": 2.8, "stroke-linejoin": "round" }, svg);
  geom = { W, sx };
  return svg;
}

// Geometry is read from the chart on screen, never from a drawn-over copy.
const chart = () => view.querySelector(".plot svg").getBoundingClientRect();

function laneX(min, lane) {
  const s = chart();
  return s.left - lane.getBoundingClientRect().left + (geom.sx(min) * s.width) / geom.W;
}

function laneMin(clientX) {
  const s = chart();
  const xSvg = ((clientX - s.left) * geom.W) / s.width;
  return snap(((xSvg - M.l) / (geom.W - M.l - M.r)) * 1440);
}

function markerBody(btn, e) {
  btn.replaceChildren();
  const top = el("span", "m-top", btn);
  top.textContent = drag?.id === e.id && drag.moved ? clock(e.min) : e.g ? `${fmt(e.g)} g` : "";
  const u = dose(e);
  const bottom = el("span", "m-dose", btn);
  if (u >= 0.05) {
    bottom.append(svgEl("path", { d: "M1 2h10L6 10z" }, svgEl("svg", { viewBox: "0 0 12 12", class: "tri", "aria-hidden": "true" })), fmt(u, 1));
  } else {
    bottom.textContent = T.none.toLowerCase();
    bottom.classList.add("none");
  }
  btn.setAttribute("aria-label", T.marker(e));
}

function renderLane() {
  const lane = el("div", "lane");
  lane.setAttribute("role", "group");
  lane.setAttribute("aria-label", T.day);
  el("span", "lane-hint", lane).textContent = entries.length ? "" : T.laneHint;
  view.insertBefore(lane, view.querySelector(".range-key"));
  // Markers keep their true minute and may overlap. A pile carries a +N
  // badge on its top marker; tapping a selected pile member cycles through
  // the pile, so nothing ever moves or jumps.
  const sorted = [...entries].sort((a, b) => a.min - b.min);
  for (const e of sorted) {
    const btn = el("button", "marker", lane);
    btn.type = "button";
    btn.dataset.id = e.id;
    btn.classList.toggle("on", e.id === selected);
    btn.classList.toggle("fix", !e.g);
    // Keep the whole box inside the lane even at the day's edges.
    btn.style.left = `${Math.max(24, Math.min(lane.clientWidth - 24, laneX(e.min, lane)))}px`;
    markerBody(btn, e);
    btn.addEventListener("pointerdown", (ev) => startDrag(ev, e, btn, lane));
    btn.addEventListener("keydown", keyMove);
    btn.addEventListener("click", (ev) => {
      // Mouse and touch select in pointerup; this serves assistive tech.
      if (ev.detail === 0) selectOrCycle(e);
    });
  }
  const boxes = new Map(sorted.map((e) => [e.id, lane.querySelector(`.marker[data-id="${e.id}"]`).getBoundingClientRect()]));
  lanePiles.clear();
  for (let i = 0; i < sorted.length; i++) {
    const pile = [sorted[i].id];
    while (i + 1 < sorted.length && overlaps(boxes.get(pile[pile.length - 1]), boxes.get(sorted[i + 1].id))) pile.push(sorted[++i].id);
    if (pile.length > 1) {
      const top = pile.includes(selected) ? selected : pile[pile.length - 1];
      for (const id of pile) lanePiles.set(id, pile);
      const badge = el("span", "pile", lane.querySelector(`.marker[data-id="${top}"]`));
      badge.textContent = `+${pile.length - 1}`;
    }
  }
  lane.addEventListener("pointerdown", (ev) => {
    if (ev.target.closest(".marker")) return;
    // Keep the browser from moving focus off the new marker.
    ev.preventDefault();
    const e = entry(laneMin(ev.clientX), 40, "calc");
    mark();
    entries.push(e);
    selected = e.id;
    schedule();
    draw();
    renderInspector();
    view.querySelector(`.marker[data-id="${e.id}"]`)?.focus();
  });
}

const overlaps = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width;

function selectOrCycle(e) {
  const pile = lanePiles.get(e.id);
  if (pile && selected === e.id) {
    selected = pile[(pile.indexOf(e.id) + 1) % pile.length];
  } else {
    selected = e.id;
  }
  draw();
  renderInspector();
  view.querySelector(`.marker[data-id="${selected}"]`)?.focus();
}

function startDrag(ev, e, btn, lane) {
  ev.preventDefault();
  btn.setPointerCapture(ev.pointerId);
  // Claimed at once: a debounced run between down and the move threshold
  // must not detach the marker under the finger.
  drag = { id: e.id, moved: false };
  const x0 = ev.clientX;
  const move = (m) => {
    if (!drag.moved && Math.abs(m.clientX - x0) < 4) return;
    if (!drag.moved) {
      drag.moved = true;
      mark();
      btn.classList.add("dragging");
      selected = e.id;
    }
    const min = laneMin(m.clientX);
    if (min === e.min) return;
    e.min = min;
    btn.style.left = `${Math.max(24, Math.min(lane.clientWidth - 24, laneX(min, lane)))}px`;
    markerBody(btn, e);
    schedule();
  };
  const done = (select) => {
    btn.removeEventListener("pointermove", move);
    btn.removeEventListener("pointerup", up);
    btn.removeEventListener("pointercancel", cancel);
    drag = null;
    if (select) {
      draw();
      renderInspector();
      view.querySelector(`.marker[data-id="${e.id}"]`)?.focus();
    } else {
      draw();
    }
  };
  const up = () => {
    if (drag.moved) {
      done(true);
    } else {
      done(false);
      selectOrCycle(e);
    }
  };
  const cancel = () => done(false);
  btn.addEventListener("pointermove", move);
  btn.addEventListener("pointerup", up);
  btn.addEventListener("pointercancel", cancel);
}

function keyMove(ev) {
  const btn = ev.currentTarget;
  const e = entries.find((o) => o.id === Number(btn.dataset.id));
  const step = ev.shiftKey ? 60 : STEP_MIN;
  if (ev.key === "ArrowLeft" || ev.key === "ArrowRight") {
    ev.preventDefault();
    mark();
    e.min = snap(e.min + (ev.key === "ArrowLeft" ? -step : step));
  } else if (ev.key === "Delete" || ev.key === "Backspace") {
    ev.preventDefault();
    mark();
    const order = [...entries].sort((a, b) => a.min - b.min);
    const i = order.indexOf(e);
    entries = entries.filter((o) => o !== e);
    selected = null;
    schedule();
    draw();
    renderInspector();
    const next = [...view.querySelectorAll(".marker")][Math.min(i, view.querySelectorAll(".marker").length - 1)];
    (next ?? editor.querySelector(".chip"))?.focus();
    return;
  } else {
    return;
  }
  selected = e.id;
  schedule();
  draw();
  renderInspector();
  view.querySelector(`.marker[data-id="${e.id}"]`)?.focus();
}

function legend() {
  const ul = el("ul", "legend");
  for (const [muted, name] of before ? [[true, T.before], [false, T.now]] : [[false, T.now]]) {
    const li = el("li", null, ul);
    const s = svgEl("svg", { width: 28, height: 10, viewBox: "0 0 28 10", "aria-hidden": "true" }, li);
    svgEl("line", { x1: 1, x2: 27, y1: 5, y2: 5, stroke: muted ? "var(--muted)" : "var(--ink)", "stroke-width": muted ? 1.4 : 2.8 }, s);
    li.append(name);
  }
  return ul;
}

// The masthead's range line, for this one day. One live region for the
// page's lifetime, so screen readers hear each new day once.
const summary = el("p", "range-key");
summary.setAttribute("role", "status");
let summaryHtml = "";

function updateSummary() {
  const [inW, belowW, aboveW] = T.keyWords;
  const html = `<strong>${pct(current.tir_pct)}</strong> ${inW} · ${pct(current.tbr_pct)} ${belowW} · ${pct(current.tar_pct)} ${aboveW}`;
  if (html === summaryHtml) return;
  summaryHtml = html;
  summary.innerHTML = html;
}

const mean = (day) => day.glucose_mg_dl.reduce((a, b) => a + b, 0) / day.glucose_mg_dl.length;

function table() {
  const days = before ? [before, current] : [current];
  const row = (label, f) => `<tr><th scope="row">${label}</th>${days.map((d) => `<td>${f(d)}</td>`).join("")}</tr>`;
  const heads = (before ? [T.beforeCol, T.now] : [T.now]).map((h) => `<th scope="col">${h}</th>`).join("");
  totals.innerHTML = `
    <table>
      <thead><tr><th scope="col"><span class="sr">${T.measure}</span></th>${heads}</tr></thead>
      <tbody>
        ${row(T.inRange, (d) => pct(d.tir_pct))}
        ${row(T.below, (d) => pct(d.tbr_pct))}
        ${row(T.above, (d) => pct(d.tar_pct))}
        ${row(T.bolusRow, (d) => units(d.total_bolus_u))}
        ${row(T.meanRow, (d) => `${fmt(mean(d))} ${GDL}`)}
      </tbody>
    </table>`;
}

const sticky = matchMedia("(min-height: 48rem)");

function draw() {
  const focusId = document.activeElement?.closest?.(".marker")?.dataset.id;
  const host = el("div", "plot");
  // Mid-drag the strip stays: the marker under the finger must survive a run.
  view.querySelectorAll(`:scope > .legend, :scope > .plot${drag ? "" : ", :scope > .lane"}`).forEach((n) => n.remove());
  if (!summary.isConnected) view.append(summary);
  view.prepend(legend(), host);
  host.append(plot(host));
  if (!drag) {
    renderLane();
    if (focusId) view.querySelector(`.marker[data-id="${focusId}"]`)?.focus();
  }
  updateSummary();
  table();
  // A focused control under the pinned chart scrolls clear of it.
  document.documentElement.style.scrollPaddingTop = sticky.matches ? `${view.offsetHeight}px` : "";
}

function run() {
  // A Calculator bolus reads the day's glucose, which the day reshapes:
  // re-simulate until the dose list stops moving (it converges immediately
  // unless an edit shifted meals across big glucose swings).
  let evs = events();
  let passes = entries.length + 2;
  for (;;) {
    current = JSON.parse(run_scenario(JSON.stringify({
      seed: SEED,
      controller,
      resistance: dials.res.valueAsNumber,
      admit_mg_dl: ADMIT,
      events: evs,
    })));
    for (const e of entries) if (e.bolus === "calc") e.calcU = suggestion(e).total_u;
    const next = events();
    if (--passes <= 0 || JSON.stringify(next) === JSON.stringify(evs)) break;
    evs = next;
  }
  draw();
  renderInspector();
  panel.classList.remove("calculating");
}

function schedule() {
  panel.classList.add("calculating");
  clearTimeout(timer);
  timer = setTimeout(() => {
    try {
      run();
    } catch {
      fail(T.broke);
    }
  }, DEBOUNCE_MS);
}

function loadPreset(name) {
  const preset = PRESETS[name];
  setMode(preset.controller);
  for (const d of Object.values(dials)) d.value = d.defaultValue;
  updateOutputs();
  entries = preset.entries();
  selected = null;
  before = null;
  renderInspector();
  schedule();
  setPreset(name);
}

function nextSlot() {
  return entries.length ? Math.min(LAST_MIN, Math.max(...entries.map((e) => e.min)) + 120) : 420;
}

function fail(message) {
  clearTimeout(timer);
  panel.querySelectorAll("input, button").forEach((c) => {
    c.disabled = true;
  });
  panel.classList.remove("calculating");
  const p = el("p", "demo-error", panel);
  p.setAttribute("role", "status");
  p.textContent = message;
}

function stepper(label, key, value, onStep, { min = -Infinity, max = Infinity } = {}) {
  const wrap = el("div", "stepper");
  el("span", "st-label", wrap).textContent = label;
  const out = el("output", null, wrap);
  out.textContent = value.text;
  const btn = (d, dir) => {
    const b = el("button", "st-btn", wrap);
    b.type = "button";
    b.dataset.key = `${key}-${dir < 0 ? "less" : "more"}`;
    b.setAttribute("aria-label", `${dir < 0 ? T.less : T.more} ${label.toLowerCase()}`);
    b.append(icon(dir < 0 ? MINUS : PLUS));
    b.disabled = dir < 0 ? value.raw <= min : value.raw >= max;
    b.addEventListener("click", () => onStep(dir));
    return b;
  };
  wrap.append(btn(-1, -1), out, btn(1, 1));
  return wrap;
}

function renderInspector() {
  const keepFocus = document.activeElement?.closest?.("#inspector") ? document.activeElement.dataset.key : null;
  editor.replaceChildren();
  const e = entries.find((o) => o.id === selected);
  dayTitle.textContent = e ? (e.g ? T.mealAt(clock(e.min)) : T.corrAt(clock(e.min))) : T.day;
  if (!e) {
    el("p", "play-empty", editor).textContent = T.empty;
    editor.append(actions(null));
    restoreFocus(keepFocus);
    return;
  }
  const grid = el("div", "insp", editor);
  grid.append(stepper(T.mealTime, "time", { text: clock(e.min), raw: e.min }, (d) => set(() => (e.min = snap(e.min + d * STEP_MIN))), { min: 0, max: LAST_MIN }));
  if (e.g) grid.append(stepper(T.carbs, "carbs", { text: `${fmt(e.g)} g`, raw: e.g }, (d) => set(() => (e.g = Math.max(5, e.g + d * 5))), { min: 5, max: 200 }));
  if (e.g) {
    const s = suggestion(e);
    const rowEl = el("div", "seg-row", grid);
    el("span", "st-label", rowEl).textContent = T.bolus;
    const seg = el("span", "seg", rowEl);
    for (const [k, name] of [["none", T.none], ["calc", s.refused ? T.calcLow : s.total_u < 0.05 ? T.noDose : T.calc(units(s.total_u))], ["own", T.own]]) {
      const b = el("button", null, seg);
      b.type = "button";
      b.dataset.key = `bolus-${k}`;
      b.textContent = name;
      b.setAttribute("aria-pressed", String(e.bolus === k));
      b.disabled = (k === "calc" && s.refused) || (k === "calc" && s.total_u < 0.05);
      b.addEventListener("click", () => set(() => {
        if (k === "own" && e.bolus !== "own") e.u = s.total_u || 1;
        e.bolus = k;
      }));
    }
  }
  if (e.bolus === "own") grid.append(stepper(e.g ? T.ownDose : T.dose, "dose", { text: units(e.u), raw: e.u }, (d) => set(() => (e.u = Math.max(0.5, e.u + d * 0.5))), { min: 0.5, max: 30 }));
  editor.append(actions(e));
  restoreFocus(keepFocus);
}

function restoreFocus(key) {
  if (!key) return;
  (editor.querySelector(`[data-key="${key}"]:not(:disabled)`) ?? editor.querySelector("button:not(:disabled)"))?.focus();
}

function set(fn) {
  mark();
  fn();
  schedule();
  draw();
  renderInspector();
}

function actions(e) {
  const wrap = el("div", "play-actions");
  if (e) {
    const done = el("button", "chip", wrap);
    done.type = "button";
    done.dataset.key = "done";
    done.textContent = T.done;
    done.addEventListener("click", () => {
      selected = null;
      draw();
      renderInspector();
      view.querySelector(`.marker[data-id="${e.id}"]`)?.focus();
    });
    const remove = el("button", "chip", wrap);
    remove.type = "button";
    remove.dataset.key = "remove";
    remove.textContent = T.remove;
    remove.addEventListener("click", () => {
      mark();
      const order = [...entries].sort((a, b) => a.min - b.min);
      const i = order.indexOf(e);
      entries = entries.filter((o) => o !== e);
      selected = null;
      schedule();
      draw();
      renderInspector();
      const next = [...view.querySelectorAll(".marker")][Math.min(i, view.querySelectorAll(".marker").length - 1)];
      (next ?? editor.querySelector(".chip"))?.focus();
    });
  }
  const add = el("button", "chip", wrap);
  add.type = "button";
  add.dataset.key = "add";
  add.textContent = T.addCorrection;
  add.addEventListener("click", () => {
    mark();
    const c = entry(nextSlot(), 0, "own", 1);
    entries.push(c);
    selected = c.id;
    schedule();
    draw();
    renderInspector();
    view.querySelector(`.marker[data-id="${c.id}"]`)?.focus();
  });
  return wrap;
}

async function start() {
  try {
    await init();
  } catch {
    fail(T.error);
    return;
  }
  setMode(true);
  updateOutputs();
  renderInspector();
  const v = el("div", "virtual", panel);
  v.textContent = T.virtual;
  run();
  for (const b of presets) b.addEventListener("click", () => loadPreset(b.dataset.preset));
  for (const b of [modeAuto, modeBasal]) {
    b.addEventListener("click", () => {
      const on = b === modeAuto;
      if (controller === on) return;
      mark();
      setMode(on);
      schedule();
    });
  }
  // Every dial can reshape the day now: a Calculator bolus follows them.
  for (const d of Object.values(dials)) {
    let gesture = false;
    d.addEventListener("input", () => {
      if (!gesture) mark();
      gesture = true;
      updateOutputs();
      schedule();
    });
    for (const end of ["pointerup", "blur"]) d.addEventListener(end, () => {
      gesture = false;
    });
  }
  let w = innerWidth;
  let frame = 0;
  addEventListener("resize", () => {
    if (innerWidth === w) return;
    w = innerWidth;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(draw);
  });
}

start();

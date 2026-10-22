// The playground chapter: the reader composes a day from meals and
// boluses, flips the pump's Auto mode, and the simulator answers live.
// All physics stays in the wasm bindings; this file only moves state.
import init, { run_scenario, suggest_bolus } from "./sim/tir_sim.js";

const lang = document.documentElement.lang === "de" ? "de" : "en";
const UNIT = lang === "de" ? "IE" : "U";
const GDL = lang === "de" ? "mg/dl" : "mg/dL";
const T = {
  en: {
    meal: "Meal",
    bolus: "Bolus",
    mealTime: "Meal time",
    bolusTime: "Bolus time",
    carbs: "Carbohydrates",
    dose: "Dose",
    give: (u) => `Give ${u}`,
    setTo: (u) => `Set to ${u}`,
    giveLabel: (u, t) => `Give the suggested ${u} at ${t}`,
    setLabel: (u, t) => `Set the bolus at ${t} to the suggested ${u}`,
    given: "Given",
    none: "No bolus suggested",
    tooLow: "Too low for a bolus",
    drop: (t) => `Delete the entry at ${t}`,
    empty: "No meals or boluses yet. Add one below, or start from a ready-made day above.",
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
    meal: "Mahlzeit",
    bolus: "Bolus",
    mealTime: "Uhrzeit der Mahlzeit",
    bolusTime: "Uhrzeit des Bolus",
    carbs: "Kohlenhydrate",
    dose: "Dosis",
    give: (u) => `${u} geben`,
    setTo: (u) => `Auf ${u} setzen`,
    giveLabel: (u, t) => `Empfohlene ${u} um ${t} geben`,
    setLabel: (u, t) => `Bolus um ${t} auf die empfohlenen ${u} setzen`,
    given: "Gegeben",
    none: "Kein Bolus empfohlen",
    tooLow: "Zu tief für einen Bolus",
    drop: (t) => `Eintrag um ${t} löschen`,
    empty: "Noch keine Mahlzeiten oder Bolusgaben. Fügen Sie unten eine hinzu oder starten Sie oben mit einem fertigen Tag.",
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
const DEBOUNCE_MS = 150;
const SAMPLE_MIN = 15;
const LAST_SLOT_MIN = 1425;
const NEXT_SLOT_MIN = 180;
const FIRST_SLOT_MIN = 420;

const panel = document.getElementById("play");
const view = document.getElementById("view");
const totals = document.getElementById("totals");
const editor = document.getElementById("events");
const modeAuto = document.getElementById("mode-auto");
const modeBasal = document.getElementById("mode-basal");
const modeHint = document.getElementById("mode-hint");
const addMeal = document.getElementById("add-meal");
const addBolus = document.getElementById("add-bolus");
const presets = [...panel.querySelectorAll("[data-preset]")];
const dials = {
  cr: document.getElementById("cr"),
  isf: document.getElementById("isf"),
  tgt: document.getElementById("tgt"),
  res: document.getElementById("res"),
};

const meal = (min, g) => ({ kind: "meal", min, g });
const bolus = (min, u) => ({ kind: "bolus", min, u });
const PRESETS = {
  forgot: { controller: false, events: () => [meal(360, 60), meal(600, 50), meal(840, 70), bolus(600, 4.5), bolus(840, 6.5)] },
  untreated: { controller: false, events: () => [meal(360, 60), meal(600, 50), meal(840, 70)] },
  stacked: { controller: false, events: () => [meal(720, 50), bolus(720, 4.5), bolus(840, 2), bolus(960, 2)] },
};

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
const units = (u) => `${fmt(u, 1)}\u00a0${UNIT}`;
const clock = (h) => `${String(Math.floor(h)).padStart(2, "0")}:${String(Math.round((h % 1) * 60)).padStart(2, "0")}`;
const lineAttrs = (muted) => ({
  stroke: muted ? "var(--muted)" : "var(--ink)", "stroke-width": muted ? 1.4 : 2.8, "stroke-linejoin": "round",
});

let controller = true;
let events = [];
let current = null;
let before = null;
let timer = 0;
const rowEvent = new WeakMap();

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

function changed(focus) {
  mark();
  renderEvents(focus);
  schedule();
}

const bolusAt = (min) => events.find((o) => o.kind === "bolus" && o.min === min);

function suggestion(m) {
  const req = {
    cr_g_per_u: dials.cr.valueAsNumber,
    isf_mg_dl_per_u: dials.isf.valueAsNumber,
    target_mg_dl: dials.tgt.valueAsNumber,
    at_min: m.min,
    carbs_g: m.g,
    // The calculator reads the day's glucose at the meal's minute.
    glucose_now_mg_dl: current ? current.glucose_mg_dl[Math.min(current.glucose_mg_dl.length - 1, Math.round(m.min / SAMPLE_MIN))] : null,
  };
  try {
    return JSON.parse(suggest_bolus(JSON.stringify(req)));
  } catch {
    // The calculator refuses below its minimum glucose, as the pump app does.
    return null;
  }
}

function updateOutputs() {
  document.getElementById("cr-out").textContent = `${fmt(dials.cr.valueAsNumber, 1)} ${lang === "de" ? "g/IE" : "g/U"}`;
  document.getElementById("isf-out").textContent = `${fmt(dials.isf.valueAsNumber)} ${GDL}`;
  document.getElementById("tgt-out").textContent = `${fmt(dials.tgt.valueAsNumber)} ${GDL}`;
  document.getElementById("res-out").textContent = `× ${fmt(dials.res.valueAsNumber, 1)}`;
}

function refreshChips() {
  for (const give of editor.querySelectorAll(".give")) {
    const e = rowEvent.get(give.closest(".event-row"));
    const s = suggestion(e);
    const at = clock(e.min / 60);
    const hit = bolusAt(e.min);
    give.disabled = true;
    give.removeAttribute("aria-label");
    if (!s) give.textContent = T.tooLow;
    else if (s.total_u === 0) give.textContent = T.none;
    else if (hit && hit.u === s.total_u) give.textContent = T.given;
    else {
      give.disabled = false;
      give.textContent = hit ? T.setTo(units(s.total_u)) : T.give(units(s.total_u));
      give.setAttribute("aria-label", (hit ? T.setLabel : T.giveLabel)(units(s.total_u), at));
    }
  }
}

function input(type, field, label) {
  const i = document.createElement("input");
  i.type = type;
  i.dataset.field = field;
  i.setAttribute("aria-label", label);
  return i;
}

// Rebuilds the rows; `focus` is [event, field] to land on afterwards, by
// default whatever had focus, so keyboard users keep their place.
function renderEvents(focus) {
  const active = document.activeElement?.closest?.(".event-row");
  const keep = focus ?? (active ? [rowEvent.get(active), document.activeElement.dataset.field] : null);
  events.sort((a, b) => a.min - b.min);
  editor.replaceChildren();
  if (!events.length) {
    const p = document.createElement("p");
    p.className = "play-empty";
    p.textContent = T.empty;
    editor.append(p);
  }
  for (const e of events) {
    const isMeal = e.kind === "meal";
    const row = document.createElement("div");
    row.className = "event-row";
    rowEvent.set(row, e);
    const kind = document.createElement("span");
    kind.className = "kind";
    kind.textContent = isMeal ? T.meal : T.bolus;
    const time = input("time", "time", isMeal ? T.mealTime : T.bolusTime);
    time.step = SAMPLE_MIN * 60;
    time.value = clock(e.min / 60);
    // Chromium fires change on every complete time while typing, so the
    // value applies in place and the rows re-sort only once focus leaves.
    let moved = false;
    time.addEventListener("change", () => {
      const parts = time.value.split(":").map(Number);
      if (parts.length !== 2 || parts.some(Number.isNaN)) {
        time.value = clock(e.min / 60);
        return;
      }
      e.min = Math.min(LAST_SLOT_MIN, parts[0] * 60 + parts[1]);
      moved = true;
      mark();
      refreshChips();
      schedule();
    });
    time.addEventListener("focusout", (ev) => {
      if (!moved) return;
      moved = false;
      row.querySelector(".drop").setAttribute("aria-label", T.drop(clock(e.min / 60)));
      if (events.every((o, i) => i === 0 || events[i - 1].min <= o.min)) return;
      const next = ev.relatedTarget?.closest?.(".event-row");
      renderEvents(next ? [rowEvent.get(next), ev.relatedTarget.dataset.field] : undefined);
    });
    const amount = input("number", "amount", isMeal ? T.carbs : T.dose);
    amount.inputMode = "decimal";
    [amount.min, amount.max, amount.step] = isMeal ? [1, 200, 1] : [0.5, 30, 0.5];
    amount.value = isMeal ? e.g : e.u;
    amount.addEventListener("change", () => {
      let v = amount.valueAsNumber;
      if (Number.isNaN(v)) v = isMeal ? e.g : e.u;
      v = Math.min(Number(amount.max), Math.max(Number(amount.min), v));
      if (isMeal) e.g = v;
      else e.u = v;
      amount.value = v;
      mark();
      refreshChips();
      schedule();
    });
    const qty = document.createElement("span");
    qty.className = "qty";
    const unit = document.createElement("span");
    unit.className = "unit";
    unit.textContent = isMeal ? "g" : UNIT;
    qty.append(amount, unit);
    row.append(kind, time, qty);
    if (isMeal) {
      const give = document.createElement("button");
      give.type = "button";
      give.className = "chip give";
      give.dataset.field = "give";
      give.addEventListener("click", () => {
        const s = suggestion(e);
        if (!s || s.total_u === 0) return;
        const hit = bolusAt(e.min);
        if (hit) hit.u = s.total_u;
        else events.push(bolus(e.min, s.total_u));
        changed([e, "give"]);
      });
      row.append(give);
    }
    const drop = document.createElement("button");
    drop.type = "button";
    drop.className = "drop";
    drop.dataset.field = "drop";
    drop.setAttribute("aria-label", T.drop(clock(e.min / 60)));
    el("path", { d: "M2 2l8 8M10 2l-8 8" }, el("svg", { viewBox: "0 0 12 12", "aria-hidden": "true" }, drop));
    drop.addEventListener("click", () => {
      const i = events.indexOf(e);
      events.splice(i, 1);
      changed([events[Math.min(i, events.length - 1)], "drop"]);
    });
    row.append(drop);
    editor.append(row);
  }
  refreshChips();
  if (keep) {
    const [e, field] = keep;
    const row = [...editor.children].find((r) => rowEvent.get(r) === e);
    const target = row?.querySelector(`[data-field="${field}"]`);
    if (target && !target.disabled) target.focus();
    else if (row) row.querySelector('[data-field="drop"]').focus();
    else if (focus) addMeal.focus();
  }
}

function plot(host) {
  const W = host.clientWidth;
  const H = Math.round(W < 520 ? W * 0.72 : W * 0.48);
  const m = { l: 46, r: 12, t: 26, b: 34 };
  if (W < m.l + m.r + 40) return el("svg", { viewBox: `0 0 ${W} ${Math.max(H, 1)}`, width: W, height: Math.max(H, 1), "aria-hidden": "true" });
  const sx = (h) => m.l + (h / 24) * (W - m.l - m.r);
  // The site's one glucose scale, so the day compares against every chapter.
  const sy = (v) => m.t + (1 - (Math.min(Math.max(v, G_MIN), G_MAX) - G_MIN) / (G_MAX - G_MIN)) * (H - m.t - m.b);
  const axis = H - m.b;
  const svg = el("svg", { viewBox: `0 0 ${W} ${H}`, width: W, height: H, "aria-hidden": "true" });
  el("rect", { x: m.l, y: sy(180), width: W - m.l - m.r, height: sy(70) - sy(180), fill: "var(--band)" }, svg);
  for (const [v, cls] of [[250, "grid"], [180, "limit-high"], [70, "limit-low"]]) {
    el("line", { x1: m.l, x2: W - m.r, y1: sy(v), y2: sy(v), class: cls }, svg);
    text(svg, m.l - 6, sy(v) + 1, fmt(v), "tick", "end");
    text(svg, m.l - 6, sy(v) + 11, fmt(v / MG_PER_MMOL, 1), "tick-sub", "end");
  }
  text(svg, 2, 12, GDL, "tick-sub");
  for (let h = 0; h <= 24; h += 6) {
    el("line", { x1: sx(h), x2: sx(h), y1: axis, y2: axis + 4, class: "edge" }, svg);
    text(svg, sx(h), axis + 16, clock(h), "tick", "middle");
  }
  el("line", { x1: m.l, x2: W - m.r, y1: axis, y2: axis, class: "edge" }, svg);
  // A meal and its bolus share a minute: grams sit above the units, both centred.
  const bolusMin = new Set(current.boluses.map((b) => b[0]));
  for (const [min, g] of current.meals) {
    const x = sx(min / 60);
    el("line", { x1: x, x2: x, y1: axis, y2: axis - 8, stroke: "var(--ink)", "stroke-width": 2 }, svg);
    text(svg, x, bolusMin.has(min) ? axis - 24 : axis - 11, `${fmt(g)} g`, "tick-sub", "middle");
  }
  for (const [min, u] of current.boluses) {
    const x = sx(min / 60);
    el("path", { d: `M${x},${axis} L${x + 5},${axis - 11} L${x - 5},${axis - 11} Z`, fill: "var(--ink)" }, svg);
    text(svg, x, axis - 14, units(u), "tick-sub", "middle");
  }
  const path = (day) => day.glucose_mg_dl.map((v, i) => `${i ? "L" : "M"}${sx(day.t_min[i] / 60).toFixed(1)},${sy(v).toFixed(1)}`).join("");
  if (before) el("path", { d: path(before), fill: "none", ...lineAttrs(true) }, svg);
  el("path", { d: path(current), fill: "none", ...lineAttrs(false) }, svg);
  return svg;
}

function legend() {
  const ul = document.createElement("ul");
  ul.className = "legend";
  for (const [muted, name] of before ? [[true, T.before], [false, T.now]] : [[false, T.now]]) {
    const li = document.createElement("li");
    const svg = el("svg", { width: 28, height: 10, viewBox: "0 0 28 10", "aria-hidden": "true" });
    el("line", { x1: 1, x2: 27, y1: 5, y2: 5, ...lineAttrs(muted) }, svg);
    li.append(svg, name);
    ul.append(li);
  }
  return ul;
}

// The masthead's range line, for this one day. One live region for the
// page's lifetime, so screen readers hear each new day once.
const summary = document.createElement("p");
summary.className = "range-key";
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
  const host = document.createElement("div");
  host.className = "plot";
  view.querySelectorAll(":scope > .legend, :scope > .plot").forEach((n) => n.remove());
  if (!summary.isConnected) view.append(summary);
  view.prepend(legend(), host);
  host.append(plot(host));
  updateSummary();
  table();
  // A row focused under the pinned chart scrolls clear of it.
  document.documentElement.style.scrollPaddingTop = sticky.matches ? `${view.offsetHeight}px` : "";
}

function scenario() {
  return JSON.parse(run_scenario(JSON.stringify({
    seed: SEED,
    controller,
    resistance: dials.res.valueAsNumber,
    admit_mg_dl: ADMIT,
    events,
  })));
}

function run() {
  current = scenario();
  refreshChips();
  draw();
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

// A new story, not an edit: no "before" curve from whatever was on screen.
function loadPreset(name) {
  const preset = PRESETS[name];
  setMode(preset.controller);
  for (const d of Object.values(dials)) d.value = d.defaultValue;
  updateOutputs();
  events = preset.events();
  before = null;
  renderEvents();
  schedule();
  setPreset(name);
}

// Time edits re-sort only on focusout, so the latest event is found by value.
function nextSlot() {
  return events.length ? Math.min(LAST_SLOT_MIN, Math.max(...events.map((e) => e.min)) + NEXT_SLOT_MIN) : FIRST_SLOT_MIN;
}

function fail(message) {
  clearTimeout(timer);
  panel.querySelectorAll("input, button").forEach((c) => {
    c.disabled = true;
  });
  panel.classList.remove("calculating");
  const p = document.createElement("p");
  p.className = "demo-error";
  p.setAttribute("role", "status");
  p.textContent = message;
  panel.append(p);
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
  renderEvents();
  const v = document.createElement("div");
  v.className = "virtual";
  v.textContent = T.virtual;
  panel.append(v);
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
  // The calculator dials only move the suggestions; the day is unchanged.
  for (const d of [dials.cr, dials.isf, dials.tgt]) {
    d.addEventListener("input", () => {
      setPreset(null);
      updateOutputs();
      refreshChips();
    });
  }
  // One drag, or one run of arrow keys while focused, is one change.
  let gesture = false;
  dials.res.addEventListener("input", () => {
    if (!gesture) mark();
    gesture = true;
    updateOutputs();
    schedule();
  });
  for (const end of ["pointerup", "blur"]) dials.res.addEventListener(end, () => {
    gesture = false;
  });
  addMeal.addEventListener("click", () => {
    const e = meal(nextSlot(), 40);
    events.push(e);
    changed([e, "time"]);
  });
  addBolus.addEventListener("click", () => {
    const e = bolus(nextSlot(), 1);
    events.push(e);
    changed([e, "time"]);
  });
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

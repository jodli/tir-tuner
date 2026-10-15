// The playground chapter: the reader composes a day from meals and
// boluses, flips the pump's Auto mode, and the simulator answers live.
// All physics stays in the wasm bindings; this file only moves state.
import init, { run_scenario, suggest_bolus } from "./sim/tir_sim.js";

const lang = document.documentElement.lang === "de" ? "de" : "en";
const UNIT = lang === "de" ? "IE" : "U";
const GDL = lang === "de" ? "mg/dl" : "mg/dL";
const T = {
  en: {
    give: (u, t) => `Set the bolus at ${t} to the suggested ${u}`,
    drop: (t) => `Delete the entry at ${t}`,
    meal: "Meal",
    bolus: "Bolus",
    mealTime: "Meal time",
    bolusTime: "Bolus time",
    carbs: "Carbohydrates",
    dose: "Dose",
    inRange: "In range 70-180",
    below: "Below 70",
    above: "Above 180",
    bolusRow: "Bolus insulin",
    meanRow: "Mean glucose",
    measure: "Measure",
    day: "The day",
    virtual: "Virtual patient. The same model as the report chapters, nothing added.",
    error: "The playground could not be loaded. Check the connection and reload the page.",
  },
  de: {
    give: (u, t) => `Bolus um ${t} auf die empfohlenen ${u} setzen`,
    drop: (t) => `Eintrag um ${t} löschen`,
    meal: "Mahlzeit",
    bolus: "Bolus",
    mealTime: "Mahlzeitenzeit",
    bolusTime: "Boluszeit",
    carbs: "Kohlenhydrate",
    dose: "Dosis",
    inRange: "Im Bereich 70-180",
    below: "Unter 70",
    above: "Über 180",
    bolusRow: "Bolus-Insulin",
    meanRow: "Mittlerer Blutzucker",
    measure: "Messgröße",
    day: "Der Tag",
    virtual: "Virtueller Patient. Dasselbe Modell wie in den Berichtskapiteln, nichts hinzugefügt.",
    error: "Die Spielwiese konnte nicht geladen werden. Prüfen Sie die Verbindung und laden Sie die Seite neu.",
  },
}[lang];

const NS = "http://www.w3.org/2000/svg";
const MG_PER_MMOL = 18.0156;
const G_MIN = 40;
const G_MAX = 280;
const SEED = 1;
const ADMIT = 100;
const DEBOUNCE_MS = 150;

const panel = document.getElementById("play");
const editor = document.getElementById("events");
const modeAuto = document.getElementById("mode-auto");
const modeBasal = document.getElementById("mode-basal");
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
const clock = (h) => `${String(Math.floor(h)).padStart(2, "0")}:${String(Math.round((h % 1) * 60)).padStart(2, "0")}`;

let controller = true;
let events = [];
let current = null;
let timer = 0;

function setMode(on) {
  controller = on;
  modeAuto.setAttribute("aria-pressed", String(on));
  modeBasal.setAttribute("aria-pressed", String(!on));
}

function changed() {
  renderEvents();
  schedule();
}

function suggestion(m) {
  const req = {
    cr_g_per_u: dials.cr.valueAsNumber,
    isf_mg_dl_per_u: dials.isf.valueAsNumber,
    target_mg_dl: dials.tgt.valueAsNumber,
    at_min: m.min,
    carbs_g: m.g,
    // The calculator reads the day's glucose at the meal's minute.
    glucose_now_mg_dl: current ? current.glucose_mg_dl[Math.min(95, Math.round(m.min / 15))] ?? null : null,
  };
  return JSON.parse(suggest_bolus(JSON.stringify(req)));
}

function updateOutputs() {
  document.getElementById("cr-out").textContent = `${fmt(dials.cr.valueAsNumber, 1)} ${lang === "de" ? "g/IE" : "g/U"}`;
  document.getElementById("isf-out").textContent = `${fmt(dials.isf.valueAsNumber)} ${GDL}`;
  document.getElementById("tgt-out").textContent = `${fmt(dials.tgt.valueAsNumber)} ${GDL}`;
  document.getElementById("res-out").textContent = `× ${fmt(dials.res.valueAsNumber, 1)}`;
}

function renderEvents() {
  events.sort((a, b) => a.min - b.min);
  editor.replaceChildren();
  for (const e of events) {
    const isMeal = e.kind === "meal";
    const row = document.createElement("div");
    row.className = "event-row";
    const time = document.createElement("input");
    time.type = "time";
    time.step = 900;
    time.value = clock(e.min / 60);
    time.setAttribute("aria-label", isMeal ? T.mealTime : T.bolusTime);
    time.addEventListener("change", () => {
      const parts = time.value.split(":").map(Number);
      if (parts.length !== 2 || parts.some(Number.isNaN)) {
        time.value = clock(e.min / 60);
        return;
      }
      e.min = Math.min(1439, parts[0] * 60 + parts[1]);
      time.value = clock(e.min / 60);
      changed();
    });
    const amount = document.createElement("input");
    amount.type = "number";
    if (isMeal) {
      amount.min = 1;
      amount.max = 200;
      amount.step = 1;
      amount.value = e.g;
      amount.setAttribute("aria-label", T.carbs);
    } else {
      amount.min = 0.5;
      amount.max = 30;
      amount.step = 0.5;
      amount.value = e.u;
      amount.setAttribute("aria-label", T.dose);
    }
    amount.addEventListener("change", () => {
      let v = amount.valueAsNumber;
      if (Number.isNaN(v)) v = isMeal ? e.g : e.u;
      v = Math.min(Number(amount.max), Math.max(Number(amount.min), v));
      if (isMeal) e.g = v;
      else e.u = v;
      amount.value = v;
      changed();
    });
    const unit = document.createElement("span");
    unit.className = "unit";
    unit.textContent = isMeal ? "g" : UNIT;
    row.append(time, amount, unit);
    if (isMeal) {
      const s = suggestion(e);
      const give = document.createElement("button");
      give.type = "button";
      give.className = "chip give";
      give.textContent = `+ ${fmt(s.total_u, 1)} ${UNIT}`;
      give.setAttribute("aria-label", T.give(`${fmt(s.total_u, 1)} ${UNIT}`, clock(e.min / 60)));
      give.addEventListener("click", () => {
        const hit = events.find((o) => o.kind === "bolus" && o.min === e.min);
        if (hit) hit.u = s.total_u;
        else events.push(bolus(e.min, s.total_u));
        changed();
      });
      row.append(give);
    }
    const drop = document.createElement("button");
    drop.type = "button";
    drop.className = "drop";
    drop.textContent = "×";
    drop.setAttribute("aria-label", T.drop(clock(e.min / 60)));
    drop.addEventListener("click", () => {
      events.splice(events.indexOf(e), 1);
      changed();
    });
    row.append(drop);
    editor.append(row);
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
  const bolusMin = new Set(current.boluses.map((b) => b[0]));
  const mealMin = new Set(current.meals.map((mm) => mm[0]));
  for (const [min, g] of current.meals) {
    const x = sx(min / 60);
    const shared = bolusMin.has(min);
    el("line", { x1: x, x2: x, y1: axis, y2: axis - 8, stroke: "var(--ink)", "stroke-width": 2 }, svg);
    text(svg, shared ? x - 5 : x, axis - 11, `${fmt(g)} g`, "tick-sub", shared ? "end" : "middle");
  }
  for (const [min, u] of current.boluses) {
    const x = sx(min / 60);
    const shared = mealMin.has(min);
    el("path", { d: `M${x},${axis} L${x + 5},${axis - 11} L${x - 5},${axis - 11} Z`, fill: "var(--ink)" }, svg);
    text(svg, shared ? x + 7 : x, axis - 14, `${fmt(u, 1)} ${UNIT}`, "tick-sub", shared ? "start" : "middle");
  }
  const path = current.glucose_mg_dl.map((v, i) => `${i ? "L" : "M"}${sx(current.t_min[i] / 60).toFixed(1)},${sy(v).toFixed(1)}`).join("");
  el("path", { d: path, fill: "none", stroke: "var(--ink)", "stroke-width": 2.8, "stroke-linejoin": "round" }, svg);
  return svg;
}

function table() {
  const wrap = document.createElement("div");
  wrap.className = "play-table";
  const row = (label, v) => `<tr><th scope="row">${label}</th><td>${v}</td></tr>`;
  const mg = current.glucose_mg_dl.reduce((a, b) => a + b, 0) / current.glucose_mg_dl.length;
  wrap.innerHTML = `
    <table>
      <thead><tr><th scope="col"><span class="sr">${T.measure}</span></th><th scope="col">${T.day}</th></tr></thead>
      <tbody>
        ${row(T.inRange, pct(current.tir_pct))}
        ${row(T.below, pct(current.tbr_pct))}
        ${row(T.above, pct(current.tar_pct))}
        ${row(T.bolusRow, `${fmt(current.total_bolus_u, 1)} ${UNIT}`)}
        ${row(T.meanRow, `${fmt(mg)} ${GDL}`)}
      </tbody>
    </table>`;
  return wrap;
}

function draw() {
  panel.querySelectorAll(".plot, .play-table").forEach((e) => e.remove());
  const host = document.createElement("div");
  host.className = "plot";
  panel.insertBefore(host, panel.querySelector(".virtual"));
  host.append(plot(host));
  panel.insertBefore(table(), panel.querySelector(".virtual"));
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
  renderEvents();
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
      fail();
    }
  }, DEBOUNCE_MS);
}

const DIAL_DEFAULTS = { cr: 11, isf: 50, tgt: 100, res: 1 };

function loadPreset(name) {
  const preset = PRESETS[name];
  setMode(preset.controller);
  events = preset.events();
  for (const [id, v] of Object.entries(DIAL_DEFAULTS)) {
    dials[id].value = v;
  }
  updateOutputs();
  changed();
}

function fail() {
  clearTimeout(timer);
  panel.querySelectorAll("input, button").forEach((c) => {
    c.disabled = true;
  });
  panel.classList.remove("calculating");
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
  current = scenario();
  const v = document.createElement("div");
  v.className = "virtual";
  v.textContent = T.virtual;
  panel.append(v);
  run();
  updateOutputs();
  panel.querySelectorAll("[data-preset]").forEach((b) => {
    b.addEventListener("click", () => loadPreset(b.dataset.preset));
  });
  for (const b of [modeAuto, modeBasal]) {
    b.addEventListener("click", () => {
      const on = b === modeAuto;
      if (controller !== on) {
        setMode(on);
        schedule();
      }
    });
  }
  for (const d of Object.values(dials)) {
    d.addEventListener("input", () => {
      updateOutputs();
      // Suggestions read the day's glucose, so every calculator dial but
      // resistance can change them.
      if (d !== dials.res) renderEvents();
      schedule();
    });
  }
  document.getElementById("add-meal").addEventListener("click", () => {
    events.push(meal(420, 40));
    changed();
  });
  document.getElementById("add-bolus").addEventListener("click", () => {
    events.push(bolus(420, 1));
    changed();
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

---
title: "The playground: a day in your hand"
lang: en
alt: de/play.html
sources:
  - tir-tuner-web/src/lib.rs
  - tir-tuner-bolus/src/lib.rs
---

# The playground: a day in your hand

The other chapters show days the virtual patient lived through. This one is yours: add meals, decide on boluses, switch the pump's Auto mode on or off, and the simulator answers with the day that would follow.

<figure class="panel playground" id="play">
<figcaption><strong>The playground</strong><span>Meals, boluses, Auto mode: move the levers and the day redraws.</span></figcaption>
<div class="play-actions" role="group" aria-label="Ready-made days">
<button type="button" class="chip" data-preset="forgot">Forgot the breakfast bolus</button>
<button type="button" class="chip" data-preset="untreated">Untreated day</button>
<button type="button" class="chip" data-preset="stacked">Stacked corrections</button>
</div>
<div class="demo-controls" role="group" aria-label="Mode">
<span class="seg">
<button type="button" id="mode-auto" aria-pressed="true">Auto mode</button>
<button type="button" id="mode-basal" aria-pressed="false">Basal only</button>
</span>
</div>
<div class="demo-controls">
<label for="cr">Carb ratio</label>
<input type="range" id="cr" min="5" max="20" step="0.5" value="11">
<output for="cr" id="cr-out">11 g/U</output>
</div>
<div class="demo-controls">
<label for="isf">Correction factor</label>
<input type="range" id="isf" min="10" max="200" step="5" value="50">
<output for="isf" id="isf-out">50 mg/dL</output>
</div>
<div class="demo-controls">
<label for="tgt">Target</label>
<input type="range" id="tgt" min="80" max="190" step="1" value="100">
<output for="tgt" id="tgt-out">100 mg/dL</output>
</div>
<div class="demo-controls">
<label for="res">Insulin resistance</label>
<input type="range" id="res" min="1" max="2" step="0.1" value="1">
<output for="res" id="res-out">× 1.0</output>
</div>
<div class="play-editor" id="events"></div>
<div class="play-actions">
<button type="button" class="chip" id="add-meal">Add meal</button>
<button type="button" class="chip" id="add-bolus">Add bolus</button>
</div>
</figure>

## Read it like a report

The curve and the numbers follow the rules from [the numbers chapter](math-and-metrics.html): in range is 70 to 180 mg/dL, the band is drawn behind the curve, and the table sums the whole day. The suggestion on each meal row comes from the same calculator as the dosing chapter: your carb ratio, correction factor and target, plus the day's glucose at that minute for the correction.

Three things keep it honest. This is a virtual patient, not you. The day has no exercise, and the sensor noise stays on, so the curve wobbles like a real one. And the resistance dial is a what-if, not a diagnosis: push it up with Auto mode on and the loop answers with more insulin, which drives the day down rather than up. A real sick day needs your team, not a slider.

<script type="module" src="assets/playground.js"></script>

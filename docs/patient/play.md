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
<figcaption><strong>The playground</strong><span>The black curve is your day as it stands. The grey curve is the day before your last change, so every move shows what it did.</span></figcaption>
<fieldset class="play-group">
<legend>Start from a ready-made day</legend>
<div class="play-actions">
<button type="button" class="chip" data-preset="forgot" aria-pressed="false">Forgot the breakfast bolus</button>
<button type="button" class="chip" data-preset="untreated" aria-pressed="false">Untreated day</button>
<button type="button" class="chip" data-preset="stacked" aria-pressed="false">Stacked corrections</button>
</div>
</fieldset>
<div class="play-view" id="view"></div>
<fieldset class="play-group">
<legend id="day-title">Your day</legend>
<div id="inspector"></div>
</fieldset>
<fieldset class="play-group">
<legend>Pump</legend>
<div class="demo-controls">
<span class="seg">
<button type="button" id="mode-auto" aria-pressed="true" aria-describedby="mode-hint">Auto mode</button>
<button type="button" id="mode-basal" aria-pressed="false" aria-describedby="mode-hint">Basal only</button>
</span>
<p class="play-hint" id="mode-hint">The pump recalculates the insulin every 15 minutes.</p>
</div>
</fieldset>
<fieldset class="play-group">
<legend>Bolus calculator</legend>
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
</fieldset>
<fieldset class="play-group">
<legend>What if</legend>
<div class="demo-controls">
<label for="res">Insulin resistance</label>
<input type="range" id="res" min="1" max="2" step="0.1" value="1">
<output for="res" id="res-out">× 1.0</output>
</div>
</fieldset>
<div class="play-table" id="totals"></div>
</figure>

## Read it like a report

The curve and the numbers follow the rules from [the numbers chapter](math-and-metrics.html): in range is 70 to 180 mg/dL, the band is drawn behind the curve, and the table sums the whole day. The suggestion on each meal row comes from the same calculator as the dosing chapter: your carb ratio, correction factor and target, plus the day's glucose at that minute for the correction.

Three things keep it honest. This is a virtual patient, not you. The day has no exercise, and the sensor noise stays on, so the curve wobbles like a real one. And the resistance dial is a what-if, not a diagnosis: push it up with Auto mode on and the loop answers with more insulin, which drives the day down rather than up. A real sick day needs your team, not a slider.

<script type="module" src="assets/playground.js"></script>

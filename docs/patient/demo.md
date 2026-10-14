---
title: "Try it: one day, two settings"
lang: en
alt: de/demo.html
sources:
  - tir-tuner-sim/src/lib.rs
  - tir-tuner-web/src/lib.rs
---

# Try it: one day, two settings

This is a day from the same virtual patient as the other chapters. Move the slider to change the carb ratio the bolus calculator uses at breakfast, and watch what the day would look like.

<figure class="panel demo" id="demo">
<figcaption><strong>One virtual day, two carb ratios</strong><span>The grey curve keeps the pump's setting. The black curve is the same day with the breakfast ratio from the slider. Everything else stays as it is.</span></figcaption>
<div class="demo-controls">
<label for="cr">Breakfast carb ratio</label>
<input type="range" id="cr" min="5" max="20" step="0.5" value="11">
<output for="cr" id="cr-out">11 g/U</output>
</div>
</figure>

## The numbers underneath

The curves and the table come from the same simulator as the report panels in the other chapters. Only one thing changes: the carb ratio the bolus calculator applies at breakfast (06:00, 60 g). Lunch, dinner and the late meal keep their ratio, and the pump's Auto mode reacts to both days the same way.

A smaller ratio delivers more insulin for the same carbs. Push it far enough and the day trades highs for lows, which is why the goal is never "as much insulin as possible", but the ratio your team found with you.

<script type="module" src="assets/demo.js"></script>

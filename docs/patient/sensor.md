---
title: The sensor
lang: en
alt: de/sensor.html
sources:
  - tir-tuner-cgm/src/device.rs
  - tir-tuner-body/src/subject.rs
---

# The sensor

A CGM sensor gives a new sugar reading every few minutes, day and night. It is the only view the pump has of your body, so it helps to know what it actually measures.

<figure class="panel" data-fig="sensor" data-lines="Blood|Fluid under the skin|Sensor readings">
<figcaption><strong>Blood, fluid and sensor after a meal</strong><span>A 60 g meal with its bolus. The fluid under the skin follows the blood a little later, and each reading every 15 minutes adds its own small error.</span></figcaption>
</figure>

## It doesn't measure blood

The sensor's thin filament sits just under the skin, in the fluid between the cells. It measures the sugar in that fluid, not in the blood. Sugar passes from the blood into this fluid, so the two are close, but the fluid follows the blood with a delay of roughly ten minutes.

When the sugar is steady, sensor and finger prick agree well. When it moves fast, after a meal or during a low, they differ: the finger prick shows where the blood is now, the sensor shows where it was a few minutes ago. During a fast drop the sensor can still look fine while the blood is already low.

## Every reading is a little off

Each reading carries a small error. Measured on an earlier Abbott sensor, a typical reading is off by about 11 mg/dL (0.6 mmol/L), and most are within about 20 mg/dL of the true value.

The error is sticky. If the sensor reads too high now, it will probably still read too high in 15 minutes: about 70% of the error is still there a quarter of an hour later, and about half after half an hour. And it is lopsided: readings that are far too high are more common than readings that are far too low.

That stickiness is why one strange reading is not worth acting on, and why an automatic system weighs each new value against its own prediction instead of trusting it blindly.

## When to check with a finger prick

When the reading doesn't match how you feel or what you expect, check with a meter. The finger prick measures the blood itself.

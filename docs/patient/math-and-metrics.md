---
title: The numbers on your report
lang: en
alt: de/math-and-metrics.html
sources:
  - tir-tuner-common/src/units.rs
  - tir-tuner-common/src/metrics.rs
---

# The numbers on your report

A sensor collects hundreds of readings a day. Your clinic sums them up in a few numbers, and the same numbers are used worldwide, so a report from one clinic means the same in another.

<figure class="panel" data-fig="day" data-lines="Typical day">
<figcaption><strong>The virtual patient against the goals</strong><span>The two weeks from the first page. It meets every goal: 89% in range, 2% below 70 with less than 1% below 54, and 8% above 180 with none above 250.</span></figcaption>
</figure>

## Two units for the same thing

Sugar is measured in mg/dL or mmol/L, depending on the country and the device. They are the same thing on a different scale: 1 mmol/L is 18 mg/dL.

| mmol/L | mg/dL | Meaning |
|---|---|---|
| 3.0 | 54 | very low |
| 3.9 | 70 | lower edge of the target range |
| 10.0 | 180 | upper edge of the target range |
| 13.9 | 250 | very high |

## Time in range

The headline number. It is the share of readings between 70 and 180 mg/dL (3.9 to 10.0 mmol/L). 70 and 180 themselves count as in range.

The international goal for most adults with type 1 diabetes:

| Range | Goal | Per day |
|---|---|---|
| In range, 70 to 180 | more than 70% | more than 16 h 48 min |
| Below 70 | less than 4% | less than 1 h |
| Below 54 | less than 1% | less than 15 min |
| Above 180 | less than 25% | less than 6 h |
| Above 250 | less than 5% | less than 1 h 12 min |

Time below range has the strictest goal, because lows are dangerous right away and highs do their harm over years. Raising time in range is only progress if time below range doesn't climb with it.

## Mean glucose and variation

The **mean** is the average of all readings. It says where the day sits overall.

The **coefficient of variation** (CV) says how much the readings swing around that mean, as a percentage of it. Two days with the same mean can feel completely different: one flat, one going up and down all day. The CV tells them apart. The goal is 36% or less.

## Reading your own report

A single day says little. One forgotten bolus or one wrong carb count can ruin it. Reports usually cover the last 14 days, and patterns over that time are what count: the same high after the same meal, the same low in the same part of the night. Those patterns point at a specific time and a specific setting, as described in [carb ratio, correction factor, target](dosing.md).

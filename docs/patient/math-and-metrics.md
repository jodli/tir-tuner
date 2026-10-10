---
title: The numbers that matter
lang: en
alt: de/math-and-metrics.html
sources:
  - tir-tuner-common/src/units.rs
  - tir-tuner-common/src/metrics.rs
  - tir-tuner-common/src/euler.rs
  - tir-tuner-common/src/random.rs
---

# The numbers that matter

This page covers the units, the report-card metrics, and the simulation clock, the parts every page in this folder relies on.

## Two languages for the same number

Glucose is measured in two units, and you will have seen both. The model works in millimoles per liter (mmol/L), the unit in most of Europe. The US and many meters use milligrams per deciliter (mg/dL). The two are the same measure scaled by the factor `18.0156`, glucose's molar mass of 180.156 g/mol divided by 10.

1 mmol/L = 18.0156 mg/dL. 5.8 mmol/L is 104.5 mg/dL.
1 mg/dL = 1 / 18.0156 mmol/L, roughly 0.0555.

So the numbers look different but say the same thing. The ones that recur:

| mmol/L | mg/dL | What it is |
|---|---|---|
| 3.9 | 70 | lower edge of the in-range band |
| 5.8 | 104.5 | the target glucose of the controller |
| 10.0 | 180 | upper edge of the in-range band |

## From food to blood sugar

The model counts carbohydrate in grams and glucose in millimoles. One gram of carbohydrate is treated as one gram of glucose, and a gram of glucose is 1 / 180.156 of a mole, so:

1 g carbohydrate = 5.551 mmol glucose.

A 50 g meal becomes 277.6 mmol of glucose in the model. That conversion links the meal on the plate to the sugar in the blood.

## The report card

A day of glucose readings is judged with a few standard numbers.

- **Time in range (TIR)**: the share of readings inside the band 3.9 to 10.0 mmol/L (70 to 180 mg/dL), as a percentage. This is the headline number of a diabetes day. The band comes from the international consensus on time in range (Battelino 2019). Its edges are whole mg/dL, so 70 and 180 count as in range, and the model rounds simulated readings to whole mg/dL before counting.
- **Time below range (TBR)**: the share of readings below 70 mg/dL. The consensus goal is under 4%.
- **Mean glucose**: the average of the readings, in mg/dL.
- **Coefficient of variation (CV)**: the spread of the readings around the mean, as a percentage. It measures how much the day swings around that average, independent of how high. Two days with the same average can feel very different, and CV is what tells them apart.

These are the numbers behind your pump report and your clinic reviews. The rest of this project exists to move the TIR number up without letting time below range climb.

## The clock

A simulation moves forward in small, fixed time steps rather than continuously. The core update is a clamped forward Euler step: take the current value, add the rate of change times the interval length, and never let the result go below zero.

The clamp at zero is a safety rule. No amount may ever go negative, insulin or glucose. The rule is enforced at every step, and it is one of the invariants that the formal verification proves. The body integrates in steps of 0.25 minutes; the controller thinks in steps of 15 minutes. Both use the same clamped primitive.

## The seed

The whole simulation is deterministic. Every random element, sensor noise and pump error alike, comes from a generator seeded with a fixed number. Run the same seed twice and you get the same virtual day, byte for byte.

That is why the safety proofs work. A property that holds on one simulated day can be checked across thousands, all reproducible, all checkable. The generator is a small standard one that draws from nothing but the seed.

## Same numbers as your meter

Every number on this page is chosen so the model and your meter read in the same language. Same units as your meter, same band as your report, same risk scores as your clinic. The project exists so the simulated day and your real day can be compared directly.

From here: [the sensor](sensor.md) reads the body's sugar, and [the body](body.md) is where the disease itself lives.
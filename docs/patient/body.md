---
title: The body
lang: en
alt: de/body.html
sources:
  - tir-tuner-body/src/derivative.rs
  - tir-tuner-body/src/state.rs
  - tir-tuner-body/src/subject.rs
  - tir-tuner-body/src/solver.rs
---

# The body

This page explains type 1 diabetes in the same language the model uses, and then shows how the model describes a person. It is the longest page, because this is where the disease lives.

## The disease in brief

Your body makes sugar and uses it, and both jobs need insulin: insulin to let the cells use the sugar, and insulin to stop the liver's own production once there is enough. Type 1 diabetes removes the cells that make insulin. The sugar from food rises, the sugar your liver makes rises, and nothing pulls either back down. Every treatment is an attempt to replace the missing signal, and the pump is the replacement in this project.

## The model's map of a person

The model splits the body into eleven compartments, each a reservoir that fills and drains at its own rate. They form the map that everything else in this project reads:

```mermaid
flowchart TD
    A[Insulin under the skin] --> B["Depot 1 (s1)"]
    B --> C["Depot 2 (s2)"]
    C --> D["Plasma insulin (i)"]
    D --> E["Action: pushes sugar into cells (x1)"]
    D --> F["Action: enables disposal in tissues (x2)"]
    D --> G["Action: suppresses liver sugar output (x3)"]
    M[Meal] --> H["Gut store 1 (g1)"]
    H --> I["Gut store 2 (g2)"]
    I --> J["Blood sugar (q1)"]
    K["Liver produces sugar (EGP)"] --> J
    E --> J
    F --> L["Tissue sugar (q2)"]
    J --> N["Interstitial fluid (c), what the sensor sees"]
    G -. suppresses .-> K
```

Their short names are `s1`, `s2` (insulin under the skin), `i` (insulin in the blood), `x1`, `x2`, `x3` (what insulin does once it reaches the blood), `q1`, `q2` (sugar in blood and tissue), `g1`, `g2` (food on its way through the gut) and `c` (sugar in the fluid the sensor reads).

## Insulin takes time to arrive

When a pump delivers insulin, it does not land in the blood. It sits under the skin, passes through two depot compartments, and only then reaches the blood. For Fiasp the level in the blood peaks about 57 minutes after a bolus (Heise 2017). That concentration is what acts on the body, and it builds up and fades over that timescale. In this model, no treatment can make insulin act faster.

This delay explains most of diabetes management. A bolus at the start of a meal is a bet about a peak that arrives almost an hour later. The pump's main job is to manage this delay.

## Insulin has three jobs

Once in the blood, insulin acts through three separate effects, and the model keeps them as `x1`, `x2`, `x3`:

1. **It pushes sugar into cells.** The sugar in the blood moves into the tissues faster when insulin is present. This is the transport effect, `x1`.
2. **It enables disposal.** Muscle and fat take up sugar and use or store it, and this works at full speed only with insulin. This is the disposal effect, `x2`.
3. **It tells the liver to stand down.** The liver makes sugar on its own, all the time. Insulin suppresses that production. This is the liver effect, `x3`.

Each effect is driven by the insulin concentration, but each moves at its own speed. That is why the model treats them as three separate reservoirs.

## The liver never stops

The liver produces sugar on its own, and that production is not optional. Without any insulin the model's liver makes about 0.0169 mmol per kg per minute (Wilinska 2010, Table 1). Insulin lowers that in a straight line, as in Hovorka 2004, and enough insulin shuts it off completely. Nothing else in the body can replace it. This is why blood sugar rises overnight with type 1 diabetes: with no basal insulin, the liver keeps producing and nothing holds it back.

Basal insulin targets exactly that rise. It is delivered continuously, the background signal that keeps the liver in check, unlike the larger doses given with meals. The model's term for the constant need is the basal insulin requirement (BIR), and it is one of the few numbers that defines a person.

```mermaid
flowchart LR
    L[Liver] -->|EGP: produces sugar, never off| B[Blood sugar]
    B -->|rises| H[High]
    I[Basal insulin] -. suppresses the liver .-> L
```

## Sugar use without insulin

Not all glucose use needs insulin. The brain and red blood cells take it up regardless. The model calls this the insulin-independent uptake (F01), a constant background drain on the blood. Its strength depends on the level, through a shape called a Michaelis-Menten curve: higher glucose means a stronger drain, lower glucose a weaker one. The result is that the model never drains blood glucose to zero on its own: as the level falls, the drain fades. Low blood sugar remains possible, but this drain is not the cause.

## The kidneys are a safety valve

The kidneys filter the blood and hold the sugar back up to a threshold. Above roughly 9 mmol/L the excess starts to spill into the urine. The model implements this as renal excretion: zero below the threshold, and above it a drain proportional to the excess.

That is why very high sugar tends to stop climbing on its own: past the threshold, the excess sugar is lost through urine. It is a wasteful way to lower the level, and the model keeps it.

## Why the model has a resting point

Deliver a constant basal insulin and give no food, and the liver produces a fixed amount while the tissues use a fixed amount. Where those balance is the resting glucose, the level a person without meals and without activity would sit at.

The simulator works the other way round, as Wilinska 2010 does: it takes the glucose a day starts at and computes the basal rate that holds the person there. A replayed day starts at your first CGM reading, at rest. From then on the closed loop decides.

## One person, scaled to you

Wilinska 2010 built 18 virtual people but published only the averages, so the simulator has one: the average person, scaled to a body weight. A replay uses the weight from your settings.

| Part | Value | Source |
|---|---|---|
| Insulin sensitivities, liver, glucose uptake | the published averages | Wilinska 2010, Tables 1-2 |
| Insulin absorption | Fiasp: peak at about 57 min | fitted to Heise 2017 (Fiasp by pump) |
| Carb ratio for simulated meals | about 1.1 U per 10 g | from the person's own basal, Walsh 2011 |
| Bolus speed | 0.11 U per second | YpsoPump manual |

Nobody's day runs the same twice. The model has this built in: twelve of its parameters drift up and down by 5% over three hours, each on its own rhythm, and the same meal can bring 20% more or less sugar than the last time (both Wilinska 2010). The pump delivers within 5% of what it's told.

When the other pages say "the subject", they mean this one virtual person. It is not fitted to you: it has your weight, but the average person's insulin sensitivity.

## The missing signal

The disease is a missing signal. The liver keeps producing, the food enters, and without the signal the sugar has nowhere to go but up. Every piece of this project, the sensor, the brain, the pump, exists to supply that signal with as little delay and as few mistakes as possible. That is the job, and the rest of these pages show how it is done:

- [The sensor](sensor.md) reads the sugar level the body ends up with.
- [The brain](brain.md) decides how much of the missing signal to supply.
- [The loop](loop.md) puts the two together with a pump and runs the whole day.
---
title: How insulin works
lang: en
alt: de/insulin.html
sources:
  - tir-tuner-body/src/subject.rs
  - tir-tuner-body/src/derivative.rs
  - tir-tuner-body/src/pump.rs
---

# How insulin works

Insulin is the signal that lets sugar leave the blood. A pump delivers it under the skin, and from there it has a long way to go. The numbers on this page are for a fast insulin like Fiasp given by a pump.

<figure class="panel" data-fig="insulin" data-lines="Insulin in the blood|Braking the liver|Burning and storing sugar|Moving sugar into tissues">
<figcaption><strong>One bolus, four timelines</strong><span>After a 5 U bolus. Each line is shown as a share of its own peak, so you can compare when each effect comes and goes, not how big it is.</span></figcaption>
</figure>

## From the skin to the blood

Insulin from a pump doesn't go into the blood directly. It forms a small depot under the skin and seeps out of it slowly. The level in the blood peaks about one hour after a bolus. After about three hours, nearly all of it (90%) has left the skin.

In the blood itself insulin doesn't last: half of it is cleared within about five minutes. The level in the blood only stays up because the depot under the skin keeps feeding it. That is why a bolus works for hours even though insulin in the blood is short-lived.

## Three jobs, three speeds

Once in the blood, insulin does three different things, and each one starts and fades at its own speed:

1. **It brakes the liver.** The liver releases less sugar. This effect peaks about 1 hour 40 minutes after a bolus and is mostly gone after about 5 hours.
2. **It lets the tissues burn and store sugar.** Muscle and fat use the sugar they took in. This effect is the fastest: it peaks after about 1 hour 20 minutes and is mostly gone after about 4.5 hours.
3. **It moves sugar from the blood into the tissues.** This one is slow. It peaks about 3 hours after a bolus and half of it is still there after 7.5 hours.

So a bolus doesn't have one "duration". Its effect is strongest in the first two hours, and a slower tail lasts well into the following hours.

## Insulin on board and the action time setting

Your pump has a setting called duration of insulin action, for example 2.5 hours (allowed: 2 to 8 hours). It tells the bolus calculator how long to count a bolus as still working. The insulin still counted is called insulin on board, or active insulin.

The setting is a simplification. It draws a hard line where the body has a slow fade. When the setting is shorter than the body's real tail, the calculator counts old insulin as gone while it still lowers the sugar. A correction a few hours after a meal bolus then adds insulin on top of insulin that is still working, and a low can follow later. In Auto mode, the automatic delivery doesn't use this setting; only the bolus calculator does.

## Why the same dose doesn't work the same twice

How fast insulin leaves the skin and how strongly the body reacts to it both change. Things that change absorption:

- **Where and how deep** the cannula sits.
- **Blood flow** at the site. Warmth and movement increase it.
- **The amount** given at once.
- **The pump itself**: delivery is very precise, but never exact.

Things that change how strongly the body reacts to the same insulin:

- **Activity** makes the body more sensitive to insulin, during and after the activity. Everyday things like gardening, shopping and housework count too.
- **Alcohol, hot weather, illness with vomiting or diarrhoea, and the start of a period** can make lows more likely.
- **Illness, pain, stress, hormones, and medication like steroids** can cause highs.
- **The time of day.** Even without anything special going on, the body's response to insulin drifts by a few percent over a few hours.

A blocked cannula or a failing set looks like resistance to insulin: the sugar keeps rising no matter how much the pump delivers, because the insulin never arrives.

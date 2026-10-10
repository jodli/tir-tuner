---
title: Die Zahlen in Ihrem Bericht
lang: de
alt: ../math-and-metrics.html
sources:
  - tir-tuner-common/src/units.rs
  - tir-tuner-common/src/metrics.rs
---

# Die Zahlen in Ihrem Bericht

Ein Sensor sammelt Hunderte Werte am Tag. Ihre Klinik fasst sie in wenigen Zahlen zusammen, und diese Zahlen werden weltweit gleich verwendet, ein Bericht aus der einen Klinik bedeutet also in der anderen dasselbe.

## Zwei Einheiten für dieselbe Sache

Zucker wird je nach Land und Gerät in mg/dl oder mmol/l gemessen. Das ist dieselbe Sache auf einer anderen Skala: 1 mmol/l sind 18 mg/dl.

| mmol/l | mg/dl | Bedeutung |
|---|---|---|
| 3,0 | 54 | sehr niedrig |
| 3,9 | 70 | untere Grenze des Zielbereichs |
| 10,0 | 180 | obere Grenze des Zielbereichs |
| 13,9 | 250 | sehr hoch |

## Zeit im Zielbereich

Die wichtigste Zahl. Sie ist der Anteil der Werte zwischen 70 und 180 mg/dl (3,9 bis 10,0 mmol/l). 70 und 180 selbst zählen als im Bereich.

Das internationale Ziel für die meisten Erwachsenen mit Typ-1-Diabetes:

| Bereich | Ziel | Pro Tag |
|---|---|---|
| Im Bereich, 70 bis 180 | mehr als 70 % | mehr als 16 h 48 min |
| Unter 70 | weniger als 4 % | weniger als 1 h |
| Unter 54 | weniger als 1 % | weniger als 15 min |
| Über 180 | weniger als 25 % | weniger als 6 h |
| Über 250 | weniger als 5 % | weniger als 1 h 12 min |

Die Zeit unter dem Bereich hat das strengste Ziel, weil Tiefs sofort gefährlich sind und Hochs über Jahre schaden. Mehr Zeit im Bereich ist nur ein Fortschritt, wenn die Zeit darunter nicht mitsteigt.

## Mittelwert und Schwankung

Der **Mittelwert** ist der Durchschnitt aller Werte. Er sagt, wo der Tag insgesamt liegt.

Der **Variationskoeffizient** (VK, englisch CV) sagt, wie stark die Werte um diesen Mittelwert schwanken, als Prozent davon. Zwei Tage mit demselben Mittelwert können sich völlig verschieden anfühlen: einer flach, einer den ganzen Tag auf und ab. Der VK unterscheidet sie. Das Ziel ist 36 % oder weniger.

## Den eigenen Bericht lesen

Ein einzelner Tag sagt wenig. Ein vergessener Bolus oder eine falsch geschätzte Mahlzeit kann ihn ruinieren. Berichte umfassen meist die letzten 14 Tage, und Muster über diese Zeit zählen: dasselbe Hoch nach derselben Mahlzeit, dasselbe Tief zur selben Nachtzeit. Solche Muster zeigen auf eine bestimmte Zeit und eine bestimmte Einstellung, wie auf der Seite [Kohlenhydratverhältnis, Korrekturfaktor, Zielwert](dosing.md) beschrieben.

Zurück zur [Übersicht](index.md).

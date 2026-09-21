---
title: Die Zahlen, auf die es ankommt
lang: de
alt: ../math-and-metrics.html
sources:
  - tir-tuner-common/src/units.rs
  - tir-tuner-common/src/metrics.rs
  - tir-tuner-common/src/euler.rs
  - tir-tuner-common/src/random.rs
---

# Die Zahlen, auf die es ankommt

Diese Seite behandelt die Einheiten, die Kennzahlen und die Uhr der Simulation, also die Bausteine, auf denen jede andere Seite in diesem Ordner aufbaut.

## Zwei Sprachen für dieselbe Zahl

Glukose wird in zwei Einheiten gemessen, und Sie kennen vermutlich beide. Das Modell rechnet in Millimol pro Liter (mmol/L), der üblichen Einheit in weiten Teilen Europas; die USA und viele Messgeräte arbeiten mit Milligramm pro Deziliter (mg/dL). Es ist dasselbe Maß, nur anders skaliert, mit dem Faktor 18,0182.

1 mmol/L = 18,0182 mg/dL, also sind 5,8 mmol/L = 104,5 mg/dL.
1 mg/dL = 1/18,0182 mmol/L, also rund 0,0555 mmol/L.

Die Zahlen sehen also anders aus, sagen aber dasselbe. Die wichtigsten wiederkehrenden Werte:

| mmol/L | mg/dL | Was es ist |
|---|---|---|
| 3,9 | 70 | untere Grenze des Zielbereichs |
| 5,8 | 104,5 | Zielwert der Steuerung |
| 10,0 | 180 | obere Grenze des Zielbereichs |

## Vom Essen zum Blutzucker

Das Modell rechnet Kohlenhydrate in Gramm und Glukose in Millimol. Ein Gramm Kohlenhydrate wird wie ein Gramm Glukose behandelt; ein Gramm Glukose entspricht einem 180,156stel Mol, also:

1 g Kohlenhydrate = 5,551 mmol Glukose.

Eine 50-g-Mahlzeit entspricht im Modell 277,6 mmol Glukose. Diese Umrechnung verbindet das Essen auf dem Teller mit dem Zucker im Blut.

## Die Kennzahlen des Tages

Ein Tag voller Glukosewerte wird anhand einiger standardisierter Zahlen bewertet.

- **Zeit im Zielbereich (TIR):** der Anteil der Werte im Bereich zwischen 3,9 und 10,0 mmol/L (70 bis 180 mg/dL), in Prozent. Es ist die wichtigste Zahl eines Diabetes-Tages. Den Bereich legt die ISO fest; das Modell übernimmt ihn unverändert.
- **Mittlere Glukose:** der Durchschnitt der Werte, in mmol/L.
- **Variationskoeffizient (CV):** die Streuung der Werte um den Mittelwert, in Prozent. Er sagt, wie stark der Tag um diesen Mittelwert schwankt, unabhängig davon, wie hoch er liegt. Zwei Tage mit demselben Durchschnitt können sich völlig verschieden anfühlen; der CV macht den Unterschied aus.
- **LBGI und HBGI:** die Indizes für niedrige und hohe Glukose: Risikowerte nach Kovatchev. Jeder einzelne Wert wird danach gewichtet, welche Gefahr er bedeutet. Ein langer Abschnitt mäßig hoher Werte zählt anders als ein einzelner gefährlich niedriger. Werte über etwa 112,5 mg/dL (6,24 mmol/L) tragen nichts zum Niedrig-Index bei, und umgekehrt.

Das sind die Zahlen hinter Ihrem Pumpbericht und Ihren Klinikbesuchen. Der Rest dieses Projekts existiert, um die TIR nach oben zu bringen, ohne die Risikowerte im niedrigen Bereich steigen zu lassen.

## Die Uhr

Eine Simulation bewegt sich in kleinen, festen Zeitschritten vorwärts statt kontinuierlich. Der zentrale Rechenschritt ist ein geklemmter Vorwärts-Euler-Schritt: den aktuellen Wert nehmen, die Änderungsrate mit der Intervalllänge addieren und das Ergebnis niemals unter Null fallen lassen.

Die Klemme bei Null ist eine Sicherheitsregel: Keine Menge darf negativ werden, weder Insulin noch Glukose. Die Regel gilt in jedem Schritt und zählt zu den Invarianten, die die formale Verifikation beweist. Der Körper rechnet in Schritten von 0,25 Minuten, die Steuerung denkt in 15-Minuten-Schritten; beide nutzen dasselbe geklemmte Grundprimitiv.

## Der Seed

Die Simulation ist vollständig deterministisch: Jedes Zufallselement, Sensorrauschen wie Pumpfehler, stammt aus einem Generator, der mit einer festen Zahl geseedet wird. Wer denselben Seed zweimal laufen lässt, bekommt exakt denselben virtuellen Tag, Byte für Byte.

Deshalb tragen die Sicherheitsbeweise: Eine Eigenschaft, die für einen simulierten Tag gilt, lässt sich über Tausende Tage prüfen, alle reproduzierbar, alle nachrechenbar. Der Generator ist klein und standardisiert und zieht ausschließlich aus dem Seed.

## Dieselben Zahlen wie Ihr Messgerät

Jede Zahl auf dieser Seite ist so gewählt, dass das Modell und Ihr Messgerät dieselbe Sprache sprechen: dieselben Einheiten wie Ihr Messgerät, derselbe Bereich wie Ihr Bericht, dieselben Risikowerte wie Ihre Klinik. Das Projekt existiert, damit sich der simulierte Tag mit Ihrem echten Tag direkt vergleichen lässt.

Weiter: [Der Sensor](sensor.md) misst den Zucker des Körpers, und [Der Körper](body.md) ist der Ort, an dem die Krankheit selbst lebt.
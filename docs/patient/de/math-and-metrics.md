---
title: Die Zahlen, auf die es ankommt
lang: de
sources:
  - tir-tuner-common/src/units.rs
  - tir-tuner-common/src/metrics.rs
  - tir-tuner-common/src/euler.rs
  - tir-tuner-common/src/random.rs
---

# Die Zahlen, auf die es ankommt

Diese Seite behandelt die Einheiten, die Kennzahlen und die Simulationsuhr, also die Bausteine, auf die sich jede Seite in diesem Ordner stützt.

## Zwei Sprachen für dieselbe Zahl

Glukose wird in zwei Einheiten gemessen, und Sie kennen vermutlich beide. Das Modell rechnet in Millimol pro Liter (mmol/L), der Einheit in den meisten Teilen Europas. Die USA und viele Messgeräte nutzen Milligramm pro Deziliter (mg/dL). Die beiden sind dasselbe Maß, nur skaliert mit dem Faktor `18.0182`.

1 mmol/L = 18.0182 mg/dL. 5.8 mmol/L sind 104.5 mg/dL.
1 mg/dL = 1 / 18.0182 mmol/L, also rund 0.0555.

Die Zahlen sehen also anders aus, sagen aber dasselbe. Die, die immer wieder auftauchen:

| mmol/L | mg/dL | Was es ist |
|---|---|---|
| 3.9 | 70 | untere Grenze des Zielbereichs |
| 5.8 | 104.5 | der Zielwert der Steuerung |
| 10.0 | 180 | obere Grenze des Zielbereichs |

## Vom Essen zum Blutzucker

Das Modell zählt Kohlenhydrate in Gramm und Glukose in Millimol. Ein Gramm Kohlenhydrate wird wie ein Gramm Glukose behandelt, und ein Gramm Glukose ist 1 / 180.156 eines Mols, also:

1 g Kohlenhydrate = 5.551 mmol Glukose.

Eine 50-g-Mahlzeit wird im Modell zu 277.6 mmol Glukose. Diese Umrechnung verbindet das Essen auf dem Teller mit dem Zucker im Blut.

## Der Zeugnisbericht

Ein Tag voller Glukosewerte wird mit ein paar standardisierten Zahlen bewertet.

- **Zeit im Zielbereich (TIR)**: der Anteil der Werte innerhalb des Bereichs 3.9 bis 10.0 mmol/L (70 bis 180 mg/dL), als Prozentsatz. Das ist die wichtigste Zahl eines Diabetes-Tages. ISO definiert den Bereich; das Modell meldet ihn auf dieselbe Weise.
- **Mittlere Glukose**: der Durchschnitt der Werte, in mmol/L.
- **Variationskoeffizient (CV)**: die Streuung der Werte um den Mittelwert, als Prozentsatz. Er misst, wie stark der Tag um diesen Mittelwert schwankt, unabhängig davon, wie hoch er liegt. Zwei Tage mit demselben Durchschnitt können sich sehr unterschiedlich anfühlen; der CV ist es, der sie unterscheidet.
- **LBGI und HBGI**: die Indizes für niedrige und hohe Glukose. Es sind die Risiko-Scores nach Kovatchev; sie gewichten jeden Wert danach, welche Gefahr er darstellt. Ein langer Abschnitt mäßig hoher Werte wird anders bewertet als ein einzelner gefährlich niedriger Wert. Werte oberhalb von etwa 112.5 mg/dL (6.24 mmol/L) tragen nichts zum Niedrig-Index bei, und umgekehrt.

Das sind die Zahlen hinter Ihrem Pumpbericht und Ihren Klinikbesuchen. Der Rest dieses Projekts existiert, um die TIR-Zahl nach oben zu bringen, ohne dass die Niedrig-Risiko-Werte klettern.

## Die Uhr

Eine Simulation bewegt sich in kleinen, festen Zeitschritten vorwärts statt kontinuierlich. Die Kernaktualisierung ist ein geklemmter Vorwärts-Euler-Schritt: den aktuellen Wert nehmen, die Änderungsrate mal die Intervalllänge addieren, und das Ergebnis niemals unter Null fallen lassen.

Die Klemme bei Null ist eine Sicherheitsregel. Keine Menge darf je negativ werden, weder Insulin noch Glukose. Die Regel wird in jedem Schritt durchgesetzt, und sie gehört zu den Invarianten, die die formale Verifikation beweist. Der Körper integriert in Schritten von 0.25 Minuten; die Steuerung denkt in Schritten von 15 Minuten. Beide nutzen dasselbe geklemmte Grundprimitiv.

## Der Seed

Die gesamte Simulation ist deterministisch. Jedes Zufallselement, Sensorrauschen und Pumpfehler gleichermaßen, stammt aus einem Generator, der mit einer festen Zahl geseedet wird. Denselben Seed zweimal laufen lassen ergibt denselben virtuellen Tag, Byte für Byte.

Deshalb funktionieren die Sicherheitsbeweise. Eine Eigenschaft, die an einem simulierten Tag gilt, lässt sich über Tausende prüfen, alle reproduzierbar, alle nachprüfbar. Der Generator ist ein kleiner, standardisierter, der ausschließlich aus dem Seed zieht.

## Dieselben Zahlen wie Ihr Messgerät

Jede Zahl auf dieser Seite ist so gewählt, dass das Modell und Ihr Messgerät dieselbe Sprache sprechen. Dieselben Einheiten wie Ihr Messgerät, derselbe Bereich wie Ihr Bericht, dieselben Risiko-Scores wie Ihre Klinik. Das Projekt existiert, damit der simulierte Tag und Ihr echter Tag direkt vergleichbar sind.

Weiter zu: [Der Sensor](sensor.md) misst den Zucker des Körpers, und [Der Körper](body.md) ist der Ort, an dem die Krankheit selbst lebt.
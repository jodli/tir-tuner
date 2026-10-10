---
title: Der Regelkreis
lang: de
alt: ../loop.html
sources:
  - tir-tuner-cli/src/engine.rs
  - tir-tuner-ingest/src/glooko.rs
  - tir-tuner-common/src/metrics.rs
---

# Der Regelkreis

Diese Seite zeigt, wie Sensor, Steuerung, Pumpe und Körper zu einem laufenden Tag verdrahtet sind, und spielt den Tag zusätzlich mit echten Pumpdaten durch.

## Einen Tag laufen lassen

Der Körper wird in Schritten von 0,25 Minuten weitergerechnet. Alle 15 Minuten beginnt eine neue Regelperiode: Der Sensor liest den Körper, das Gehirn legt eine Rate fest, die Pumpe gibt sie ab, und der Körper arbeitet bis zur nächsten Periode mit dieser Folge weiter. Jeder Baustein hat seine Aufgabe:

```mermaid
flowchart LR
    V[(Körper)] -->|Gewebezucker| S[Sensor]
    S -->|Wert| G[Gehirn]
    G -->|Rate| P[(Pumpe)]
    P -->|Insulin| V
    CF[(Echte Pumpdaten)] -.Boli.- G
    V -.Abgaben.- T[(Spur: TIR, Mittelwert, CV)]
```

Mahlzeiten sind es, die aus der Simulation einen Tag machen. Eine Mahlzeit ist eine Menge Kohlenhydrate in Gramm zu einem Zeitpunkt. Sie wandert durch den Magen-Darm-Trakt und verhält sich genau so, wie es [Der Körper](body.md) beschreibt.

## Mahlzeiten und Boli

Eine Mahlzeit ist angekündigt oder nicht:

1. **Angekündigt.** Die Mahlzeit wurde in der App eingegeben, die Steuerung kennt ihre Kohlenhydrate. Ein nachgespielter Tag kündigt jede protokollierte Mahlzeit an und spielt deine echten Boli zu ihrer Minute ab.
2. **Nicht angekündigt.** Die Steuerung weiß nichts davon; sie sieht den Zucker steigen und korrigiert von selbst.

Mit abgeschalteter Steuerung (offener Regelkreis) gibt die Pumpe die Basalrate ab, die den Startzucker hält; die Hypoglykämie-Unterbrechung bleibt aktiv. Bei gleichen Mahlzeiten und gleichem Probanden schneidet der geschlossene Regelkreis besser ab als diese reine Basalvariante.

## Die Pumpe ist ungenau

Auch die Pumpe ist im Modell nicht perfekt, wie bei Wilinska 2010: Was sie abgibt, weicht mit einem Variationskoeffizienten von 5 % von dem ab, was sie soll, bei der Basalrate wie bei Boli. Ein Bolus läuft mit den 0,11 U pro Sekunde der YpsoPump. Jede Pumpe hat diese Unvollkommenheit, und der Regelkreis muss mit ihr leben. Der Sensor rauscht ebenfalls, wie die [Sensorseite](sensor.md) erklärt, und der Körper ist im Modell der Steuerung nur angenähert, wie die [Gehirnseite](brain.md) beschreibt. In einem Lauf wirken alle drei zusammen, so wie an einem echten Tag.

## Das Modell gegen die Landkarte

In einer Simulation laufen zwei Versionen des Körpers, und sie sind nicht identisch:

- Der **Körper** ist in der Simulation die Wahrheit: der vollständige virtuelle Patient, mit seinen eigenen, möglicherweise zufällig gezogenen Parametern.
- Die **Annahme** ist das Bild, das sich die Steuerung von diesem Körper macht: eine vereinfachte Skizze, absichtlich nicht identisch mit dem Körper, und so kalibriert, dass ihre Ruheglukose auf demselben Ziel liegt wie die des Körpers.

Die Steuerung handelt nach der Annahme, auch wenn diese deutlich danebenliegt, und der Regelkreis muss trotzdem funktionieren. Diese Trennung ist dieselbe Konstruktionsentscheidung wie im echten System; sie ist der Grund, warum die Simulation eine Prüfung der Robustheit ist und kein reiner Selbsttest. Bei jedem echten Wert wird die Annahme korrigiert, gewichtet danach, wie sehr Sensor und Modell zu trauen ist, sodass sich das Bild im Lauf des Tages selbst korrigiert.

## Ihr eigener Tag, wiederholt

Der Glooko-Import ist der persönlichste Teil dieses Projekts. Echte Pumpensoftware exportiert den Tag als CSV-Dateien, mit Komma als Dezimaltrenner, Zeitstempeln wie `05.08.2026 23:59` und Zeilen in umgekehrt chronologischer Reihenfolge. Der Parser rechnet mit dem realen Format, nicht mit einem idealisierten.

Importiert werden zwei Dateien:

- **CGM-Export:** die Sensorwerte eines echten Tages, in mg/dL.
- **Bolus-Export:** die Mahlzeiten, zu denen Sie Kohlenhydrate protokolliert haben und die Pumpe die passenden Boli abgab.

Der Bolus-Export speist den Mahlzeitenplan: dasselbe Essen, dieselben Zeiten, abgespielt gegen das Modell, während die Steuerung das Insulin dosiert. Am Ende druckt der Lauf für beide Tage dieselbe Kennzahlenzeile, Zeit im Zielbereich, mittlere Glukose und Variationskoeffizient, sodass sich echter und simulierter Tag direkt vergleichen lassen:

```
real Glooko: <N> samples, TIR <...>%, mean <...> mmol/L, CV <...>%
sim (closed loop): <M> samples, TIR <...>%, mean <...> mmol/L, CV <...>%
```

Ihre Werte und Ihre Mahlzeiten laufen durch den verifizierten Regelkreis; die gedruckten Zahlen sind das Ergebnis dieses Laufs.

## Der Fünfzehn-Minuten-Neustart

Alle fünfzehn Minuten verwirft der Regelkreis seinen eigenen Plan und stellt mit dem neuesten Wert die ganze Frage neu. Dieser Neustart ist Absicht: Die Pumpe muss nur über die nächsten fünfzehn Minuten richtig liegen; ob sie später falsch liegt, bleibt offen. Ein Tag ist die Summe dieser Korrekturen, über die ganze Nacht.

Weiter: [Die Zahlen, auf die es ankommt](math-and-metrics.md) erklären die Kennzahlen, an denen der Tag gemessen wird, und die [Übersicht](index.md) fügt das ganze System wieder zusammen. Die formale Verifikation der Sicherheitsregeln steht im [vollständigen Bericht](../verification_report.html).
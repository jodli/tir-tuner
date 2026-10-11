---
title: Der Sensor
lang: de
alt: ../sensor.html
sources:
  - tir-tuner-cgm/src/device.rs
  - tir-tuner-body/src/subject.rs
---

# Der Sensor

Ein CGM-Sensor liefert alle paar Minuten einen neuen Zuckerwert, Tag und Nacht. Er ist der einzige Blick, den die Pumpe auf Ihren Körper hat, deshalb lohnt es sich zu wissen, was er eigentlich misst.

<figure class="panel" data-fig="sensor" data-lines="Blut|Flüssigkeit unter der Haut|Sensorwerte">
<figcaption><strong>Blut, Gewebeflüssigkeit und Sensor nach einer Mahlzeit</strong><span>Eine Mahlzeit mit 60 g und ihrem Bolus. Die Flüssigkeit unter der Haut folgt dem Blut etwas später, und jeder Wert alle 15 Minuten bringt seinen eigenen kleinen Fehler mit.</span></figcaption>
</figure>

## Er misst nicht im Blut

Der dünne Faden des Sensors sitzt direkt unter der Haut, in der Flüssigkeit zwischen den Zellen. Er misst den Zucker in dieser Flüssigkeit, nicht im Blut. Zucker wandert aus dem Blut in diese Flüssigkeit, die beiden liegen also nah beieinander, aber die Flüssigkeit folgt dem Blut mit einer Verzögerung von grob zehn Minuten.

Wenn der Zucker stabil ist, passen Sensor und Fingerpiks gut zusammen. Wenn er sich schnell bewegt, nach dem Essen oder während eines Tiefs, weichen sie ab: Der Fingerpiks zeigt, wo das Blut jetzt ist, der Sensor, wo es vor ein paar Minuten war. Bei einem schnellen Abfall kann der Sensor noch gut aussehen, während das Blut schon niedrig ist.

## Jeder Wert liegt ein bisschen daneben

Jeder Wert hat einen kleinen Fehler. Gemessen an einem früheren Abbott-Sensor liegt ein typischer Wert um etwa 11 mg/dl (0,6 mmol/l) daneben, und die meisten liegen innerhalb von etwa 20 mg/dl um den wahren Wert.

Der Fehler ist hartnäckig. Misst der Sensor jetzt zu hoch, misst er wahrscheinlich auch in 15 Minuten noch zu hoch: Etwa 70 % des Fehlers sind eine Viertelstunde später noch da, und etwa die Hälfte nach einer halben Stunde. Und er ist schief: Werte, die weit zu hoch liegen, sind häufiger als Werte, die weit zu niedrig liegen.

Diese Hartnäckigkeit ist der Grund, warum ein einzelner seltsamer Wert keine Reaktion wert ist, und warum ein automatisches System jeden neuen Wert gegen seine eigene Vorhersage abwägt, statt ihm blind zu glauben.

## Wann ein Fingerpiks sinnvoll ist

Wenn der Wert nicht dazu passt, wie Sie sich fühlen oder was Sie erwarten, messen Sie mit einem Messgerät nach. Der Fingerpiks misst das Blut selbst.

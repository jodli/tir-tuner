---
title: "Die Spielwiese: ein Tag in Ihrer Hand"
lang: de
alt: ../play.html
sources:
  - tir-tuner-web/src/lib.rs
  - tir-tuner-bolus/src/lib.rs
---

# Die Spielwiese: ein Tag in Ihrer Hand

Die anderen Kapitel zeigen Tage, die der virtuelle Patient erlebt hat. Dieser ist Ihrer: Fügen Sie Mahlzeiten hinzu, entscheiden Sie über Bolusdosen, schalten Sie den Auto-Modus der Pumpe ein oder aus, und die Simulation antwortet mit dem Tag, der daraus folgt.

<figure class="panel playground" id="play">
<figcaption><strong>Die Spielwiese</strong><span>Die schwarze Kurve ist Ihr Tag, wie er jetzt ist. Die graue Kurve ist der Tag vor Ihrer letzten Änderung, so zeigt jeder Schritt, was er bewirkt.</span></figcaption>
<fieldset class="play-group">
<legend>Mit einem fertigen Tag beginnen</legend>
<div class="play-actions">
<button type="button" class="chip" data-preset="forgot" aria-pressed="false">Frühstücksbolus vergessen</button>
<button type="button" class="chip" data-preset="untreated" aria-pressed="false">Unbehandelt</button>
<button type="button" class="chip" data-preset="stacked" aria-pressed="false">Doppelte Korrektur</button>
</div>
</fieldset>
<div class="play-view" id="view"></div>
<fieldset class="play-group">
<legend id="day-title">Ihr Tag</legend>
<div id="inspector"></div>
</fieldset>
<fieldset class="play-group">
<legend>Pumpe</legend>
<div class="demo-controls">
<span class="seg">
<button type="button" id="mode-auto" aria-pressed="true" aria-describedby="mode-hint">Auto-Modus</button>
<button type="button" id="mode-basal" aria-pressed="false" aria-describedby="mode-hint">Nur Basalinsulin</button>
</span>
<p class="play-hint" id="mode-hint">Die Pumpe berechnet das Insulin alle 15 Minuten neu.</p>
</div>
</fieldset>
<fieldset class="play-group">
<legend>Bolusrechner</legend>
<div class="demo-controls">
<label for="cr">Kohlenhydratverhältnis</label>
<input type="range" id="cr" min="5" max="20" step="0.5" value="11">
<output for="cr" id="cr-out">11 g/IE</output>
</div>
<div class="demo-controls">
<label for="isf">Korrekturfaktor</label>
<input type="range" id="isf" min="10" max="200" step="5" value="50">
<output for="isf" id="isf-out">50 mg/dl</output>
</div>
<div class="demo-controls">
<label for="tgt">Zielwert</label>
<input type="range" id="tgt" min="80" max="190" step="1" value="100">
<output for="tgt" id="tgt-out">100 mg/dl</output>
</div>
</fieldset>
<fieldset class="play-group">
<legend>Was wäre wenn</legend>
<div class="demo-controls">
<label for="res">Insulinresistenz</label>
<input type="range" id="res" min="1" max="2" step="0.1" value="1">
<output for="res" id="res-out">× 1,0</output>
</div>
</fieldset>
<div class="play-table" id="totals"></div>
</figure>

## Lesen Sie es wie einen Bericht

Die Kurve und die Zahlen folgen den Regeln aus dem [Kapitel zu den Zahlen](math-and-metrics.html): Im Bereich heißt 70 bis 180 mg/dl, das Band liegt hinter der Kurve, und die Tabelle fasst den ganzen Tag zusammen. Der Vorschlag an jeder Mahlzeit stammt aus demselben Rechner wie im Dosierkapitel: Ihr Kohlenhydratverhältnis, Ihr Korrekturfaktor und Ihr Zielwert, dazu der Blutzucker des Tages zu dieser Minute für die Korrektur.

Drei Dinge halten es ehrlich. Das ist ein virtueller Patient, nicht Sie. Der Tag hat keine Bewegung, und das Sensorrauschen bleibt an, deshalb wackelt die Kurve wie eine echte. Und der Resistenzregler ist ein Was-wäre-wenn, keine Diagnose: Schieben Sie ihn bei eingeschaltetem Auto-Modus nach oben, antwortet die Regelung mit mehr Insulin, was den Tag nach unten statt nach oben bringt. Ein echter Krankheitstag braucht Ihr Team, nicht einen Regler.

<script type="module" src="../assets/playground.js"></script>

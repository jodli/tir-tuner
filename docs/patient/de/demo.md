---
title: "Ausprobieren: ein Tag, zwei Einstellungen"
lang: de
alt: ../demo.html
sources:
  - tir-tuner-sim/src/lib.rs
  - tir-tuner-web/src/lib.rs
---

# Ausprobieren: ein Tag, zwei Einstellungen

Dies ist ein Tag desselben virtuellen Patienten wie in den anderen Kapiteln. Bewegen Sie den Regler, um das Kohlenhydratverhältnis zu ändern, mit dem der Bolusrechner das Frühstück abdeckt, und sehen Sie, wie der Tag aussehen würde.

<figure class="panel demo" id="demo">
<figcaption><strong>Ein virtueller Tag, zwei Kohlenhydratverhältnisse</strong><span>Die graue Kurve hält die Einstellung der Pumpe ein. Die schwarze Kurve ist derselbe Tag mit dem Frühstücksverhältnis aus dem Regler. Alles andere bleibt, wie es ist.</span></figcaption>
<div class="demo-controls">
<label for="cr">Frühstücks-Kohlenhydratverhältnis</label>
<input type="range" id="cr" min="5" max="20" step="0.5" value="11">
<output for="cr" id="cr-out">11 g/IE</output>
</div>
</figure>

## Die Zahlen darunter

Die Kurven und die Tabelle stammen aus derselben Simulation wie die Berichtstafeln in den anderen Kapiteln. Geändert wird nur eine Sache: das Kohlenhydratverhältnis, mit dem der Bolusrechner das Frühstück (6 Uhr, 60 g) abdeckt. Mittagessen, Abendessen und Spätmahlkeit behalten ihr Verhältnis, und der Auto-Modus der Pumpe reagiert auf beide Tage gleich.

Ein kleineres Verhältnis gibt für dieselben Kohlenhydrate mehr Insulin. Treiben Sie es weit genug, tauscht der Tag Hohe gegen Tiefe. Deshalb ist das Ziel nie „möglichst viel Insulin“, sondern das Verhältnis, das Ihr Team mit Ihnen gefunden hat.

<script type="module" src="../assets/demo.js"></script>

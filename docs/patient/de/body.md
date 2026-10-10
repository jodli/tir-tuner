---
title: Der Körper
lang: de
alt: ../body.html
sources:
  - tir-tuner-body/src/derivative.rs
  - tir-tuner-body/src/state.rs
  - tir-tuner-body/src/subject.rs
  - tir-tuner-body/src/solver.rs
---

# Der Körper

Diese Seite erklärt Typ-1-Diabetes in derselben Sprache, die das Modell verwendet, und zeigt dann, wie das Modell einen Menschen beschreibt. Es ist die längste Seite, denn hier steht die Krankheit im Mittelpunkt.

## Die Krankheit in Kürze

Der Körper bildet Zucker und verbraucht ihn, und beides braucht Insulin: Insulin, damit die Zellen den Zucker nutzen können, und Insulin, damit die Leber aufhört, selbst Zucker zu bilden, sobald genug da ist. Bei Typ-1-Diabetes werden die Zellen zerstört, die Insulin herstellen. So steigt der Zucker aus dem Essen, der Zucker aus der Leber steigt ebenfalls, und nichts holt ihn zurück. Jede Behandlung versucht, das fehlende Signal zu ersetzen; die Pumpe ist in diesem Projekt dieser Ersatz.

## Die Landkarte, die das Modell von einem Menschen hat

Das Modell teilt den Körper in elf Kompartimente, jedes ein Reservoir, das sich in eigenem Tempo füllt und leert. Zusammen bilden sie die Landkarte, an der sich alles andere in diesem Projekt orientiert:

```mermaid
flowchart TD
    A[Insulin unter der Haut] --> B["Depot 1 (s1)"]
    B --> C["Depot 2 (s2)"]
    C --> D["Insulin im Blut (i)"]
    D --> E["Wirkung: zieht Zucker in die Zellen (x1)"]
    D --> F["Wirkung: ermöglicht Aufnahme im Gewebe (x2)"]
    D --> G["Wirkung: drosselt die Zuckerproduktion der Leber (x3)"]
    M[Mahlzeit] --> H["Magenspeicher 1 (g1)"]
    H --> I["Magenspeicher 2 (g2)"]
    I --> J["Blutzucker (q1)"]
    K["Leber produziert Zucker (EGP)"] --> J
    E --> J
    F --> L["Zucker im Gewebe (q2)"]
    J --> N["Gewebeflüssigkeit (c), was der Sensor sieht"]
    G -. drosselt .-> K
```

Die Kurznamen der Kompartimente: `s1`, `s2` (Insulin unter der Haut), `i` (Insulin im Blut), `x1`, `x2`, `x3` (was Insulin tut, sobald es im Blut ist), `q1`, `q2` (Zucker in Blut und Gewebe), `g1`, `g2` (Essen auf dem Weg durch den Magen-Darm-Trakt) und `c` (Zucker in der Flüssigkeit, die der Sensor liest).

## Insulin braucht Zeit

Wenn die Pumpe Insulin abgibt, landet es nicht sofort im Blut. Es sammelt sich unter der Haut, wandert durch zwei Depot-Kompartimente und kommt erst dann im Blut an. Bei Fiasp erreicht der Spiegel im Blut etwa 57 Minuten nach einem Bolus seinen Höhepunkt (Heise 2017). Über dieselbe Zeitspanne baut sie sich auf und wieder ab. In diesem Modell kann keine Behandlung das Insulin schneller wirken lassen.

Diese Verzögerung erklärt den Großteil des Diabetesmanagements. Ein Bolus zur Mahlzeit ist eine Wette auf eine Wirkungsspitze, die erst fast eine Stunde später eintrifft. Die Hauptaufgabe der Pumpe ist es, mit dieser Verzögerung umzugehen.

## Insulin hat drei Aufgaben

Sobald das Insulin im Blut ist, wirkt es über drei getrennte Effekte; das Modell führt sie als `x1`, `x2`, `x3`:

1. **Es zieht Zucker in die Zellen.** Der Zucker im Blut wandert schneller ins Gewebe, wenn Insulin da ist. Das ist der Transporteffekt, `x1`.
2. **Es ermöglicht die Aufnahme.** Muskeln und Fett nehmen Zucker auf und verbrauchen oder speichern ihn; nur mit Insulin läuft das vollständig. Das ist der Aufnahmeeffekt, `x2`.
3. **Es drosselt die Leber.** Die Leber bildet laufend Zucker. Insulin bremst diese Produktion. Das ist die Leberhemmung, `x3`.

Jeder Effekt wird von der Insulinkonzentration angetrieben, aber jeder bewegt sich in eigenem Tempo. Deshalb behandelt das Modell sie als drei getrennte Reservoire.

## Die Leber hört nie auf

Die Leber bildet von sich aus Zucker; das lässt sich nicht abschalten. Ganz ohne Insulin bildet die Leber im Modell etwa 0,0169 mmol pro Kilogramm pro Minute (Wilinska 2010, Tabelle 1). Insulin senkt das linear, wie bei Hovorka 2004, und genug Insulin schaltet sie ganz ab. Nichts anderes im Körper kann sie ersetzen. Deshalb steigt der Blutzucker bei Typ-1-Diabetes über Nacht: Ohne Basalinsulin arbeitet die Leber weiter, und nichts hält sie zurück.

Basalinsulin wirkt genau gegen diesen Anstieg. Es läuft kontinuierlich, ein Hintergrundsignal, das die Leber in Schach hält, anders als die größeren Dosen zu den Mahlzeiten. Das Modell nennt diesen konstanten Bedarf Basalinsulinbedarf (BIR); er gehört zu den wenigen Zahlen, die einen Menschen beschreiben.

```mermaid
flowchart LR
    L[Leber] -->|EGP: produziert Zucker, nie aus| B[Blutzucker]
    B -->|steigt| H[Hoch]
    I[Basalinsulin] -. drosselt die Leber .-> L
```

## Zuckerabbau ohne Insulin

Nicht jede Glukoseaufnahme braucht Insulin. Gehirn und rote Blutkörperchen nehmen Zucker ohne Insulin auf. Das Modell nennt das den insulinunabhängigen Abbau (F01): eine Senke, die im Blut ständig arbeitet. Wie stark sie ist, regelt eine Michaelis-Menten-Kurve: Je höher die Glukose, desto stärker die Senke, je niedriger, desto schwächer. Dadurch fällt der Blutzucker im Modell von selbst nie auf Null: Sinkt das Niveau, verblasst die Senke. Unterzucker bleibt möglich, nur ist diese Senke nicht die Ursache.

## Die Nieren als Sicherheitsventil

Die Nieren filtern das Blut und behalten den Zucker bis zu einer Schwelle zurück. Ab etwa 9 mmol/L läuft der Überschuss in den Urin über. Das Modell bildet das als renale Ausscheidung ab: unterhalb der Schwelle Null, darüber eine Senke proportional zum Überschuss.

Deshalb steigt sehr hoher Zucker von selbst nicht unbegrenzt weiter: Oberhalb der Schwelle geht der Überschuss über den Urin verloren. Das ist ein verschwenderischer Weg, das Niveau zu senken, aber das Modell bildet genau diesen Mechanismus ab.

## Warum das Modell einen Ruhepunkt hat

Bei konstantem Basalinsulin und ohne Mahlzeiten erzeugt die Leber eine feste Menge, während das Gewebe eine feste Menge verbraucht. Der Punkt, an dem sich beides ausgleicht, ist die Ruheglukose: das Niveau, auf dem ein Mensch ohne Essen und ohne Aktivität verharrt.

Der Simulator rechnet andersherum, wie Wilinska 2010: Er nimmt den Zucker, mit dem ein Tag beginnt, und berechnet die Basalrate, die den Menschen dort hält. Ein nachgespielter Tag beginnt beim ersten CGM-Wert, in Ruhe. Danach entscheidet der geschlossene Regelkreis.

## Ein Mensch, auf dich skaliert

Wilinska 2010 hat 18 virtuelle Menschen gebaut, aber nur die Mittelwerte veröffentlicht. Der Simulator hat deshalb einen: den Durchschnittsmenschen, auf ein Körpergewicht skaliert. Beim Nachspielen gilt das Gewicht aus deinen Einstellungen.

| Teil | Wert | Quelle |
|---|---|---|
| Insulinempfindlichkeit, Leber, Zuckeraufnahme | die veröffentlichten Mittelwerte | Wilinska 2010, Tabellen 1-2 |
| Insulinaufnahme | Fiasp: Spitze nach etwa 57 min | angepasst an Heise 2017 (Fiasp per Pumpe) |
| KE-Faktor für simulierte Mahlzeiten | etwa 1,1 U pro 10 g | aus der eigenen Basalrate, Walsh 2011 |
| Bolusgeschwindigkeit | 0,11 U pro Sekunde | YpsoPump-Gebrauchsanweisung |

Kein Tag läuft zweimal gleich. Das Modell hat das eingebaut: Zwölf seiner Parameter schwanken über drei Stunden um 5 %, jeder in seinem eigenen Takt, und dieselbe Mahlzeit kann 20 % mehr oder weniger Zucker bringen als beim letzten Mal (beides Wilinska 2010). Die Pumpe gibt bis auf 5 % genau ab, was sie soll.

Wenn die anderen Seiten von "dem Probanden" sprechen, ist dieser eine virtuelle Mensch gemeint. Er ist nicht an dich angepasst: Er hat dein Gewicht, aber die Insulinempfindlichkeit des Durchschnitts.

## Das fehlende Signal

Die Krankheit ist ein fehlendes Signal: Die Leber arbeitet weiter, das Essen kommt herein, und ohne das Signal kann der Zucker nur steigen. Jeder Baustein dieses Projekts, Sensor, Gehirn, Pumpe, dient dem einen Ziel, dieses Signal so verzögerungsarm und so fehlerfrei wie möglich zu liefern. Die folgenden Seiten zeigen, wie das gelingt:

- [Der Sensor](sensor.md) misst, auf welchem Zuckerniveau der Körper am Ende landet.
- [Das Gehirn](brain.md) entscheidet, wie viel von dem fehlenden Signal geliefert wird.
- [Der Regelkreis](loop.md) verbindet Sensor und Gehirn mit einer Pumpe und lässt alles einen Tag lang laufen.
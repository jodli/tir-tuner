"""Centralized German user-facing labels.

Only user-facing output is German; code identifiers, JSON keys and contract
field names stay English. Keep all UI wording here so it is consistent and easy
to adjust.
"""
from __future__ import annotations

L = {
    "title": "tir-tuner: Typ-1-Diabetes Auswertung",
    "window": "Zeitraum",
    "weeks": "Wochen",
    "readings": "Messwerte",
    "overall": "Gesamtwerte",
    "tir": "Zeit im Zielbereich (70-180)",
    "tbr70": "Zeit unter 70",
    "tbr54": "Zeit unter 54",
    "tar180": "Zeit über 180",
    "tar250": "Zeit über 250",
    "mean": "Mittelwert",
    "gmi": "GMI",
    "cv": "Variationskoeffizient (CV)",
    "per_block": "Nach Tagesblock",
    "col_block": "Block",
    "col_tir": "TIR",
    "col_eff_cr": "eff. CR",
    "col_conf_cr": "konf. CR",
    "col_peak": "Anstieg",
    "col_inrange": "im Ziel",
    "col_hypo": "Hypos",
    "col_meals": "Mahlz.",
    "recommendations": "Empfehlungen",
    "no_recommendations": "Keine Änderungen mit ausreichender Evidenz empfohlen.",
    "insufficient": "Zu wenig Daten für",
    "narrative": "Zusammenfassung",
    "trend": "Trend gegenüber vorherigem Lauf",
    "backtest": "Ergebnis früherer Empfehlungen",
    "applied": "umgesetzt",
    "applied_yes": "ja",
    "applied_no": "nein",
    "applied_unknown": "unbekannt",
    "outcome_improved": "verbessert",
    "outcome_worsened": "verschlechtert",
    "outcome_unchanged": "unverändert",
    "outcome_unknown": "unbekannt",
    "no_delivery_note": "Mögliche Insulin-Abgabestörung (betroffene Werte ausgeschlossen)",
    "dia": "Insulinwirkdauer (DIA)",
    "clamp_note": "Sicherheitsbegrenzung angewendet",
    "confidence": "Sicherheit",
    "caveats": "Hinweise",
    "inference_only": "Keine settings.json gefunden: CR/CF werden nur aus den Daten geschätzt (Inferenzmodus).",
    "mode_llm": "Empfehlungen vom Sprachmodell (BAML)",
    "mode_rules": "Empfehlungen vom deterministischen Regelwerk (--no-llm)",
    "charts_written": "Diagramme geschrieben nach",
    "result_written": "Ergebnis gespeichert unter",
}

# German names for the default time blocks (keyed by the English block name).
BLOCK_DE = {
    "night": "Nacht",
    "breakfast": "Frühstück",
    "lunch": "Mittag",
    "afternoon": "Nachmittag",
    "dinner": "Abend",
    "late": "Spät",
}

# Standing caveats shown at the bottom of every report.
CAVEATS = [
    "CR und CF sind aus den Daten abgeleitet, nicht aus den Geräteeinstellungen gelesen.",
    "Das effektive CR kann eine vom Loop hinzugefügte Korrektur enthalten.",
    "Korrekturfaktor-Aussagen sind im Closed Loop grundsätzlich unsicher.",
    "Bei vermuteter Insulin-Abgabestörung (Pumpe/Katheter) werden betroffene Werte "
    "ausgeschlossen; IOB und CR-Schätzung nutzen die konfigurierte Insulinwirkdauer.",
    "Jede Änderung ist auf +/-10 % pro Lauf begrenzt; bitte klein und schrittweise umsetzen.",
    "Dies ist eine Entscheidungshilfe zur Besprechung mit dem Behandlungsteam, keine ärztliche Anweisung.",
]

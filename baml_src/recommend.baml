// Typed output for the reasoning step. The input is the analysis snapshot as a
// JSON string (assembled deterministically in Python); the model only exercises
// judgment over already-computed numbers. All numeric guardrails (+/-10% cap,
// minimum-sample gate, CF confidence cap) are enforced in code AFTER this call,
// so the model has no arithmetic authority.

class Proposal {
    block string          @description("time-block key, e.g. \"06-11\"")
    parameter string      @description("\"CR\" (carb ratio) or \"CF\" (correction factor)")
    direction string      @description("\"down\", \"up\" or \"hold\" — the change to the numeric value")
    current_value float?  @description("current parameter value (configured if known, else effective)")
    proposed_value float? @description("proposed new value; null when direction is hold or unknown")
    confidence string     @description("\"low\", \"medium\" or \"high\"")
    rationale string      @description("GERMAN. One or two sentences citing the evidence.")
    caveats string        @description("GERMAN. Key uncertainty for this proposal.")
}

class RecommendationSet {
    proposals Proposal[]
    overall_narrative string       @description("GERMAN. Short summary of glycemic status and the main levers.")
    insufficient_data_blocks string[] @description("block keys with too little evidence to judge")
}

function RecommendSettings(snapshot_json: string) -> RecommendationSet {
    client Reasoner
    prompt #"
        Du bist ein Assistent für die Analyse von Typ-1-Diabetes-Daten. Der Nutzer
        verwendet ein CamAPS-FX-Hybrid-Closed-Loop-System mit FreeStyle Libre 3 und
        mylife YpsoPump. Ziel ist es, die Time-in-Range (TIR, 70-180 mg/dl) durch
        vorsichtige Anpassungen des Kohlenhydratverhältnisses (CR, g/E) und – mit
        deutlich geringerer Sicherheit – des Korrekturfaktors (CF, mg/dl pro E) zu
        verbessern.

        Wichtige Rahmenbedingungen:
        - Die konfigurierten Einstellungen sind NICHT im Export enthalten. Das
          effektive CR wird pro Mahlzeit als carbs/Einheiten geschätzt; der CF wird
          nur schwach aus Korrektur-Boli abgeleitet (kein geloggter Auslöse-BZ,
          Basalrate wird vom Loop laufend angepasst). Behandle CF-Aussagen als
          Beobachtungen mit niedriger Sicherheit.
        - Sei KONSERVATIV: Schlage eine Änderung nur vor, wenn die Evidenz in einem
          Block stark und konsistent ist (ausreichende Stichprobe, klares Muster).
          Sonst direction = "hold" oder nenne den Block in insufficient_data_blocks.
        - Das System begrenzt jede Änderung nachträglich hart auf +/-10 % pro Lauf;
          halte deine Vorschläge in diesem Rahmen.

        Bedeutung der Richtung (proposed_value ist der NEUE Zahlenwert):
        - CR (g/E): Bei hohem Anstieg nach dem Essen / viel Zeit über 180 / niedriger
          TIR OHNE Unterzuckerungen ist zu wenig Insulin unterwegs → CR-Zahl SENKEN
          (mehr Insulin pro KH). Bei häufigen Unterzuckerungen nach dem Essen
          (pct_post_meal_hypo) ist zu viel Insulin unterwegs → CR-Zahl ERHÖHEN.
        - CF (mg/dl pro E): nur bei klarer Evidenz und stets mit niedriger Sicherheit.

        Nutze bei vorhandener Historie die Trend-Deltas (trend_effective_cr_delta,
        trend_tir_delta), um die Reaktion auf frühere Änderungen zu berücksichtigen.

        Gib rationale, caveats und overall_narrative auf DEUTSCH aus. Dies ist eine
        Entscheidungshilfe zur Besprechung mit dem Behandlungsteam, keine ärztliche
        Anweisung. Verwende ausschließlich die untenstehende Evidenz.

        {{ ctx.output_format }}

        Analyse-Snapshot (JSON):
        {{ snapshot_json }}
    "#
}

# Researcher: CamAPS FX & Cambridge Artificial Pancreas Algorithm

## 📄 Primary Algorithm Papers & Mathematical Foundations

1. **Nonlinear Model Predictive Control of Glucose Concentration in Subjects with Type 1 Diabetes**
   - **Authors:** Roman Hovorka, Valentina Canonico, Ludovic J. Chassin, Ulrich Haueter, et al.
   - **Publication:** Physiological Measurement, 2004 (Vol. 25, No. 4, pp. 905–920)
   - **URL:** https://pubmed.ncbi.nlm.nih.gov/15382830/
   - **Summary:** Das fundamentale Paper von Roman Hovorka zum Cambridge-Regelalgorithmus. Es beschreibt das physiologische 9-Kompartiment-Modell (Glukose-Kinetik, Insulin-Absorption von Lispro/Aspart, Darm-Absorption) sowie die mathematische Formulierung der nichtlinearen Modellprädiktiven Regelung (NMPC) mit bayesscher Parameterschätzung zur Echtzeit-Adaption der Insulinempfindlichkeit.

2. **Simulation Environment to Evaluate Closed-Loop Insulin Delivery Systems in Type 1 Diabetes**
   - **Authors:** Malgorzata E. Wilinska, Ludovic J. Chassin, Carlo L. Acerini, Janet M. Allen, David B. Dunger, Roman Hovorka, et al.
   - **Publication:** Journal of Diabetes Science and Technology, 2010 (Vol. 4, No. 1, pp. 132–144)
   - **URL:** https://pmc.ncbi.nlm.nih.gov/articles/PMC2825634/
   - **Summary:** Beschreibung der Simulationsumgebung und des virtuellen Patientenkohorten-Modells (Cambridge In-Silico Simulator). Essentiell für den Software-Nachbau, da hier Parametersätze, stochastische Variationen der Insulinempfindlichkeit und Mahlzeiten-Resorption mathematisch definiert sind.

3. **Algorithms for a Closed-Loop Artificial Pancreas: The Case for Model Predictive Control**
   - **Authors:** B. Wayne Bequette
   - **Publication:** Journal of Diabetes Science and Technology, 2013 (Vol. 7, No. 6, pp. 1632–1643)
   - **URL:** https://pmc.ncbi.nlm.nih.gov/articles/PMC3876342/
   - **Summary:** Analyse der relativen Vorzüge von Model Predictive Control (MPC) gegenüber Proportional-Integral-Derivative (PID) für den künstlichen Pankreas. Betont, dass beide Ansätze keine Einzelalgorithmen sind, sondern Strategien, und dass MPC explizite Insulin-Begrenzungen (Constraints), Meals und andere Störungen über einen allgemeinen Rahmen einbeziehen kann.

4. **Substance Monitoring and Control in Human or Animal Bodies** (the Cambridge multi-model algorithm; the US family member is titled *Glucose Monitoring and Control Using Multi-Model Approach*)
   - **Authors:** Roman Hovorka (inventor), assigned to Cambridge Enterprise Ltd
   - **Patent:** CA2702345C / US9402953B2
   - **URL:** https://patents.google.com/patent/CA2702345C/en
   - **Summary:** Patentdokumentation des multi-modell-basierten Algorithmus, der in CamAPS FX zum Einsatz kommt. Enthält exakte Steuerungs- und Vorhersagealgorithmen, Gewichtigungsverfahren für Multi-Modell-Prädiktoren und Sicherheitsgrenzen für die Dosierung.

## 📄 Clinical & Real-World Efficacy Studies for CamAPS FX

5. **Cambridge Hybrid Closed-Loop Algorithm in Children and Adolescents with Type 1 Diabetes**
   - **Authors:** Julia Ware, Charlotte K. Boughton, Janet M. Allen, Malgorzata E. Wilinska, Roman Hovorka, et al.
   - **Publication:** The Lancet Digital Health, 2022 (Vol. 4, No. 4, pp. e245–e255)
   - **URL:** https://www.camdiabtraining.com/content-documents/60/Hybrid-closed-loop-in-children-and-adolescents-with-type-1-Dan05-Ware-2022.pdf
   - **Summary:** Pivotal-Studie zur Validierung des CamAPS FX Algorithmus auf der Ypsopump (mylife CamAPS FX) und Dana RS Pump mit Dexcom G6 CGM. Liefert detaillierte klinische Metriken (TIR, Basal- vs. Bolus-Verhältnis, Anpassungen der Zielwerte).

6. **Real-World Evidence on the CamAPS FX Hybrid Closed-Loop System in People Living with Type 1 Diabetes**
   - **Authors:** Charlotte K. Boughton, Tomas Hovorka, Malgorzata E. Wilinska, Sara Hartnell, Roman Hovorka
   - **Publication:** Metabologia / Springer Medicine, 2026 (Analyse von >35.000 CamAPS FX Nutzern)
   - **URL:** https://www.repository.cam.ac.uk/items/58b2eb8e-6f28-4463-acbe-bc8ef359ba8b
   - **Summary:** Bisher umfangreichste Real-World-Datenanalyse zu CamAPS FX mit über 35.000 Patienten across 19 Ländern. Zeigt genaue Verteilungen von Time in Range (3,9–10,0 mmol/l = 70–180 mg/dl), Zielwerteinstellungen (z. B. Standard 5,8 mmol/l vs. individuelle Ziele) und deren Auswirkung auf die TIR.

7. **Randomized Trial of Closed-Loop Control in Very Young Children with Type 1 Diabetes**
   - **Authors:** Julia Ware, Janet M. Allen, Charlotte K. Boughton, Roman Hovorka, et al.
   - **Publication:** New England Journal of Medicine (NEJM), 2022 (Vol. 386, No. 3, pp. 209–219)
   - **URL:** https://www.cam.ac.uk/stories/KidsArtificialPancreas
   - **Summary:** Studie zum Einsatz von CamAPS FX bei Kleinkindern (1–7 Jahre). Beschreibt die Herausforderungen der hohen Glukose-Variabilität und wie die adaptive Kompartiment-Modellierung niedrige Gesamttagesdosen präzise regelt.

## 🔧 Technical Overview: Key Components of the Cambridge (CamAPS FX) Algorithm

- **Model Predictive Control (MPC):** Verwendet ein physiologisches Glukose-Insulin-Kompartimentmodell mit 9 festen Parametern und 6 dynamischen Parametern.
- **Bayesian Real-Time Adaptation:** Die 6 dynamischen Parameter (u. a. Insulinempfindlichkeit, Insulingewebeverteilung) werden kontinuierlich über Bayessche Verfahren anhand der historischen CGM- und Insulindaten aktualisiert.
- **Multi-Model Strategy:** Nutzt mehrere Sub-Modelle parallel, um plötzliche Empfindlichkeitsänderungen (z. B. Sport, Nacht, Mahlzeiten) abzufangen.
- **Adjustable Target Range / Ease-off Mode:** Standard-Zielwert liegt bei 5,8 mmol/l (104 mg/dl), kann aber dynamisch angepasst oder über den "Ease-off"-Modus (erhöhter Zielwert bei Aktivität) modifiziert werden.

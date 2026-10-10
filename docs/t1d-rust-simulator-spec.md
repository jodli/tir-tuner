# Simulator spec: virtual patient, pump and CGM

The in-silico testbed the controller runs against: the Wilinska 2010 (W10) virtual patient in `tir-tuner-body`, the pump model in `tir-tuner-body/src/pump.rs`, and the Breton & Kovatchev 2008 (B08) sensor in `tir-tuner-cgm`. The loop is wired in `tir-tuner-cli/src/engine.rs`.

This document describes what the code does. Every number in it is a claim in `docs/sources.toml`, named in brackets, e.g. [w10-table1]. Page numbers are PDF pages of the papers in `docs/papers/` (local only). Where no source exists, the claim is marked `assumption` there and says so here.

---

## 1. Loop

```
 pump (W10 error, YpsoPump bolus speed)
   |  insulin influx (mU/min)
   v
 body: W10 model, clamped Euler, dt 0.25 min
   |  interstitial glucose C every 15 min
   v
 CGM: B08 eq 7 error, whole mg/dL
   |  reading
   v
 controller (tir-tuner-aps, see camaps_fx_kani_specification.md)
```

* The body steps with clamped forward Euler every 0.25 min [own-body-step]: every compartment is floored at 0 after each step. W10 doesn't give its solver; the fastest rate, ke = 0.14 /min, times 0.25 min is 0.035.
* The controller and the sensor work on one 15-min period [h04-period, b08-interval]. The engine panics if the two periods differ, because B08's error correlation is per 15-min sample.
* The subject starts at rest at the admit glucose, on the basal that holds it there, as in W10 p7. That basal comes from bisecting the steady-state glucose balance on [0, 1/S_IE], 60 halvings [math-bisection].
* Meals and boluses act at their own minute, not at period boundaries.

---

## 2. Model equations (W10 Appendix, p12-13)

Units: insulin masses S1, S2 in mU, plasma insulin I in mU/L, glucose masses Q1, Q2 in mmol/kg, gut masses G1, G2 in mmol, concentrations in mmol/L, rates per minute. `w` is body weight (kg).

### 2.1 Insulin

$$\frac{dS_1}{dt} = u(t) - k_a S_1, \qquad \frac{dS_2}{dt} = k_a (S_1 - S_2), \qquad \frac{dI}{dt} = \frac{k_a S_2}{V_I\,w} - k_e I$$

One rate `ka` for both depots and for basal and bolus, as in W10. `u(t)` is the pump's insulin influx (mU/min).

### 2.2 Insulin action

$$\frac{dx_1}{dt} = k_{b1}(S_{IT} I - x_1), \qquad \frac{dx_2}{dt} = k_{b2}(S_{ID} I - x_2), \qquad \frac{dx_3}{dt} = k_{b3}(S_{IE} I - x_3)$$

x1 and x2 are in /min, x3 is dimensionless. This is W10's `-k_a x + S k_b I` with `k_a = k_b` written as a relaxation.

### 2.3 Glucose

$$\frac{dQ_1}{dt} = EGP + U_G - F_{01}^c - x_1 Q_1 + k_{12} Q_2 - F_R, \qquad \frac{dQ_2}{dt} = x_1 Q_1 - (k_{12} + x_2) Q_2, \qquad G = \frac{Q_1}{V_G}$$

* **EGP:** $EGP = \max(0,\ EGP_0 (1 - x_3))$ [h04-egp]. H04 eq 1 and H02 eq 7 have the sign `1 - x3`; W10 prints `1 + x3`, which is a typo. The floor at 0 is W10's. There is no cap.
* **Non-insulin-dependent uptake:** $F_{01}^c = \frac{F_{01}}{0.85}\,\frac{G}{G + 1}$ [w10-f01c]. H04 eq 2 has a piecewise form with a 4.5 mmol/L threshold instead; we follow W10 because the parameters are W10's.
* **Renal excretion:** $F_R = \max(0,\ R_{cl}(G - R_{thr}) V_G)$ (W10 p12), per kg, no body weight factor.

### 2.4 Gut

$$\frac{dG_1}{dt} = -\frac{G_1}{t_{max}}, \qquad \frac{dG_2}{dt} = \frac{G_1 - G_2}{t_{max}}, \qquad U_G = \min\left(\frac{G_2}{t_{max,G}\,w},\ UG_{ceil}\right)$$

* A meal adds `Bio · carbs · 5.551` mmol to G1 at once. W10 puts Bio on the input, `Bio D(t)` [w10-bio].
* Above the flux ceiling, W10 stretches `t_max = max(t_max,G, G2 / (UG_ceil w))` for the whole chain, so absorption slows and no carbs are lost [w10-ug-ceil].

### 2.5 Interstitial glucose and CGM

$$\frac{dC}{dt} = k_{a,int}(G - C)$$

C already carries the lag, so the sensor adds no extra delay.

Measurement error, B08 eq 7 (p8) [b08-eq7, b08-johnson]:

$$e_1 = v_1, \qquad e_n = 0.7\,(e_{n-1} + v_n), \qquad v_n \sim \mathcal{N}(0, 1)$$
$$\varepsilon_n = \xi + \lambda \sinh\!\left(\frac{e_n - \gamma}{\delta}\right), \qquad CGM_n = C(t_n) + \varepsilon_n \ \text{(mg/dL)}$$

* Johnson SU parameters (B08 Table 1, mg/dL): λ = 15.96, ξ = -5.471, δ = 1.6898, γ = -0.5444.
* B08 reports mean 0.76 and SD 11 mg/dL. Eq 7 with these parameters gives about 0.7 and 11.7, because the driver's variance is 0.96, not 1.
* Readings are whole mg/dL, floored at 0 [user-cgm-integers]. No calibration gain and no drift: B08 publishes no population values for either, and Libre 3 is factory calibrated (CamAPS manual p93).

---

## 3. Parameters

There is one nominal subject, scaled to a body weight (`VirtualSubject::nominal`). W10 publishes means and priors, not its 18 parameter sets, so they can't be rebuilt.

| Symbol | Value | Source | Notes |
|---|---|---|---|
| w | 74.9 kg default; the user's weight in the replay | [w10-population] | W10 gives 74.9 ± 14.4 kg over its 18 subjects, no sampling distribution |
| F01 | 11.1 µmol/kg/min | [w10-table1] | oscillatory |
| EGP0 | 16.9 µmol/kg/min | [w10-table1] | EGP at zero insulin; oscillatory |
| k12 | 0.060 /min | [w10-table1] | oscillatory |
| S_IT | 18.41e-4 /min per mU/L | [w10-table1] | oscillatory |
| S_ID | 5.05e-4 /min per mU/L | [w10-table1] | oscillatory |
| S_IE | 0.019 per mU/L | [w10-table1] | oscillatory |
| kb1, kb2, kb3 | 0.0034, 0.056, 0.024 /min | [w10-table1] | oscillatory |
| V_G | 0.15 L/kg | [w10-table2] | |
| V_I | 0.12 L/kg | [w10-table2] | |
| ke | 0.14 /min | [w10-table2] | oscillatory |
| ka | 0.0207 /min | [heise-fiasp-ka] | Fiasp, fitted to Heise 2017 CSII Table 1; W10's lispro value is 0.018. Oscillatory |
| t_max,G | 40 min | [w10-table2] | |
| UG_ceil | 0.0275 mmol/kg/min | [w10-ug-ceil] | mean of W10's U(0.02, 0.035) |
| Bio | 0.95 | [w10-bio] | mean of W10's U(70%, 120%) |
| R_thr | 9.0 mmol/L | [w10-table2] | |
| R_cl | 0.01 /min | [w10-table2] | |
| k_a,int | 0.0933 /min | [w10-table2] | median of W10's lognormal prior, exp(-2.372), so τ ≈ 10.7 min; oscillatory |

### 3.1 Variability

* **Intra-subject oscillation:** the 12 parameters marked oscillatory above swing sinusoidally with 5% amplitude and a 3 h period, each with a random phase in [0, 3 h] [w10-oscillation].
* **Meal-to-meal bioavailability:** each meal's Bio is multiplied by a draw with CV 20% [w10-bio-interoccasion]. W10 states "an additional 20% interoccasion variability" without a distribution; the normal draw floored at 0 is ours.
* Both are switched off together by `SimConfig::variability`.

### 3.2 Meal boluses in scenarios

Scenario meal boluses use Walsh 2011: basal = 0.48 TDD (eq 1) and carb factor = 5.7 w / TDD g/U (eq 2), from the subject's own resting basal at the default target [walsh-basal-share, walsh-carb-factor]. W10's population mean ratio of 1.7 U/10 g takes the mean subject to 2.8 mmol/L after 60 g, so it doesn't fit the nominal subject. Replays of a Glooko export use the user's real boluses instead.

---

## 4. Pump

* **Delivery error:** every programmed rate and every bolus is multiplied by a draw with CV 5%, zero mean, uncorrelated [w10-pump-error]. The YpsoPump IFU (p185) states ±5% for basal and 6 U boluses.
* **Bolus speed:** 0.11 U/s, so 6.6 U/min [ypsopump-bolus-speed]. A bolus runs over the next body steps at that speed.

---

## 5. Not modelled

No source in `docs/papers/` covers these, so they are left out rather than guessed:

1. Libre 3 error, measuring range and LO/HI behaviour. B08 and W10 are FreeStyle Navigator data. The Abbott Libre 3 manual would give the range.
2. Sensor dropouts and compression lows. W10 mentions dropouts without parameters.
3. Facchinetti 2014 drift and AR model. The paper isn't available locally.
4. Dalla Man nonlinear gastric emptying. No Dalla Man paper is in the folder.
5. Between-subject variability. W10 gives no SDs for Table 1, and its 18 subjects aren't published.
6. Exercise and stress.

---

## 6. Checks

Native tests and Kani proofs pin the model against the sources:

1. **Equations:** each right-hand side is checked term by term against W10 p12-13 and H04 eq 1 (`tir-tuner-body/src/derivative.rs` tests).
2. **Rest:** `rest_state` is a fixed point of the model, and the open-loop resting basal holds the admit glucose.
3. **Gut mass balance:** an 87 g meal at 74.9 kg arrives to more than 99% within 8 h, so the ceiling only delays carbs.
4. **CGM:** over 100,000 samples the error has B08's mean, SD and lag-1 autocorrelation of 0.7.
5. **Kani:** `cargo kani -p tir-tuner-body` and `-p tir-tuner-cgm`; `docs/verification_report.html` lists the harnesses.
6. **Mutation gate:** `cargo mutants` over the files in `[lint]` of `docs/sources.toml`, with zero survivors.

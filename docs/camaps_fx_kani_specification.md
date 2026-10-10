# Controller spec: the Cambridge algorithm lineage and its verification

`tir-tuner-aps` re-implements what Cambridge published about its closed-loop algorithm: the glucoregulatory model of patent US8977504B2, the NMPC of Hovorka 2004 (H04), and Boost and Ease-off as Ware 2022/2022b describe them. CamAPS FX itself isn't published. This is the published lineage, not the product, and every place where the product's behaviour is unknown is marked as our assumption.

Every number below is a claim in `docs/sources.toml`, named in brackets, e.g. [h04-period]. Page numbers are PDF pages of the papers in `docs/papers/` (local only). The virtual patient, pump and CGM are in `t1d-rust-simulator-spec.md`.

```
 CGM reading ──> belief update (section 3)
                     │ belief state
                     v
                 NMPC (H04 eq 9) ──> Auto mode rules (Boost, Ease-off, hypo cutoff) ──> pump rate
```

---

## 1. Sources

| Key | Source | Used for |
|---|---|---|
| us504 | US8977504B2, Hovorka R., "Substance monitoring and control in human or animal bodies". US member of the CA2702345C family (PCT/GB2008/050932, WO2009/047569) | The model, the initial state, the IMM and the control law, cited by equation number. CA2702345C as granted has no paragraph numbers, and Hovorka is its only inventor |
| h04 | Hovorka et al. 2004, Physiol Meas 25(4):905-920 | The NMPC objective, trajectory, period, horizon and rate bound; parameter values for the test fixture. Its own model has no interstitial compartment and a linear EGP, so section 2 follows the patent |
| ware2022 | Ware et al. 2022, Lancet Digit Health 4(4):e245-e255 | Default and personal targets, Ease-off |
| ware2022b | Ware et al., J Diabetes Sci Technol 2024;18(4):882-888 | Boost; the default target in mg/dL |
| wilinska2010 | Wilinska et al. 2010, J Diabetes Sci Technol 4(1):132-144 | The virtual patient; k31 in the test fixture |
| camaps-manual | mylife CamAPS FX user manual | Mode descriptions; TDD and weight as inputs |

US9402953B2 (Wilinska et al., with Abbott) and US9089305B2 are different families, not members of CA2702345C.

---

## 2. Model (US8977504B2 eq 1-16)

State: insulin depots `i1`, `i2` (mU), remote actions `r_D`, `r_E` (mU/L), gut carbs `a1`, `a2` (g), glucose masses `q1`, `q2`, `q3` (mmol/kg) and the unexplained influx `u_S` (mmol/kg/min). Inputs: insulin rate `u` (U/h; a bolus is a rate over its delivery time) and meal rate `v_G` (g/min). `W` is body weight (kg).

$$\frac{di_1}{dt} = \frac{1000\,u}{60} - \frac{i_1}{t_{max,I}}, \qquad \frac{di_2}{dt} = \frac{i_1 - i_2}{t_{max,I}}, \qquad i = \frac{i_2}{t_{max,I}\,MCR_I\,W} \quad \text{(eq 1-3)}$$

$$\frac{dr_D}{dt} = p_{2D}(i - r_D), \qquad \frac{dr_E}{dt} = p_{2E}(i - r_E)$$

$$\frac{da_1}{dt} = v_G - \frac{a_1}{t_{max,G}}, \qquad \frac{da_2}{dt} = \frac{a_1 - a_2}{t_{max,G}}, \qquad u_A = \frac{5.551\,a_2}{t_{max,G}\,W} \quad \text{(eq 8)}$$

$$\frac{dq_1}{dt} = -(S_{ID} r_D + k_{21}) q_1 + k_{12} q_2 - F_{01} + EGP(r_E) + F\,u_A + u_S, \qquad \frac{dq_2}{dt} = k_{21} q_1 - k_{12} q_2 \quad \text{(eq 9-10)}$$

$$\frac{dq_3}{dt} = k_{31}(q_1 - q_3), \qquad g_P = \frac{q_1}{V_G}, \qquad g_{IG} = \frac{q_3}{V_G} \quad \text{(eq 11, 16)}$$

* **Gut units:** `u_A` multiplies grams by 5.551 mmol/g (eq 8).
* **F01** is constant and **F** is the carb bioavailability, as in eq 9.
* **EGP** (eq 12) [us504-egp]: $EGP = EGP_B \exp\!\left(-\frac{(r_E - BIC)\ln 2}{I_{1/2}}\right)$, with $BIC = \frac{1000\,BIR}{60\,MCR_I\,W}$ (eq 14). The patent gives no values for `EGP_B` or `I_1/2`. There is no cap.
* **u_S** (eq 13) is estimated by the IMM filter (section 3).
* **Initial state:** insulin at its basal steady state, glucose split as eq 63.
* **Numerics:** forward Euler, every compartment clamped at zero, 1 min steps [own-rollout-step]. The patent discretizes exactly (eq 18-45); 1 min keeps `k31 dt` well below 1.

### 2.1 Parameters

The patent names the parameters without values. `HovorkaParams::from_weight_and_tdd` builds them from the two numbers CamAPS asks for, weight and TDD (manual p29, p70) [own-tdd-sensitivity]:

1. Walsh eq 1 gives the basal requirement, `BIR = 0.48 TDD / 24` [walsh-basal-share], and eq 14 gives `BIC` from it and the weight.
2. Eq 12 defines `BIC` as the insulin that holds 5.5 mmol/L [us504-bic-rest]. H04's population values [h04-table1, h04-table2] can't hold rest at a T1D basal, so H04's three sensitivities are scaled by one factor until H04's model rests there (0.39 for 36 U/day at 50 kg). The manual calls TDD "an initial indicator of insulin sensitivity" (p70).
3. The patent's parameters linearize the scaled H04 model at `BIC`: `EGP_B` and `I_1/2` match H04's EGP in level and slope, `S_ID` matches the slope of H04's insulin-dependent uptake, `k21` is H04's `x1` at basal, and `F01` closes the rest at 5.5. `t_max,I` is H04's lispro 55 min, `k31` Wilinska 2010's median [w10-table2].

CamAPS then learns these values (ware2022 p4); this model stays fixed. Tests and proofs use the same constructor.

---

## 3. Belief update

`tir-tuner-aps/src/kalman.rs` runs the patent's interacting multiple model filter (eq 53-82) on the subset state `[q1, q2, q3, u_S]` (eq 46, without the bioavailability `F`):

* **Predict** (eq 53-59, 70-71): the belief is stepped through the period with the known inputs. The map is affine in the subset states, so unit steps give the transition matrix `F` exactly (eq 55 allows numerical coefficients). The `u_S` shock enters where a unit `u_S` does (eq 56, 59).
* **Update** (eq 60-62, 72-77): the reading is `q3 / V_G` (eq 61) plus a CGM error state. The patent takes the measurement noise as white (eq 62); the simulated sensor's error is B08's AR(1), and a white-noise filter reads its runs as glucose changes. So ours carries the error as a fifth state with B08's 0.7 per period [b08-eq7] and SD 11 mg/dL [own-kf-sensor-sd].
* **Modes** (eq 65-69, 79-82): three filters that differ in `σ_w` [own-imm-process], mixed and weighted by their likelihood. Mode count and transition matrix are ours [own-imm].
* **Horizon:** no measurement arrives in the prediction, so the NMPC lets `u_S` regress to zero (eq 57) with our halftime [own-us-halftime].

On its own model with B08 noise the estimate beats the raw reading (`estimator_beats_the_raw_reading`). The dips that remain after bolused meals come from the NMPC's overshoot and the belief's model mismatch (section 8), not from the estimate.

---

## 4. NMPC (H04 eq 9)

Every 15 min [h04-period] the controller predicts plasma glucose over 240 min [h04-horizon] for candidate rate sequences `u` and minimizes

$$J(u) = \sum_{j=1}^{N} \big(g_P(t + j) - w(t + j)\big)^2 + \frac{1}{k_{agr}} \sum_{j=1}^{N} \big(u(t + j) - u(t + j - 1)\big)^2, \qquad 0 \le u \le 4\ \text{U/h}$$

* The prediction tracks plasma glucose, as H04 eq 9 and the patent's eq 83 do. Sample `j` is compared with the target at the same time `t + j`.
* `u(t)` in the effort term is the rate delivered in the previous period.
* **Rate bound:** 4 U/h, H04's value for 2004 pumps [h04-max-rate]. CamAPS sets it per person (manual p70) and doesn't publish the rule.
* **k_agr = 5** [own-kagr]. H04 gives the tuning rule (fewer than 5% of 36 runs below 3.3 mmol/L, fasting near 6), not the value. Every value from 0.5 to 50 meets it on the simulation, and none changes the dips after meals, which come from W10's parameter variability.
* **Trajectory** `w` (H04 section 3.3, Fig 4) [h04-trajectory]: starts at the belief's plasma glucose, falls 2 mmol/L/h while more than 2 mmol/L above target, 1 mmol/L/h between that and the target, and rises toward the target with a 15 min halftime below it. H04 states the bands for a target of 6 as 8 and 6 mmol/L; here they are relative to the personal target.
* **Solver** [own-optimizer]: a constant-rate grid of 7 rates `k u_max / 6` seeds a coordinate descent in steps of `u_max / 30`, at most 50 passes (H04 capped Marquardt at 50 iterations), never increasing the cost. H04 uses Marquardt; the patent has a closed form (eq 99). Only the first rate is applied (H04 p7).

---

## 5. Auto mode rules

Applied to the NMPC rate, in `Mode::rate`:

| Rule | Value | Source |
|---|---|---|
| Default target | 5.8 mmol/L (104.5 mg/dL) | ware2022 p4; 104.5 mg/dL ware2022b p3 |
| Personal target range | 4.4-11.0 mmol/L, 30 min blocks | [ware2022-target-range]; the mg/dL edges the app shows aren't in the sources |
| Boost | rate × 1.35 while the reading is above target | [ware2022b-boost]: "approximately 35%", glucose-responsive, off at target |
| Ease-off | no algorithm insulin below 7.0 mmol/L | [ware2022-ease-off] |
| Ease-off target | at least 7.0 mmol/L | [own-ease-off-target]: the raised target isn't published (manual p47 "slightly higher") |
| Hypo cutoff | no algorithm insulin below 4.4 mmol/L | [own-hypo-cutoff]: no source gives a suspend threshold. 4.4 is the lowest personal target; the manual (p70) only says insulin may be reduced further when glucose is low or falling |

---

## 6. Simulation wiring (`tir-tuner-cli/src/engine.rs`)

* Each period: CGM reading, belief update (section 3), NMPC, Auto mode rules, pump.
* The belief hears of announced meals and of every bolus, applied in the first minute of the period. Unannounced meals reach it only through the CGM.
* With `use_controller` off the pump delivers the subject's resting basal, still suspended below the hypo cutoff. That is the open-loop comparison arm.

---

## 7. Verification

Kani proves bounded symbolic statements; long `f64` chains, the full horizon and the IMM identities are covered by native tests, proptests and fuzz targets.

### 7.1 Kani

`cargo kani -p tir-tuner-aps` and `-p tir-tuner-common`. `docs/verification_report.html` lists every harness with its doc comment and assertions, generated from the code by `tools/gen_verification_report.py`.

### 7.2 Native tests and fuzzing

* The patent's eq 3, 8, 12, 14 and 16 are checked term by term, the H04 trajectory bands on worked numbers, and the eq 9 cost against a hand-computed rollout with prediction and target at the same time.
* The coordinate descent reaches a local minimum on its lattice within its pass cap.
* Proptests cover non-negativity and finiteness of long model walks and the full-horizon NMPC sequence.
* `fuzz/fuzz_targets/hovorka_step.rs` and `nmpc_sequence.rs` soak the same invariants with `cargo +nightly fuzz run <target>`.

### 7.3 Source and mutation gates

* `tir-tuner-common/tests/sources.rs` fails when a literal in a gated file isn't a tagged const, when a tag names no claim, or when a claim's values differ from the code.
* `cargo mutants` over the same files, with zero survivors.

---

## 8. Open

The assumptions are marked where they apply: the weight + TDD model (2.1), the filter's noise values (3), `k_agr` and the solver (4), the hypo cutoff and the Ease-off target (5). Not covered there:

1. **Control cadence:** CamAPS FX adjusts every 8-12 min (ware2022 p4); the code uses H04's 15 min, which the sensor model also needs.
2. **No adaptive learning.** CamAPS learns daily insulin needs, diurnal variations and meal patterns (ware2022 p4); none of it is published in detail.

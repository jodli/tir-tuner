# Technical Specification & Formal Verification Blueprint: Cambridge Artificial Pancreas Algorithm (CamAPS FX) & Kani Verification Suite

---

## 1. Executive Summary & Architecture Overview

This document specifies the technical, mathematical, and verification requirements for re-implementing the **Cambridge Hybrid Closed-Loop Algorithm (CamAPS FX)** in Rust and verifying it using the **Kani Rust Verifier**. 

The goal of this implementation is to provide a deterministic, mathematically verified core engine for:
1. **In-Silico Simulation & Data Science Experiments**: Running closed-loop scenarios against virtual patient populations to optimize Time in Range (TIR) tuning algorithms.
2. **Formal Safety Verification**: Proving via model checking (Kani) that the controller satisfies critical physiological non-negativity constraints, hard hypoglycemia cutoffs, probability bounds, and numerical stability (panic-freedom).

```
 +------------------+      CGM Reading (z_k)      +-----------------------------------------+
 | Continuous       | --------------------------> | Interacting Multiple Model (IMM)        |
 | Glucose Monitor  |                             | State Estimator & Bayesian Adaptation   |
 +------------------+                             +-----------------------------------------+
                                                                     |
                                                          State Estimate (x_k) &
                                                          Covariances / Probabilities
                                                                     v
 +------------------+   Infusion Rate Command u(t) +-----------------------------------------+
 | Insulin Pump     | <-------------------------- | Nonlinear Model Predictive Control      |
 | (mylife Ypsopump)|                             | (NMPC) Dosing Calculator & Safety Layer|
 +------------------+                             +-----------------------------------------+
```

---

## 2. Grounding Sources & Reference Mapping

When developing and reviewing code, the agent should refer to the following sources in the notebook:

1. **Patent CA2702345C ("Substance monitoring and control in human or animal bodies", Hovorka & Wilinska)**:
   * *Role*: Primary reference for discrete-time state transitions, the 10-dimensional extended state vector, the IMM mixing equations [102–108], and the explicit matrices of the NMPC cost function [109–116].
2. **Hovorka et al. (2004) ("Nonlinear model predictive control of glucose concentration in subjects with type 1 diabetes")**:
   * *Role*: Mathematical definition of the 9-compartment physiological glucoregulatory model, subcutaneous Lispro/Aspart kinetics, gut absorption, and EGP suppression.
3. **Ware et al. (2022) ("Cambridge hybrid closed-loop algorithm in children and adolescents with type 1 diabetes")**:
   * *Role*: Clinical target ranges and operational parameters of CamAPS FX: nominal target $5.8\text{ mmol/L}$, user-adjustable range $4.4\text{–}11.0\text{ mmol/L}$, Ease-off mode target $7.0\text{ mmol/L}$, and Boost mode (+35% delivery).
4. **Boughton et al. (2026) ("Real-world evidence on the CamAPS FX hybrid closed-loop system in people living with type 1 diabetes")**:
   * *Role*: Real-world data distributions (>35,000 users across 19 countries) for validating baseline basal/bolus ratios and age-dependent glucose targets.
5. **Wilinska et al. (2010) ("Simulation Environment for In Silico Testing...")**:
   * *Role*: Parameter distributions for virtual T1D patient populations used in simulation scenarios.
6. **Facchinetti et al. (2014) ("Modeling the glucose sensor error")**:
   * *Role*: The CGM measurement error model: a lagged, autocorrelated first-order (AR(1)) additive error on the interstitial glucose, with the calibration gain applied in the CGM crate.
7. **Breton & Kovatchev (2008) ("Analysis, modeling, and simulation of the accuracy of continuous glucose sensors")**:
   * *Role*: Continuous-glucose-sensor error analysis and simulation; jointly with Facchinetti et al. (2014) the source of the CGM error structure.
8. **Patek et al. (2009) ("In silico preclinical trials: methodology and engineering guide to closed-loop control in type 1 diabetes mellitus")**:
   * *Role*: The alignment-free outcome measures (time in range, mean glucose, coefficient of variation) used by the clinical reporting metrics.
9. **Bequette (2013) ("Algorithms for a Closed-Loop Artificial Pancreas: The Case for Model Predictive Control")**:
   * *Role*: Justification of the NMPC dose-calculator formulation (spec section 5.1).
10. **Ware et al. (2022b) ("Safety of user-initiated intensification of insulin delivery using Cambridge hybrid closed-loop algorithm")**:
    * *Role*: Safety evidence behind the Boost-mode intensification factor.
11. **Alwan et al. (2023) ("Real-World Evidence Analysis of a Hybrid Closed-Loop System")**:
    * *Role*: Real-world TIR distributions; one of the sources of the `3.9`-`10.0` mmol/L TIR band.

**Section-numbering conventions.** Section references in this document
follow the target source's own numbering. "eq. 9" and "section 3.3" (the
NMPC objective and the moving target trajectory) are numbered as inside
Hovorka et al. (2004). "spec section 3.2A-F" and "spec section 5.1" point
at *this* specification's headings. "Paragraphs [0102]-[0116]" are the
paragraph numbers of patent CA2702345C. The verification-report reference
keys `[W04]`, `[W10]`, `[BQ13]`, `[W22]`, `[W22B]`, `[B26]`, `[A23]`,
`[F14]`, `[B08]`, `[P09]`, `[CA2345]` map to this section's numbered list.

---

## 3. Mathematical & Physiological Submodels

### 3.1 Extended 10-Dimensional State Vector
The glucoregulatory state at time $k$ is defined by the extended vector:
$$x_{e,k} = \begin{bmatrix} i_{1,k} & i_{2,k} & r_{D,k} & r_{E,k} & a_{1,k} & a_{2,k} & q_{1,k} & q_{2,k} & q_{3,k} & u_{S,k} \end{bmatrix}^T$$

| State Component | Description | Unit |
|---|---|---|
| $i_1, i_2$ | Subcutaneous insulin masses in depot 1 and depot 2 | mU |
| $r_D$ | Remote insulin action on peripheral glucose disposal | $\text{mU/L}$ |
| $r_E$ | Remote insulin action on hepatic endogenous glucose production (EGP) | $\text{mU/L}$ |
| $a_1, a_2$ | Carbohydrate masses in gut absorption depot 1 and depot 2 | Grams (g) |
| $q_1$ | Accessible glucose mass in plasma compartment | $\text{mmol/kg}$ |
| $q_2$ | Non-accessible glucose mass in peripheral tissue compartment | $\text{mmol/kg}$ |
| $q_3$ | Interstitial fluid glucose mass (measured by CGM) | $\text{mmol/kg}$ |
| $u_S$ | Unexplained stochastic glucose influx (process noise state) | $\text{mmol/kg/min}$ |

---

### 3.2 Continuous-Time Differential Equations

#### A. Subcutaneous Insulin Absorption Submodel
$$\frac{di_1(t)}{dt} = -\frac{1}{t_{max,I}} i_1(t) + \frac{u(t)}{60} + v(t)$$
$$\frac{di_2(t)}{dt} = \frac{1}{t_{max,I}} \left(i_1(t) - i_2(t)\right)$$
$$i(t) = \frac{i_2(t)}{t_{max,I} \cdot MCR_I \cdot W}$$
*Where $u(t)$ is basal insulin infusion rate ($\text{U/h}$), $v(t)$ is manual insulin bolus ($\text{U}$), $t_{max,I}$ is time-to-peak absorption ($\text{min}$), $MCR_I$ is metabolic clearance rate ($\text{L/kg/min}$), and $W$ is body weight ($\text{kg}$). The depot masses $i_1, i_2$ are carried in millie-units (mU) per Hovorka et al. 2004; the dosing inputs are converted internally ($\times 1000$), so the basal depot equilibrium under $u(t) = BIR$ sits exactly at the basal insulin concentration $BIC$ (mU/L) defined in submodel D. This unit convention is pinned by the `basal_steady_state_matches_target` native test (EGP returns to $EGP_B$ and fasting glucose $\approx 5.8\text{ mmol/L}$ at the default parameters), which the symbolic saturation proofs cannot observe.*

#### B. Insulin Action Submodel
$$\frac{dr_D(t)}{dt} = p_{2D} \left(i(t) - r_D(t)\right)$$
$$\frac{dr_E(t)}{dt} = p_{2E} \left(i(t) - r_E(t)\right)$$
*Where $p_{2D}$ and $p_{2E}$ are fractional disappearance rates ($/\text{min}$) for remote insulin action.*

#### C. Gut Carbohydrate Absorption Submodel
$$\frac{da_1(t)}{dt} = -\frac{1}{t_{max,G}} a_1(t) + v_G(t)$$
$$\frac{da_2(t)}{dt} = \frac{1}{t_{max,G}} \left(a_1(t) - a_2(t)\right)$$
$$u_A(t) = \frac{a_2(t)}{t_{max,G} \cdot W \cdot 5.551} \quad (\text{mmol/kg/min})$$
*Where $v_G(t)$ is meal ingestion rate ($\text{g/min}$) and $t_{max,G}$ is gut absorption time-to-peak ($\text{min}$). The implementation accepts $v_G$ as the `meal_g_per_min` argument of `HovorkaState::step`/`derivative`, so unannounced-meal scenarios (section 7.2) can be driven through the gut submodel.*

#### D. Blood Glucose Kinetics Submodel
$$\frac{dq_1(t)}{dt} = -\left(S_{ID} r_D(t) + k_{21}\right) q_1(t) + k_{12} q_2(t) - F_{01}^c\left(g_P(t)\right) + EGP(t) + u_A(t) + u_S(t)$$
$$\frac{dq_2(t)}{dt} = k_{21} q_1(t) - k_{12} q_2(t)$$
$$g_P(t) = \frac{q_1(t)}{V_G}$$
*Where $S_{ID}$ is peripheral insulin sensitivity ($/\text{min per mU/L}$), $k_{12}, k_{21}$ are inter-compartmental transfer rates ($/\text{min}$), $F_{01}^c$ is the glucose-dependent non-insulin dependent glucose utilization ($\text{mmol/kg/min}$), $V_G$ is distribution volume ($\text{L/kg}$), and $g_P(t)$ is plasma glucose concentration ($\text{mmol/L}$).*

**Non-insulin dependent glucose utilization**: carried as the published constant $F_{01}$ of Hovorka et al. 2004 and applied through the glucose-dependent Michaelis-Menten form
$$F_{01}^c(g_P) = \frac{F_{01}}{0.85} \cdot \frac{g_P}{g_P + 1}$$
the same form the virtual patient body uses (Wilinska Table 1). At $g_P = 5.67\text{ mmol/L}$ the applied uptake equals the published $F_{01}$; as glucose approaches zero the uptake vanishes, so the model keeps a hepatic floor instead of predicting a collapse under modest above-basal insulin over the four-hour prediction horizon. `s_id` is calibrated so the basal equilibrium of the default configuration rests on the nominal target $5.8\text{ mmol/L}$:
$$q_1 = \frac{EGP_B - F_{01}^c(g_P)}{S_{ID} \cdot BIC} \qquad \text{at } g_P = 5.8\text{ mmol/L}$$

**Endogenous Glucose Production (EGP)**:
$$EGP(t) = EGP_B \cdot \exp\left(-\frac{S_{EGP}\left(r_E(t) - BIC\right)}{0.5} \cdot \ln 2\right)$$
$$BIC = \frac{1000 \cdot BIR}{60 \cdot MCR_I \cdot W}$$
*Where $EGP_B$ is basal EGP ($\text{mmol/kg/min}$), $BIR$ is basal insulin requirement ($\text{U/h}$), and $S_{EGP}$ is the EGP suppression gain of the remote insulin action (action units per $\text{mU/L}$). The suppression mirrors the virtual patient body's model: the EGP action $x_3 = S_{EGP} \cdot r_E$ halves basal EGP every $0.5$ action units above its resting value $S_{EGP} \cdot BIC$ and grows as the remote action drops below it; the low-insulin branch is capped at $3 \cdot EGP_B$ (`EGP_MAX_FOLD_OVER_BASAL` in `src/hovorka.rs`; the cap preserves the basal identity $EGP(BIC) = EGP_B$). With the population gain $S_{EGP} = 0.019$ per $\text{mU/L}$, 50% higher insulin cuts EGP to about 80% of basal, a physiological suppression consistent with the 2004 publication's linear form $EGP_0[1 - x_3]$ ("insulin sensitivity of EGP $520 \times 10^{-4}$ per $\text{mU L}^{-1}$", Table 1). The earlier 0.5 $\text{mU/L}$ per halving scale collapsed EGP to near zero under any sustained above-basal delivery, which froze the four-hour NMPC prediction in closed-loop simulation; it is not the model here.*

#### E. Interstitial Glucose Kinetics Submodel
$$\frac{dq_3(t)}{dt} = k_{31} \left(q_1(t) - q_3(t)\right)$$
$$g_{IG}(t) = \frac{q_3(t)}{V_G}$$
*Where $k_{31}$ is interstitial transfer rate ($/\text{min}$) and $g_{IG}(t)$ is sensor-measured interstitial glucose concentration ($\text{mmol/L}$).*

#### F. Process Noise Dynamics
$$d u_S(t) = d w(t)$$
*Where $w(t)$ is a 1-dimensional Wiener process representing unexplained glucose influx variations.*

---

## 4. State Estimator: Interacting Multiple Model (IMM) & Bayesian Filtering

To handle unpredictable physiological variations (e.g., meals, stress, circadian shifts), the algorithm runs $N$ parallel extended Kalman filters (modes $j = 1 \dots N$), each defined with a different process noise variance $\sigma_{w,j}^2$.

### 4.1 IMM Cycle Execution Steps

1. **Model Interaction Step**:
   Given mode probabilities $\mu_{i,k-1}$ and Markov transition matrix $p_{ji}$:
   $$\mu_{i|j, k-1} = \frac{p_{ji} \mu_{i,k-1}}{c_j}, \quad c_j = \sum_{i=1}^N p_{ji} \mu_{i,k-1}$$
   Mixed state estimate for mode $j$:
   $$x_{0j, k-1|k-1} = \sum_{i=1}^N \mu_{i|j, k-1} x_{i, k-1|k-1}$$
   Mixed covariance $P_{0j, k-1|k-1}$:
   $$P_{0j, k-1|k-1} = \sum_{i=1}^N \mu_{i|j, k-1} \left[ P_{i,k-1|k-1} + (x_{i,k-1|k-1} - x_{0j,k-1|k-1})(x_{i,k-1|k-1} - x_{0j,k-1|k-1})^T \right]$$

   *The equations above follow patent CA2702345C [102-108]. The concrete
   Markov probabilities used by the implementation
   (`IMM_MARKOV_TRANSITION` in `src/imm.rs`) are illustrative tuning
   values with the correct structural property (column-stochastic,
   diagonal-dominant); they are not published algorithm internals and the
   verification only relies on the stochastic structure, not on the
   specific entries.*

2. **Predict Step**:
   $$x_{j, k|k-1} = f(x_{0j, k-1|k-1}, u_k, 0)$$
   $$P_{j, k|k-1} = F_k P_{0j, k-1|k-1} F_k^T + Q_{j,k}$$

3. **Update Step** (upon receiving CGM measurement $z_k = g_{IG,k} + v_k$):
   $$\nu_{j,k} = z_k - H x_{j,k|k-1}$$
   $$S_{j,k} = H P_{j,k|k-1} H^T + R_k$$
   $$K_{j,k} = P_{j,k|k-1} H^T S_{j,k}^{-1}$$
   $$x_{j,k|k} = x_{j,k|k-1} + K_{j,k} \nu_{j,k}$$
   $$P_{j,k|k} = (I - K_{j,k} H) P_{j,k|k-1}$$

4. **Likelihood & Mode Probability Update**:
   $$\Lambda_{j,k} = \frac{1}{\sqrt{2\pi |S_{j,k}|}} \exp\left( -\frac{1}{2} \nu_{j,k}^T S_{j,k}^{-1} \nu_{j,k} \right)$$
   $$\mu_{j,k} = \frac{c_j \Lambda_{j,k}}{\sum_{m=1}^N c_m \Lambda_{m,k}}$$

5. **Combination Step**:
   $$x_{k|k} = \sum_{j=1}^N \mu_{j,k} x_{j,k|k}$$
   $$P_{k|k} = \sum_{j=1}^N \mu_{j,k} \left[ P_{j,k|k} + (x_{j,k|k} - x_{k|k})(x_{j,k|k} - x_{k|k})^T \right]$$

---

## 5. NMPC Dosing Calculator & Safety Control Rules

### 5.1 NMPC Cost Function
The dose calculator optimizes the future insulin infusion sequence over prediction horizon $N_2$ by minimizing the Hovorka et al. 2004 eq. 9 objective with a moving target trajectory:
$$J(u) = \sum_{j=1}^{N_2} \left( g_{IG}(t+j) - w(t+j) \right)^2 + \frac{1}{k_{agr}} \sum_{j=1}^{N_2} \left( \frac{u(t+j) - u(t+j-1)}{K_u} \right)^2$$
*Where $w(t)$ is the moving target trajectory (section 3.3: linear decline at $2\text{ mmol/L/h}$ while more than $2\text{ mmol/L}$ above target, $1\text{ mmol/L/h}$ between that and the target, exponential rise with 15-minute halftime below it, seeded at the measured sensor glucose), $u(t+j-1)$ with $u(t-1) = u_{prev}$ is the rate delivered in the previous control period, and $1/k_{agr}$ weights the effort of changing the rate ($k_{agr}$ is the aggressiveness constant: larger values price rate changes less). The glucose term uses the interstitial/sensor glucose $g_{IG}$ (section 3.2E), not the plasma value.*

*The effort term is normalized by $K_u = 0.5\text{ U/h}$ (`NMPC_EFFORT_UNIT_U_PER_H`), the delivery step that moves this subject's glucose by roughly $1\text{ mmol/L}$ over the 4h horizon. With $K_u = 1$ the objective is eq. 9 as published; the constant is a realization detail that puts the two sums on the same magnitude so $k_{agr}$ truly balances adherence against rate variation. The paper folds the same scaling into the numeric value of $k_{agr}$.*

The implemented solver (`nmpc_sequence` in `src/controller.rs`) rolls the state forward over the horizon at `step_min` resolution under each candidate sequence, sums the two terms sample-by-sample, and selects the first rate $u(t+1)$ of the best sequence (receding horizon). It is seeded by a constant-rate grid search over $k=0..N_2$ candidates $k/N \, u_{max}$ and refined by bounded coordinate descent over the quantized sequence with step $u_{max}/(6 \cdot 5)$ (`NMPC_REFINE_SUBSTEPS`), at most `NMPC_REFINEMENT_PASSES` passes, monotone non-increasing in cost; this stands in for the paper's Marquardt minimization. The engine drives it at `CONTROL_PERIOD_MIN = 15` minute decisions over `CONTROL_HORIZON_MIN = 240` minutes: the horizon long enough for the prediction to see the effect of a candidate rate (insulin action acts over tens of minutes, and a single-sample horizon collapses the grid to the previous rate). The hypoglycemia guard (`is_hypoglycemic`) zeroes delivery outright and resets `u_prev`.

### 5.2 Operating Modes & Safety Boundaries

| Parameter / Feature | Value / Specification | Reference |
|---|---|---|
| **Default Target Glucose** | $5.8\text{ mmol/L}$ ($104\text{ mg/dL}$) | Ware et al. 2022 |
| **User Target Range** | $4.4\text{ to } 11.0\text{ mmol/L}$ ($80\text{ to } 200\text{ mg/dL}$) | Ware et al. 2022 |
| **Ease-off Mode (Exercise)** | Target = $7.0\text{ mmol/L}$; suspend if $g_{IG} < 7.0\text{ mmol/L}$ | Ware et al. 2022 |
| **Boost Mode** | Temporary insulin intensification by $+35\%$ | Ware et al. 2022 |
| **Hard Hypo Cutoff** | Mandatory zero delivery ($u(t) = 0$) if $g_{IG} < 4.4\text{ mmol/L}$ | Safety Invariant |
| **Max Insulin Limit ($u_{max}$)** | Capped at user-defined maximum hourly rate | Safety Invariant |

### 5.3 Closed-Loop Simulation Wiring (`tir-tuner-cli/src/engine.rs`)

The engine is the integration testbed that wires the verified pieces
together; its knobs are not part of the published algorithm.

* The virtual patient is the `population_mean` subject recalibrated to a
  euglycemic resting glucose of $5.8\text{ mmol/L}$
  (`with_basal_glucose`, the Cambridge-simulator convention). The belief
  (`controller_model`) reuses the subject's EGP, volumes, kinetics and
  suppression gain and recalibrates only `s_id` so its basal equilibrium
  sits on the same target, so belief and body share one resting point
  instead of fighting each other.
* Control runs at a 15-minute period over a 240-minute horizon with
  `controller_kagr = 20.0` (the aggressiveness value that keeps below-range
  readings under 5% and holds realistic scenarios in range on both the
  hermetic four-meal day and the real-data day; it carries the tuned
  `k_agr = 5.0` of the un-normalized formulation across the
  `K_u = 0.5` renormalization, since the effort weight
  $1 / (K_u^2 k_{agr})$ is unchanged at $0.2$), a 0.5 mixing gain on
  sensor readings, and `last_rate_u_per_h` feeding the eq. 9 effort term.
* Announced meals deliver 80% of the full ICR bolus
  (`meal_bolus_factor`, default 0.8); the closed loop covers the rest.
  The full 100% bolus stacks with the loop's own correction and pushes
  the reading below the range floor. Unannounced meals get the 100%
  loop correction.

---

## 6. Verification Suite Specification

The suite is split by what each tool is good at. Kani verifies universal
statements over bounded symbolic inputs whose operations are comparisons,
clamps, additions and constant multiplications. The floating-point-level
claims (rounding identities, dense sweeps, wired model behaviour) live in
the native `proptest` / exhaustive suite plus the `fuzz/` targets.

### 6.1 Budget and tooling

* Kani harnesses live in `#[cfg(kani)] mod verification` (`src/verification.rs`).
* The whole Kani suite finishes in about two minutes sequential; the
  cost harness dominates at about 72 seconds and the remaining eight
  proofs finish well under 30 seconds each.
* CI command: `cargo kani -Z unstable-options --harness-timeout 30s -j --output-format terse`.
* Rustdoc includes the Kani-only modules when the same `cfg` flag is passed: `RUSTDOCFLAGS='--cfg kani' cargo doc --no-deps`.
* Native properties run under plain `cargo test` (default budget about 10 seconds); `CAMAPS_SOAK_ITERS=<n>` raises the `proptest` case count for an opt-in soak run.
* Coverage-guided soak: `cargo +nightly fuzz run <target>` over the `fuzz/` crate.

### 6.2 What Kani proves (all bounded symbolic)

| Harness | Property |
|---|---|
| `verify_physiological_non_negativity` | `clamped_forward_euler` is non-negative over a bounded finite range |
| `verify_step_compartment_non_negativity` | `HovorkaState::step` routes every compartment through the saturation primitive |
| `verify_hypo_cutoff_and_dosing_bounds` | dose in `[0, u_max]`, mandatory zero below the hard hypo cutoff |
| `verify_mode_specific_dosing_invariants` | Ease-off suspension, Boost `>=` Standard, `[0, u_max]` per mode |
| `verify_no_floating_point_panics` | EGP submodel is NaN/infinity free and non-negative |
| `verify_nmpc_candidate_rates_in_bounds` | every grid candidate lies in `[0, u_max]` |
| `verify_nmpc_cost_finite_nonneg` | a two-step roll-out slice of the sequence cost is finite and non-negative |
| `verify_nmpc_selection_minimal_cost` | the selected index attains the minimal cost |
| `verify_moving_target_trajectory_decline_bounded` | the trajectory above target declines within the linear band |
| `verify_imm_probability_normalization` | normalized mode probabilities stay non-negative |

### 6.3 What is deliberately not in Kani

CBMC bit-blasts full-`f64` multiply/divide chains with symbolic mantissas
into circuits no current backend discharges in reasonable time; the
historical attempt to quantify the full `f64` domain pushed the suite
past 30 minutes. These claims are covered natively instead:

* IMM sum-to-one, mixing and Bayesian-update distributions: exhaustive divisor-16 lattice plus `proptest` (`src/imm.rs`).
* Full-state wired non-negativity / finiteness of the model: exhaustive compartment slices plus a `proptest` random walk (`src/hovorka.rs`).
* `nmpc_grid_dose` realized-cost composition: `proptest` (`src/controller.rs`).
* The full 240-minute roll-out instance the simulation drives (the Kani cost harness covers a short slice only, because CBMC bit-blasts the symbolic `f64` trajectory into an intractable circuit): `proptest` (`src/controller.rs`).

`nmpc_grid_dose` is deliberately a pure cost minimizer and does not see
the CGM reading: the delivered pump rate must pass through the
hypoglycemia guard (`is_hypoglycemic` / `compute_nmpc_dose_mode`). That
composition is covered natively by the `grid_dose_composed_with_hypo_cutoff_is_safe`
`proptest` (bounds + mandatory zero below the cutoff for arbitrary grid
outcomes); it is not a single Kani harness because the two layers are
proved separately there.

### 6.4 Out of scope

* Full 10-state IMM extended Kalman filter (state/covariance mixing, predict, update, likelihood): not implemented; the crate covers mode-probability bookkeeping only. The simulation re-anchors the belief on the sensor reading instead (the state-estimation layer of the loop).
* The process-noise state `u_S` of section 3.2F is reserved; the deterministic core carries it through unchanged and the stochastic increment is injected externally.

---

## 7. Data Science Scenarios & Simulation Tuning Roadmap

For the Data Science team using this engine to improve the Time in Range (TIR) tuner:

1. **In-Silico Population Benchmarking**:
   * Implement virtual patient cohorts using Wilinska 2010 parameters (varying body weight, $S_{ID}$, $t_{max,I}$, and $EGP_B$).
2. **Stress Scenarios**:
   * **Unannounced Meals**: Test 40g, 70g, and 100g CHO meals without meal bolus to verify IMM adaptation speed.
   * **Exercise Events**: Simulate 45-minute bouts of aerobic exercise by doubling $S_{ID}$ and activating Ease-off mode ($7.0\text{ mmol/L}$ target).
   * **Sensor Noise & Dropouts**: Inject Gaussian noise ($\sigma_Z = 0.5\text{ mmol/L}$) and 30-minute sensor loss periods.
3. **Metric Targets**:
   * **Time in Range ($3.9\text{–}10.0\text{ mmol/L}$)**: Target $> 70\%$. The Boughton et al. (2026) real-world cohort (35,714 users, 19 countries) reaches a median TIR of $69.6\%$; the earlier Alwan et al. (2023) analysis (N = 1,805) reported a mean TIR of $72.6\%$.
   * **Time Below Range ($< 3.9\text{ mmol/L}$)**: Target $< 2.5\%$.
   * **Severe Hypo ($< 3.0\text{ mmol/L}$)**: Target $< 1.0\%$.

---

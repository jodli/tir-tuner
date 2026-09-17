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
$$\frac{dq_1(t)}{dt} = -\left(S_{ID} r_D(t) + k_{21}\right) q_1(t) + k_{12} q_2(t) - F_{01} + EGP(t) + u_A(t) + u_S(t)$$
$$\frac{dq_2(t)}{dt} = k_{21} q_1(t) - k_{12} q_2(t)$$
$$g_P(t) = \frac{q_1(t)}{V_G}$$
*Where $S_{ID}$ is peripheral insulin sensitivity ($/\text{min per mU/L}$), $k_{12}, k_{21}$ are inter-compartmental transfer rates ($/\text{min}$), $F_{01}$ is non-insulin dependent glucose utilization ($\text{mmol/kg/min}$), $V_G$ is distribution volume ($\text{L/kg}$), and $g_P(t)$ is plasma glucose concentration ($\text{mmol/L}$).*

**Endogenous Glucose Production (EGP)**:
$$EGP(t) = EGP_B \cdot \exp\left(-\frac{r_E(t) - BIC}{1/2 \text{ increment}} \cdot \ln 2\right)$$
$$BIC = \frac{1000 \cdot BIR}{60 \cdot MCR_I \cdot W}$$
*Where $EGP_B$ is basal EGP ($\text{mmol/kg/min}$), $BIR$ is basal insulin requirement ($\text{U/h}$), and $BIC$ is basal plasma insulin concentration ($\text{mU/L}$). The exponential form halves basal EGP for every $0.5\text{ mU/L}$ the remote EGP action rises above $BIC$ and grows as the remote action drops below it; since the low-insulin branch grows without bound, the implemented model caps it at $3 \cdot EGP_B$ (`EGP_MAX_FOLD_OVER_BASAL` in `src/hovorka.rs`). The cap preserves the basal identity $EGP(BIC) = EGP_B$ and leaves the verified properties (non-negativity, finiteness, basal steady state) unchanged.*

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
The dose calculator optimizes the vector of future inputs $u^+ = (v, u)^T$ over prediction horizon $N_2$ by minimizing:
$$J(u^+) = J_1 + J_2$$
$$J_1 = \sum_{j=1}^{N_2} \left( g_{IG}(t+j) - w(t+j) \right)^2$$
$$J_2 = \lambda \sum_{j=1}^{N_2} \left( u(t+j) - u_{operating} \right)^2$$
*Where $w(t)$ is the target glucose setpoint trajectory, $u_{operating}$ is the baseline basal rate, and $\lambda$ is the penalty parameter on control action effort. Note the cost uses the interstitial/sensor glucose $g_{IG}$ (section 3.2E), not the plasma value $g_P$: the closed loop is driven by the CGM reading. The realized `nmpc_cost` evaluates this sum by rolling the model forward under a constant candidate rate `u`, summing the squared glucose deviation at every sample over the horizon (`horizon_min`, sampled at `step_min`) plus `lambda` times the summed squared deviation from `u_operating`. The horizon has to be long enough for the prediction to see the effect of the candidate rate: a single-sample horizon leaves predicted glucose identical across the candidate grid (insulin action acts over tens of minutes), so the argument of the minimum collapses to `u_operating` and the loop degenerates to fixed basal. This is the effective upper bound for the realized `lambda` / horizon / step values; the majority weight sits on the glucose term.*

### 5.2 Operating Modes & Safety Boundaries

| Parameter / Feature | Value / Specification | Reference |
|---|---|---|
| **Default Target Glucose** | $5.8\text{ mmol/L}$ ($104\text{ mg/dL}$) | Ware et al. 2022 |
| **User Target Range** | $4.4\text{ to } 11.0\text{ mmol/L}$ ($80\text{ to } 200\text{ mg/dL}$) | Ware et al. 2022 |
| **Ease-off Mode (Exercise)** | Target = $7.0\text{ mmol/L}$; suspend if $g_{IG} < 7.0\text{ mmol/L}$ | Ware et al. 2022 |
| **Boost Mode** | Temporary insulin intensification by $+35\%$ | Ware et al. 2022 |
| **Hard Hypo Cutoff** | Mandatory zero delivery ($u(t) = 0$) if $g_{IG} < 4.4\text{ mmol/L}$ | Safety Invariant |
| **Max Insulin Limit ($u_{max}$)** | Capped at user-defined maximum hourly rate | Safety Invariant |

---

## 6. Verification Suite Specification

The suite is split by what each tool is good at. Kani verifies universal
statements over bounded symbolic inputs whose operations are comparisons,
clamps, additions and constant multiplications. The floating-point-level
claims (rounding identities, dense sweeps, wired model behaviour) live in
the native `proptest` / exhaustive suite plus the `fuzz/` targets.

### 6.1 Budget and tooling

* Kani harnesses live in `#[cfg(kani)] mod verification` (`src/verification.rs`).
* The whole Kani suite must finish within **90 seconds** wall-clock (currently about 30 seconds sequential, about 15 seconds with `--jobs`), with no single harness above a 30 second timeout.
* CI command: `cargo kani -Z unstable-options --harness-timeout 30s -j --output-format terse`.
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
| `verify_nmpc_cost_finite_nonneg` | a short roll-out slice of the NMPC cost is finite and non-negative |
| `verify_nmpc_selection_minimal_cost` | the selected index attains the minimal cost |
| `verify_imm_probability_normalization` | normalized mode probabilities stay non-negative |

### 6.3 What is deliberately not in Kani

CBMC bit-blasts full-`f64` multiply/divide chains with symbolic mantissas
into circuits no current backend discharges in reasonable time; the
historical attempt to quantify the full `f64` domain pushed the suite
past 30 minutes. These claims are covered natively instead:

* IMM sum-to-one, mixing and Bayesian-update distributions: exhaustive divisor-16 lattice plus `proptest` (`src/imm.rs`).
* Full-state wired non-negativity / finiteness of the model: exhaustive compartment slices plus a `proptest` random walk (`src/hovorka.rs`).
* `nmpc_grid_dose` realized-cost composition: `proptest` (`src/controller.rs`).
* The full 60-minute roll-out instance the simulation drives (the Kani cost harness covers a short slice only, because CBMC bit-blasts the symbolic `f64` trajectory into an intractable circuit): `proptest` (`src/controller.rs`).

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

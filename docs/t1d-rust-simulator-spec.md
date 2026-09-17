# Technical Handover Specification: In Silico T1D Digital Body & Sensor Simulator in Rust

**Target Audience:** Autonomous AI Coding Agent / Rust Systems Engineer  
**Project Objective:** Implement a verified, high-performance Type 1 Diabetes (T1D) physiological body model (Hovorka Glucose-Insulin Simulator) and Continuous Glucose Monitor (CGM) sensor error model in Rust. The simulator will serve as an in silico testbed against a real-time Artificial Pancreas (AP) closed-loop controller.

---

## 1. Executive Summary & Architecture Overview

### 1.1 System Architecture
The simulator operates on a discrete-time simulation loop (typically $dt = 1.0\text{ min}$) while interfacing with the AP Controller on a $5\text{--}15\text{ minute}$ control epoch.

```
+-----------------------------------------------------------------------------------+
|                               SIMULATION ENGINE (Rust)                            |
|                                                                                   |
|  +--------------------+    Insulin    +----------------------------------------+  |
|  |   AP Controller    | ------------> | Subcutaneous Insulin PK (S1, S2, I)    |  |
|  |  (Verified Loop)   |               +----------------------------------------+  |
|  +--------------------+                                   |                       |
|            ^                                              v                       |
|            |                          +----------------------------------------+  |
|            |                          | Remote Insulin Actions (x1, x2, x3)    |  |
|            |                          +----------------------------------------+  |
|            |                                              |                       |
|            |                                              v                       |
|      Sensor Reading                   +----------------------------------------+  |
|     (CGM Sampled)                     | Glucose Kinetics ODEs (Q1, Q2, EGP)    |  |
|            |                          +----------------------------------------+  |
|            |                                     ^                 ^              |
|            |                                     | (UG)            | (FR)         |
|            |                    +------------------+     +--------------------+   |
|            |                    | Gut Meal Depot   |     | Renal Clearance    |   |
|            |                    | (D1, D2 / Qgut)  |     | (High BG Guard)    |   |
|            |                    +------------------+     +--------------------+   |
|            |                                     |                                |
|            |                                     v                                |
|            |                          +----------------------------------------+  |
|            |                          | Interstitial Glucose Kinetics (C)      |  |
|            |                          +----------------------------------------+  |
|            |                                              |                       |
|            |                                              v                       |
|            |                          +----------------------------------------+  |
|            +------------------------- | CGM Error Model (AR1 Noise + Bias)     |  |
|                                       +----------------------------------------+  |
+-----------------------------------------------------------------------------------+
```

---

## 2. Physiological Mathematical Formulation

### 2.1 Subcutaneous Insulin Absorption & PK Subsystem
Insulin injected into subcutaneous tissue flows through two depot compartments ($S_1, S_2$) into plasma insulin concentration ($I(t)$):

$$\frac{dS_1(t)}{dt} = u(t) - k_a S_1(t)$$
$$\frac{dS_2(t)}{dt} = k_a S_1(t) - k_a S_2(t)$$
$$\frac{dI(t)}{dt} = \frac{k_a S_2(t)}{V_I \cdot BW} - k_e I(t)$$

*   $u(t)$: Insulin delivery rate ($\text{U/min}$ or $\text{pmol/min}$) combining basal infusion and boluses.
*   $S_1(t), S_2(t)$: Subcutaneous insulin masses ($\text{U}$ or $\text{pmol}$).
*   $I(t)$: Plasma insulin concentration ($\text{mU/L}$ or $\text{pmol/L}$).
*   $k_a$: Subcutaneous absorption rate constant ($\text{min}^{-1}$).
*   $k_e$: Elimination rate constant ($\text{min}^{-1}$).
*   $V_I$: Distribution volume of insulin ($\text{L/kg}$).
*   $BW$: Body weight ($\text{kg}$).

---

### 2.2 Remote Insulin Action Subsystem
Insulin in plasma mediates three distinct physiological actions via delay compartments ($x_1, x_2, x_3$):

$$\frac{dx_1(t)}{dt} = -k_{b1} x_1(t) + k_{a1} I(t) \equiv k_{b1} \left( S_{IT} I(t) - x_1(t) \right)$$
$$\frac{dx_2(t)}{dt} = -k_{b2} x_2(t) + k_{a2} I(t) \equiv k_{b2} \left( S_{ID} I(t) - x_2(t) \right)$$
$$\frac{dx_3(t)}{dt} = -k_{b3} x_3(t) + k_{a3} I(t) \equiv k_{b3} \left( S_{IE} I(t) - x_3(t) \right)$$

*   $x_1(t)$: Effect on glucose transport/distribution ($\text{min}^{-1}$).
*   $x_2(t)$: Effect on glucose disposal ($\text{min}^{-1}$).
*   $x_3(t)$: Effect on suppression of endogenous glucose production (unitless).
*   $S_{IT}, S_{ID}, S_{IE}$: Insulin sensitivities ($\text{min}^{-1}/(\text{mU/L})$, $\text{min}^{-1}/(\text{mU/L})$, $(\text{mU/L})^{-1}$).
*   $k_{b1}, k_{b2}, k_{b3}$: Activation/deactivation rate constants ($\text{min}^{-1}$).

---

### 2.3 Glucose Kinetics & Endogenous Production Subsystem
The accessible (plasma, $Q_1$) and non-accessible (peripheral tissue, $Q_2$) glucose mass kinetics:

$$\frac{dQ_1(t)}{dt} = -F_{01}^c(t) - x_1(t) Q_1(t) + k_{12} Q_2(t) + EGP(t) + U_G(t) - F_R(t)$$
$$\frac{dQ_2(t)}{dt} = x_1(t) Q_1(t) - k_{12} Q_2(t) - x_2(t) Q_2(t)$$

#### Key Terms & Fluxes:
1.  **Plasma Glucose Concentration:**
    $$G(t) = \frac{Q_1(t)}{V_G \cdot BW} \quad (\text{mmol/L})$$
2.  **Non-Insulin-Dependent Glucose Flux ($F_{01}^c$):**
    $$F_{01}^c(t) = \begin{cases} F_{01} & \text{if } G(t) \ge 4.5 \text{ mmol/L} \\ F_{01} \frac{G(t)}{4.5} & \text{if } G(t) < 4.5 \text{ mmol/L} \end{cases}$$
3.  **Endogenous Glucose Production ($EGP$):**
    $$EGP(t) = \max\left(0, EGP_0 \left(1 - x_3(t)\right)\right)$$
4.  **Renal Glucose Clearance ($F_R$):**
    $$F_R(t) = \begin{cases} R_{cl} \cdot (G(t) - R_{thr}) \cdot V_G \cdot BW & \text{if } G(t) > R_{thr} \\ 0 & \text{otherwise} \end{cases}$$
    *   $R_{thr} \approx 9.0 \text{ mmol/L} \ (\sim 180 \text{ mg/dL})$.
    *   $R_{cl} \approx 0.01 \text{ min}^{-1}$.

---

### 2.4 Gut Meal Absorption Subsystem ($U_G$)

#### Phase 1: Hovorka Linear Two-Depot Model
$$\frac{dD_1(t)}{dt} = d(t) - \frac{D_1(t)}{t_{max,G}}$$
$$\frac{dD_2(t)}{dt} = \frac{D_1(t)}{t_{max,G}} - \frac{D_2(t)}{t_{max,G}}$$
$$U_G(t) = \frac{Bio \cdot D_2(t)}{t_{max,G}}$$

*   $d(t)$: Ingested carbohydrate rate ($\text{mmol/min}$).
*   $D_1, D_2$: Carbohydrate masses in gut compartments ($\text{mmol}$).
*   $t_{max,G}$: Time-to-peak gut absorption ($\text{min}$).
*   $Bio$: Bioavailability fraction ($\sim 0.70\text{--}1.20$).

#### Phase 2: Dalla Man Non-Linear Gastric Emptying Subsystem
$$\frac{dQ_{sto1}(t)}{dt} = -k_{gri} Q_{sto1}(t) + d(t)$$
$$\frac{dQ_{sto2}(t)}{dt} = k_{gri} Q_{sto1}(t) - k_{empt}(Q_{sto}) Q_{sto2}(t)$$
$$\frac{dQ_{gut}(t)}{dt} = k_{empt}(Q_{sto}) Q_{sto2}(t) - k_{abs} Q_{gut}(t)$$
$$U_G(t) = \frac{f \cdot k_{abs} \cdot Q_{gut}(t)}{BW}$$
$$k_{empt}(Q_{sto}) = k_{min} + \frac{k_{max} - k_{min}}{2} \left[ \tanh\left(\alpha (Q_{sto} - b \cdot D)\right) - \tanh\left(\beta (Q_{sto} - c \cdot D)\right) + 2 \right]$$

---

### 2.5 Interstitial Glucose Kinetics & CGM Sensor Model

#### 1. Interstitial Glucose Diffusion (1st Order Lag):
$$\frac{dC(t)}{dt} = k_{a\_int} (G(t) - C(t)) \implies \tau = \frac{1}{k_{a\_int}} \quad (\sim 5\text{--}15 \text{ min})$$

#### 2. Stochastic Measurement Noise & Calibration Model:
$$CGM(t) = \text{Calib}(t) \cdot C(t - \tau) + \text{Drift}(t) + v(t)$$
$$v(t) = \alpha_1 v(t-1) + e(t), \quad e(t) \sim \mathcal{N}(0, \sigma_e^2)$$

*   $\alpha_1$: Autoregressive parameter ($\sim 0.7\text{--}0.9$).
*   $\sigma_e$: Noise standard deviation ($\sim 2\text{--}5 \text{ mg/dL}$ or $0.11\text{--}0.28 \text{ mmol/L}$).
*   $\text{Calib}(t)$: Time-varying calibration gain ($1.0 \pm 0.1$).

---

## 3. Parameter Distributions & 18 Synthetic Virtual Subjects

### 3.1 Mean & Standard Deviation Population Statistics (Wilinska 2010)

| Symbol | Parameter Description | Mean Value | Distribution Type | Variability / Notes |
| :--- | :--- | :--- | :--- | :--- |
| **$BW$** | Body weight | $74.9 \text{ kg}$ | Normal ($\text{SD}=14.4$) | Stationary per subject |
| **$V_G$** | Accessible glucose volume | $0.15 \text{ L/kg}$ | Lognormal ($\text{SD}=0.23$) | Stationary |
| **$F_{01}$** | Non-insulin glucose flux | $11.1 \ \mu\text{mol/kg/min}$ | Lognormal | Oscillatory (5% amplitude, 3h period) |
| **$EGP_0$** | Basal EGP at zero insulin | $16.9 \ \mu\text{mol/kg/min}$ | Lognormal | Oscillatory |
| **$k_{12}$** | Inter-compartmental rate | $0.060 \text{ min}^{-1}$ | Lognormal | Oscillatory |
| **$S_{IT}$** | Insulin sensitivity (transport) | $18.41 \times 10^{-4} \text{ min}^{-1}/(\text{mU/L})$ | Lognormal | Oscillatory |
| **$S_{ID}$** | Insulin sensitivity (disposal) | $5.05 \times 10^{-4} \text{ min}^{-1}/(\text{mU/L})$ | Lognormal | Oscillatory |
| **$S_{IE}$** | Sensitivity (EGP suppression) | $0.019 \ (\text{mU/L})^{-1}$ | Lognormal | Oscillatory |
| **$k_{b1}$** | Activation rate (transport) | $0.0034 \text{ min}^{-1}$ | Lognormal | Oscillatory |
| **$k_{b2}$** | Activation rate (disposal) | $0.056 \text{ min}^{-1}$ | Lognormal | Oscillatory |
| **$k_{b3}$** | Activation rate (EGP) | $0.024 \text{ min}^{-1}$ | Lognormal | Oscillatory |
| **$V_I$** | Insulin distribution volume | $0.12 \text{ L/kg}$ | Normal ($\text{SD}=0.012$) | Stationary |
| **$k_a$** | Insulin absorption rate | $0.018 \text{ min}^{-1}$ | Normal ($\text{SD}=0.0045$) | Stationary |
| **$k_e$** | Insulin elimination rate | $0.14 \text{ min}^{-1}$ | Normal ($\text{SD}=0.035$) | Oscillatory |
| **$t_{max,G}$** | Peak gut absorption time | $40.0 \text{ min}$ | Lognormal ($\text{SD}=0.25$) | Stationary |
| **$Bio$** | Carbohydrate bioavailability | $0.90 \ (90\%)$ | Uniform ($0.70\text{--}1.20$) | Inter-occasion variability |
| **$\tau$** | Interstitial delay constant | $10.0 \text{ min}$ | Lognormal | Oscillatory |

---

## 4. Implementation Roadmap (Phase 1 vs. Phase 2)

```
                    +------------------------------------------+
                    |           PHASE 1 (MVP SIMULATOR)        |
                    +------------------------------------------+
                    | 1. Hovorka 2004/2010 Core ODE Engine     |
                    | 2. Hovorka Linear 2-Depot Meal Subsystem |
                    | 3. High BG Renal Clearance Threshold     |
                    | 4. Interstitial Lag + AR(1) CGM Noise    |
                    | 5. Deterministic 18-Subject Instantiation|
                    | 6. RK4 Fixed-Step Solver (dt = 1.0 min)  |
                    +------------------------------------------+
                                         |
                                         v
                    +------------------------------------------+
                    |          PHASE 2 (EXTENDED SUITE)        |
                    +------------------------------------------+
                    | 1. Dalla Man Non-Linear Stomach Emptying |
                    | 2. Non-Gaussian Johnson Noise Transformation|
                    | 3. Sleep Compression Lows & Dropouts     |
                    | 4. Intra-Subject Diurnal Variations      |
                    | 5. 300 UVA/Padova Virtual Cohort Support |
                    +------------------------------------------+
```

---

## 5. Rust Architecture & Code Specifications

### 5.1 Core Data Structures

```rust
/// Units and Conversion Constants
pub const MMOL_TO_MG_DL: f64 = 18.0182;
pub const PMOL_TO_MU: f64 = 1.0 / 6.0; // Insulin unit conversion

/// State vector for Hovorka 2004/2010 Simulator
#[derive(Debug, Clone, Copy)]
pub struct HovorkaState {
    pub s1: f64,      // Subcutaneous insulin depot 1 (U)
    pub s2: f64,      // Subcutaneous insulin depot 2 (U)
    pub i: f64,       // Plasma insulin concentration (mU/L)
    pub x1: f64,      // Remote insulin action on transport (1/min)
    pub x2: f64,      // Remote insulin action on disposal (1/min)
    pub x3: f64,      // Remote insulin action on EGP (dimensionless)
    pub q1: f64,      // Glucose mass in accessible compartment (mmol)
    pub q2: f64,      // Glucose mass in non-accessible compartment (mmol)
    pub d1: f64,      // Meal compartment 1 (mmol)
    pub d2: f64,      // Meal compartment 2 (mmol)
    pub c_int: f64,   // Interstitial glucose concentration (mmol/L)
    pub noise_ar: f64,// Autoregressive noise state (mmol/L)
}

/// Subject-specific physiological parameters
#[derive(Debug, Clone)]
pub struct VirtualSubject {
    pub id: usize,
    pub bw: f64,        // Body weight (kg)
    pub v_g: f64,       // Glucose distribution volume (L/kg)
    pub f01: f64,       // Non-insulin dependent glucose flux (mmol/kg/min)
    pub egp0: f64,      // Basal EGP (mmol/kg/min)
    pub k12: f64,       // Transfer rate Q2 -> Q1 (1/min)
    pub s_it: f64,      // Transport sensitivity (1/min per mU/L)
    pub s_id: f64,      // Disposal sensitivity (1/min per mU/L)
    pub s_ie: f64,      // EGP suppression sensitivity (1/mU/L)
    pub kb1: f64,       // Activation rate 1 (1/min)
    pub kb2: f64,       // Activation rate 2 (1/min)
    pub kb3: f64,       // Activation rate 3 (1/min)
    pub v_i: f64,       // Insulin distribution volume (L/kg)
    pub ka: f64,        // Subcutaneous absorption rate (1/min)
    pub ke: f64,        // Insulin elimination rate (1/min)
    pub t_max_g: f64,   // Peak gut absorption time (min)
    pub bio: f64,       // Carbohydrate bioavailability
    pub k_a_int: f64,   // Interstitial glucose transfer rate (1/min)
    pub r_thr: f64,     // Renal clearance threshold (mmol/L)
    pub r_cl: f64,      // Renal clearance rate (1/min)
}

/// CGM Sensor Configuration & Noise Generator
#[derive(Debug, Clone)]
pub struct CgmSensor {
    pub tau: f64,       // Delay constant (min)
    pub alpha1: f64,    // AR(1) parameter (e.g. 0.85)
    pub sigma_e: f64,   // Noise SD (mmol/L)
    pub calib_gain: f64,// Calibration factor (1.0)
}
```

### 5.2 RK4 Numerical Integration Engine Snippet

```rust
impl HovorkaState {
    /// Compute derivatives dx/dt given parameters, insulin rate, and meal input
    pub fn derivatives(&self, sub: &VirtualSubject, u_insulin: f64, d_meal: f64) -> HovorkaState {
        // Plasma glucose concentration
        let g_conc = self.q1 / (sub.v_g * sub.bw);

        // 1. Subcutaneous Insulin PK
        let ds1 = u_insulin - sub.ka * self.s1;
        let ds2 = sub.ka * self.s1 - sub.ka * self.s2;
        let di = (sub.ka * self.s2) / (sub.v_i * sub.bw) - sub.ke * self.i;

        // 2. Remote Insulin Actions
        let dx1 = -sub.kb1 * self.x1 + sub.kb1 * sub.s_it * self.i;
        let dx2 = -sub.kb2 * self.x2 + sub.kb2 * sub.s_id * self.i;
        let dx3 = -sub.kb3 * self.x3 + sub.kb3 * sub.s_ie * self.i;

        // 3. Gut Absorption Subsystem
        let dd1 = d_meal - self.d1 / sub.t_max_g;
        let dd2 = (self.d1 - self.d2) / sub.t_max_g;
        let u_g = (sub.bio * self.d2) / sub.t_max_g; // mmol/min

        // 4. Fluxes (F01, EGP, Renal)
        let f01_c = if g_conc >= 4.5 {
            sub.f01 * sub.bw
        } else {
            sub.f01 * sub.bw * (g_conc / 4.5)
        };

        let egp = (sub.egp0 * sub.bw * (1.0 - self.x3)).max(0.0);

        let f_r = if g_conc > sub.r_thr {
            sub.r_cl * (g_conc - sub.r_thr) * sub.v_g * sub.bw
        } else {
            0.0
        };

        // 5. Glucose Kinetics ODEs
        let dq1 = -f01_c - self.x1 * self.q1 + sub.k12 * self.q2 + egp + u_g - f_r;
        let dq2 = self.x1 * self.q1 - sub.k12 * self.q2 - self.x2 * self.q2;

        // 6. Interstitial Glucose Diffusion
        let dc_int = sub.k_a_int * (g_conc - self.c_int);

        HovorkaState {
            s1: ds1, s2: ds2, i: di,
            x1: dx1, x2: dx2, x3: dx3,
            q1: dq1, q2: dq2,
            d1: dd1, d2: dd2,
            c_int: dc_int,
            noise_ar: 0.0, // Updated stochastically
        }
    }

    /// Execute 4th-Order Runge-Kutta step
    pub fn rk4_step(&mut self, sub: &VirtualSubject, u_insulin: f64, d_meal: f64, dt: f64) {
        let k1 = self.derivatives(sub, u_insulin, d_meal);
        
        let mut s_k2 = *self;
        s_k2.add_scaled(&k1, dt * 0.5);
        let k2 = s_k2.derivatives(sub, u_insulin, d_meal);

        let mut s_k3 = *self;
        s_k3.add_scaled(&k2, dt * 0.5);
        let k3 = s_k3.derivatives(sub, u_insulin, d_meal);

        let mut s_k4 = *self;
        s_k4.add_scaled(&k3, dt);
        let k4 = s_k4.derivatives(sub, u_insulin, d_meal);

        self.s1 += (dt / 6.0) * (k1.s1 + 2.0*k2.s1 + 2.0*k3.s1 + k4.s1);
        self.s2 += (dt / 6.0) * (k1.s2 + 2.0*k2.s2 + 2.0*k3.s2 + k4.s2);
        self.i  += (dt / 6.0) * (k1.i  + 2.0*k2.i  + 2.0*k3.i  + k4.i);
        self.x1 += (dt / 6.0) * (k1.x1 + 2.0*k2.x1 + 2.0*k3.x1 + k4.x1);
        self.x2 += (dt / 6.0) * (k1.x2 + 2.0*k2.x2 + 2.0*k3.x2 + k4.x2);
        self.x3 += (dt / 6.0) * (k1.x3 + 2.0*k2.x3 + 2.0*k3.x3 + k4.x3);
        self.q1 += (dt / 6.0) * (k1.q1 + 2.0*k2.q1 + 2.0*k3.q1 + k4.q1);
        self.q2 += (dt / 6.0) * (k1.q2 + 2.0*k2.q2 + 2.0*k3.q2 + k4.q2);
        self.d1 += (dt / 6.0) * (k1.d1 + 2.0*k2.d1 + 2.0*k3.d1 + k4.d1);
        self.d2 += (dt / 6.0) * (k1.d2 + 2.0*k2.d2 + 2.0*k3.d2 + k4.d2);
        self.c_int += (dt / 6.0) * (k1.c_int + 2.0*k2.c_int + 2.0*k3.c_int + k4.c_int);
    }

    fn add_scaled(&mut self, rhs: &HovorkaState, factor: f64) {
        self.s1 += rhs.s1 * factor;
        self.s2 += rhs.s2 * factor;
        self.i  += rhs.i  * factor;
        self.x1 += rhs.x1 * factor;
        self.x2 += rhs.x2 * factor;
        self.x3 += rhs.x3 * factor;
        self.q1 += rhs.q1 * factor;
        self.q2 += rhs.q2 * factor;
        self.d1 += rhs.d1 * factor;
        self.d2 += rhs.d2 * factor;
        self.c_int += rhs.c_int * factor;
    }
}
```

---

## 6. Verification & Test Suite Requirements

1.  **Mass Balance Test:** Verify that in the absence of glucose utilization ($F_{01}=0, x_1=x_2=0$) and renal clearance, ingested glucose $D$ is strictly conserved in plasma/tissue $Q_1 + Q_2$.
2.  **Euglycemic Clamp Verification:** Input a constant baseline insulin infusion $u_b = \frac{EGP_0 \cdot BW}{S_{IE}}$ and confirm that blood glucose stabilizes to $100 \text{ mg/dL} \ (5.55 \text{ mmol/L})$.
3.  **Meal Response Curve Check:** Inject an $80\text{g}$ carbohydrate meal over $15\text{ min}$ and verify postprandial glucose peak ($160\text{--}220 \text{ mg/dL}$) occurs between $60\text{--}90\text{ minutes}$.
4.  **CGM Noise Autocorrelation Check:** Confirm the empirical autocorrelation function $\text{ACF}(k)$ of the simulated noise process $v(t)$ matches $\alpha_1^k \approx 0.85^k$.

---
*Specification Document Generated for Autonomous Agent Handover.*

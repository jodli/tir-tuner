#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use camaps_fx::hovorka::{HovorkaParams, HovorkaState};

#[derive(Arbitrary, Debug)]
struct Input {
    i1: f64,
    i2: f64,
    r_d: f64,
    r_e: f64,
    a1: f64,
    a2: f64,
    q1: f64,
    q2: f64,
    q3: f64,
    u_s: f64,
    u_basal: f64,
    u_bolus: f64,
    meal: f64,
    dt: f64,
}

fn bounded(value: f64, lo: f64, hi: f64) -> Option<f64> {
    value.is_finite().then(|| value.clamp(lo, hi))
}

fuzz_target!(|input: Input| {
    let (
        Some(i1),
        Some(i2),
        Some(r_d),
        Some(r_e),
        Some(a1),
        Some(a2),
        Some(q1),
        Some(q2),
        Some(q3),
        Some(u_s),
        Some(u_basal),
        Some(u_bolus),
        Some(meal),
        Some(dt),
    ) = (
        bounded(input.i1, 0.0, 25_000.0),
        bounded(input.i2, 0.0, 25_000.0),
        bounded(input.r_d, 0.0, 50.0),
        bounded(input.r_e, 0.0, 50.0),
        bounded(input.a1, 0.0, 150.0),
        bounded(input.a2, 0.0, 150.0),
        bounded(input.q1, 0.0, 30.0),
        bounded(input.q2, 0.0, 30.0),
        bounded(input.q3, 0.0, 30.0),
        bounded(input.u_s, 0.0, 5.0),
        bounded(input.u_basal, 0.0, 25.0),
        bounded(input.u_bolus, 0.0, 5.0),
        bounded(input.meal, 0.0, 5.0),
        bounded(input.dt, 0.5, 3.0),
    ) else {
        return;
    };

    let params = HovorkaParams::default();
    let state = HovorkaState {
        i1,
        i2,
        r_d,
        r_e,
        a1,
        a2,
        q1,
        q2,
        q3,
        u_s,
    };

    let next = state.step(&params, u_basal, u_bolus, meal, dt);

    for (name, value) in [
        ("i1", next.i1),
        ("i2", next.i2),
        ("r_d", next.r_d),
        ("r_e", next.r_e),
        ("a1", next.a1),
        ("a2", next.a2),
        ("q1", next.q1),
        ("q2", next.q2),
        ("q3", next.q3),
    ] {
        assert!(value.is_finite(), "{name} is not finite: {value}");
        assert!(value >= 0.0, "{name} became negative: {value}");
    }
});

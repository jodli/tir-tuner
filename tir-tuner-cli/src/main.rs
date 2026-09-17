fn main() {
    println!("tir-tuner-cli {}", env!("CARGO_PKG_VERSION"));
    // Future entry points:
    // - load CGM data logged by the real algorithm
    // - replay it through the controller core with tuned parameters
    // - serve a web UI for live parameter sweeps, and tune time-in-range
}
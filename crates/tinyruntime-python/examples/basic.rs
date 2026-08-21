//! What this provider knows about Python, printed.
//!
//! Every answer here is one the router would otherwise have to hard-code. None
//! of it downloads or installs anything — that is the router's half.

use tinyruntime_python::{
    DEFAULT_VERSION, RuntimeSettings, distribution, harness, parse_version, satisfies,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("default version floor: {DEFAULT_VERSION}");

    match distribution::host_suffix() {
        Ok(suffix) => println!("this host installs: cpython-*-{suffix}"),
        Err(error) => println!("this host cannot install a managed build: {error}"),
    }

    // A request names a floor, so anything newer satisfies it.
    for candidate in ["3.11.9", "3.12.4", "3.13.1"] {
        let parsed = parse_version(candidate).expect("a version");
        let verdict = if satisfies(parsed, DEFAULT_VERSION, None) {
            "reused"
        } else {
            "rejected"
        };
        println!("  a host {candidate} would be {verdict}");
    }

    match tinyruntime_python::system::detect(&RuntimeSettings::new(DEFAULT_VERSION)).await {
        Some(layout) => println!(
            "found a host interpreter: {} at {}",
            layout.version, layout.bin_dir
        ),
        None => println!("no compatible host interpreter; the router would install one"),
    }

    let harness = harness();
    println!(
        "warm worker: {} ({} bytes) under `{}` with {:?}",
        harness.filename,
        harness.source.len(),
        harness.executable,
        harness.args_before_script
    );
    Ok(())
}

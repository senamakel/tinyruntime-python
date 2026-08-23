//! The warm-worker harness this provider ships to the router.
//!
//! The script is compiled into the module and handed over on request rather than
//! installed anywhere, so an upgraded provider ships an upgraded harness and
//! there is no version of it on disk that can drift.
//!
//! One flag and one variable travel with it. `-u` makes the interpreter's own
//! streams unbuffered, and `PYTHONUNBUFFERED` covers what the flag does not, so a
//! job's output reaches the capture files promptly instead of sitting in a buffer
//! until the process exits.

use tinyruntime_bus::WorkerHarness;

/// The harness source, compiled in.
const SOURCE: &str = include_str!("pool_worker.py");

/// The filename the router writes the harness under.
const FILENAME: &str = "pool_worker.py";

/// The harness for this provider's warm workers.
#[must_use]
pub fn harness() -> WorkerHarness {
    WorkerHarness::new(FILENAME, SOURCE, "python")
        .with_flag("-u")
        .with_env("PYTHONUNBUFFERED", "1")
}

#[cfg(test)]
mod test;

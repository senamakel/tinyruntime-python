//! The Python runtime provider for tinyruntime.
//!
//! # What this crate is
//!
//! One half of a deliberate split. `tinyruntime` — the router — owns everything
//! that is the same for every language: downloading an archive, verifying its
//! digest, unpacking it, promoting it into a cache atomically, reusing it on the
//! next start, and keeping a bounded set of warm interpreter processes in front
//! of it. This crate owns everything that is true only of Python:
//!
//! - [`version`] — that a Python request names a *floor* rather than an exact
//!   version, because that is what its distribution channel actually supports.
//! - [`system`] — how to find a host interpreter, and why `python3.12` is tried
//!   before `python3`.
//! - [`distribution`] — how to search a moving release index for the newest build
//!   inside the requested range, and why a stripped build wins a tie.
//! - [`layout`] — that a standalone build hides its interpreter under `python/`,
//!   whatever version it is.
//! - [`harness`](mod@harness) — what a warm Python worker is, and the two things it cannot
//!   promise that the Node one can.
//!
//! It downloads nothing, installs nothing, and starts no worker. Every answer it
//! gives is a description the router acts on.
//!
//! # A note on isolation
//!
//! Python cannot isolate a pooled job the way JavaScript can: there is no worker
//! thread to run it in and no safe way to kill one. Jobs on a warm Python worker
//! share module state, `os.environ`, and logging configuration. The harness
//! gives each job fresh globals and the router recycles workers after a job
//! budget, which bounds the leakage without eliminating it — which is why a host
//! opts into Python pooling rather than getting it by default.
//!
//! # Using it
//!
//! Load it alongside `tinyruntime`, which routes `python` to the well-known name
//! this module claims. A host then asks the router to run Python and never
//! addresses this module directly.
//!
//! ```
//! use tinyruntime_python::{DEFAULT_VERSION, parse_version, satisfies};
//!
//! // A request names a floor, so a newer interpreter satisfies it.
//! let installed = parse_version("Python 3.13.1").expect("a version");
//! assert!(satisfies(installed, DEFAULT_VERSION, None));
//! assert!(!satisfies(installed, DEFAULT_VERSION, Some("3.13")));
//! ```

pub mod distribution;
pub mod error;
pub mod harness;
pub mod layout;
pub mod system;
pub mod version;

mod tinybus_module;

/// Entry points for hosts that link this module into their executable.
#[cfg(feature = "static-link")]
pub mod linked {
    pub use crate::tinybus_module::exports::{
        TINYBUS_MODULE_ABI_V1, tinybus_module_init_v1, tinybus_module_manifest_v1,
    };
}

pub use error::{Error, Result};
pub use harness::harness;
pub use tinybus_module::DEFAULT_VERSION;
pub use version::{Version, parse_version, satisfies};

/// Whether this provider's contract version can bind to the one it was built
/// against.
///
/// Always true for a build whose vendored contract matches, and the check the
/// router makes before routing anything here. Exposed so a host can assert it
/// without reconstructing the comparison.
#[must_use]
pub fn is_compatible_with_contract() -> bool {
    tinyruntime_bus::is_compatible(CONTRACT_VERSION)
}

// The wire contract, re-exported whole, so a consumer of this crate names the
// very types the module serves rather than copies of them.
pub use tinyruntime_bus::{
    ArchiveFormat, CONTRACT_VERSION, Distribution, Language, LayoutRequest, LayoutResponse,
    PROVIDER_INTERFACE, PROVIDER_METHODS, PYTHON, ProviderDescriptor, RuntimeLayout,
    RuntimeSettings, WORKER_PROTOCOL_VERSION, WorkerHarness, names, object_path_for,
};

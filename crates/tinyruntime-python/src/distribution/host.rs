//! Which standalone Python build this machine takes.
//!
//! A plain table of host triples, because it is only correct by matching what
//! the channel actually publishes. A machine missing from it is one with no
//! standalone build, and saying so is better than guessing a filename.

use crate::error::{Error, Result};

/// The asset suffix for the machine this is running on.
///
/// # Errors
///
/// Returns [`Error::UnsupportedHost`] when the channel publishes nothing for
/// this operating system and architecture.
pub fn host_suffix() -> Result<&'static str> {
    suffix_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// The asset suffix for a named operating system and architecture.
///
/// Split from [`host_suffix`] so the table can be tested for every platform
/// rather than only for whichever one the tests happen to run on.
///
/// # Errors
///
/// Returns [`Error::UnsupportedHost`] for a combination the channel does not
/// publish.
pub fn suffix_for(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin-install_only.tar.gz"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin-install_only.tar.gz"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu-install_only.tar.gz"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu-install_only.tar.gz"),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc-install_only.tar.gz"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc-install_only.tar.gz"),
        _ => Err(Error::UnsupportedHost {
            os: os.to_owned(),
            arch: arch.to_owned(),
        }),
    }
}

//! The crate-wide error type and result alias.
//!
//! A provider's failures are narrow by design: it answers questions and does not
//! install anything, so the things that can go wrong are a host it cannot serve,
//! a version bound that is not a version, and a release index it could not read
//! or that offered nothing suitable.
//!
//! Messages are lowercase and carry no credential, payload, or absolute path.
//! They travel to the router, which puts them in front of a person.

/// Everything this provider can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The standalone Python channel publishes no build for this machine.
    #[error("no standalone python build is published for {os}/{arch}")]
    UnsupportedHost {
        /// The operating system, as Rust names it.
        os: String,
        /// The architecture, as Rust names it.
        arch: String,
    },

    /// A configured version bound is not a version.
    #[error("`{value}` is not a python version ({bound})")]
    InvalidVersion {
        /// What was configured.
        value: String,
        /// Which bound it was configured as.
        bound: &'static str,
    },

    /// The release index could not be read.
    #[error("the standalone python release index could not be read: {0}")]
    IndexUnavailable(String),

    /// The release index was readable but held nothing this host can use.
    ///
    /// Distinct from [`Error::IndexUnavailable`] because it calls for a different
    /// response: the index is fine, and the version bounds are what excluded
    /// everything in it.
    #[error("the standalone python release `{release}` publishes no build matching {bounds}")]
    NoMatchingBuild {
        /// The release that was searched.
        release: String,
        /// The bounds that excluded everything, rendered for display.
        bounds: String,
    },
}

/// The crate's result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;

//! The shape of the release index, and which asset in it to install.
//!
//! Kept apart from the network call so selection can be tested against a real
//! index body rather than against a live channel. That matters more here than it
//! would elsewhere: selection is the part with actual judgement in it — version
//! bounds, host matching, and a preference between two builds of the same
//! version — and none of that should need a network to exercise.

use serde::Deserialize;

use tinyruntime_bus::{ArchiveFormat, Distribution};

use crate::error::{Error, Result};
use crate::version::{Version, parse_version, satisfies};

/// One release of the standalone Python channel.
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    /// The release's tag, which is a datestamp for this channel.
    pub tag_name: String,
    /// Everything published under it.
    pub assets: Vec<Asset>,
}

/// One published file in a release.
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    /// The filename, which is where the version and host triple live.
    pub name: String,
    /// Where to fetch it.
    pub browser_download_url: String,
    /// The digest, when the channel published one, as `sha256:<hex>`.
    #[serde(default)]
    pub digest: Option<String>,
}

/// A candidate build, parsed out of an asset name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    version: Version,
    asset_name: String,
    url: String,
    sha256: Option<String>,
    stripped: bool,
}

/// Pick the build to install from `release`.
///
/// Preference order, applied in turn: newest version first, then a stripped
/// build over a full one. Stripped builds omit debug symbols and static
/// libraries — a few hundred megabytes of things nothing here uses — so
/// preferring them is a large saving for no loss.
///
/// # Errors
///
/// Returns [`Error::InvalidVersion`] when a bound is not a version,
/// [`Error::UnsupportedHost`] when the channel publishes nothing for this
/// machine, and [`Error::NoMatchingBuild`] when the bounds excluded everything.
pub fn select(
    release: &Release,
    minimum: &str,
    maximum: Option<&str>,
    suffix: &str,
) -> Result<Distribution> {
    if parse_version(minimum).is_none() {
        return Err(Error::InvalidVersion {
            value: minimum.to_owned(),
            bound: "minimum",
        });
    }
    if let Some(maximum) = maximum.map(str::trim).filter(|value| !value.is_empty())
        && parse_version(maximum).is_none()
    {
        return Err(Error::InvalidVersion {
            value: maximum.to_owned(),
            bound: "maximum",
        });
    }

    let mut candidates: Vec<Candidate> = release
        .assets
        .iter()
        .filter_map(|asset| candidate(asset, suffix))
        .filter(|candidate| satisfies(candidate.version, minimum, maximum))
        .collect();

    if candidates.is_empty() {
        return Err(Error::NoMatchingBuild {
            release: release.tag_name.clone(),
            bounds: render_bounds(minimum, maximum),
        });
    }

    // Newest first; a stripped build wins a tie; the name breaks any remaining
    // tie so the choice is deterministic rather than index-order.
    candidates.sort_by(|left, right| {
        right
            .version
            .cmp(&left.version)
            .then_with(|| right.stripped.cmp(&left.stripped))
            .then_with(|| left.asset_name.cmp(&right.asset_name))
    });

    let chosen = candidates.swap_remove(0);
    let mut distribution = Distribution::new(
        chosen.version.to_string(),
        &chosen.asset_name,
        chosen.url,
        ArchiveFormat::TarGz,
    )
    // The archive expands into a plain `python/` directory, identical across
    // every build, so the install directory has to be named from the asset
    // rather than from what is inside it — otherwise every version would want
    // the same directory.
    .with_install_dir_name(install_dir_name(&chosen.asset_name));

    if let Some(digest) = chosen.sha256 {
        distribution = distribution.with_sha256(digest);
    }
    Ok(distribution)
}

/// Parse one asset into a candidate, or skip it.
///
/// Only `install_only` archives are considered. The channel also publishes full
/// build artifacts with debug information and a build manifest, which are large
/// and are not a runnable interpreter tree.
fn candidate(asset: &Asset, suffix: &str) -> Option<Candidate> {
    let name = asset.name.as_str();
    if !name.starts_with("cpython-") || !name.ends_with(".tar.gz") || !name.contains("install_only")
    {
        return None;
    }
    if !matches_host(name, suffix) {
        return None;
    }

    // `cpython-3.12.4+20240909-x86_64-unknown-linux-gnu-install_only.tar.gz`:
    // the version runs from after the prefix to the `+` that starts the
    // channel's own build stamp.
    let version = parse_version(name.strip_prefix("cpython-")?.split('+').next()?)?;

    Some(Candidate {
        version,
        asset_name: asset.name.clone(),
        url: asset.browser_download_url.clone(),
        sha256: asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .map(str::to_owned),
        stripped: name.contains("install_only_stripped"),
    })
}

/// Whether an asset targets this host.
///
/// Both spellings of the suffix count: the channel publishes a full
/// `-install_only.tar.gz` and a smaller `-install_only_stripped.tar.gz` for the
/// same triple, and both are usable.
fn matches_host(asset_name: &str, suffix: &str) -> bool {
    asset_name.ends_with(suffix)
        || asset_name
            .ends_with(&suffix.replace("-install_only.tar.gz", "-install_only_stripped.tar.gz"))
}

/// The directory name a build installs into, derived from its asset name.
fn install_dir_name(asset_name: &str) -> String {
    asset_name
        .strip_suffix(".tar.gz")
        .unwrap_or(asset_name)
        .to_owned()
}

/// Render the version bounds for an error a person reads.
fn render_bounds(minimum: &str, maximum: Option<&str>) -> String {
    match maximum.map(str::trim).filter(|value| !value.is_empty()) {
        Some(maximum) => format!(">= {minimum} and < {maximum}"),
        None => format!(">= {minimum}"),
    }
}

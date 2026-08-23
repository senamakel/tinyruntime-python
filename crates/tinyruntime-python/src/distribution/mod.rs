//! Choosing which standalone Python build to install.
//!
//! The channel is `astral-sh/python-build-standalone`, which publishes a set of
//! relocatable `CPython` builds per release rather than one archive per version.
//! Two consequences shape this module.
//!
//! First, selection is a search rather than a lookup: the index has to be read,
//! filtered to this host, filtered to the requested version range, and then
//! ranked. That is what the private `index` submodule does, and it is deliberately separable from the
//! network so it can be tested against a real index body.
//!
//! Second, every build unpacks into a directory called `python`, regardless of
//! version. The install directory is therefore named from the asset rather than
//! from the archive's contents — otherwise every version would want the same
//! directory in the cache and each install would silently replace the last.

use reqwest::Client;

use tinyruntime_bus::{Distribution, RuntimeSettings};

use crate::error::{Error, Result};

mod host;
mod index;

pub use host::{host_suffix, suffix_for};
pub use index::{Asset, Release, select as select_from};

/// Where the standalone Python builds are published.
const RELEASES_API: &str =
    "https://api.github.com/repos/astral-sh/python-build-standalone/releases";

/// Pick the build to install for this host under `settings`.
///
/// # Errors
///
/// Returns [`Error::IndexUnavailable`] when the release index cannot be read,
/// and the selection errors from [`select_from`] otherwise.
pub async fn select(client: &Client, settings: &RuntimeSettings) -> Result<Distribution> {
    select_from_api(client, RELEASES_API, settings).await
}

/// [`select`] against a named release index.
///
/// Split out so the request, the tag handling, and the failure mapping can be
/// tested against a server the test controls. Reaching GitHub from a unit test
/// would tie the suite to the network and to a release staying published, which
/// the repository's testing rules rule out.
///
/// # Errors
///
/// As [`select`].
pub async fn select_from_api(
    client: &Client,
    releases_api: &str,
    settings: &RuntimeSettings,
) -> Result<Distribution> {
    let suffix = host_suffix()?;
    let release = fetch_release(client, releases_api, settings.release_tag()).await?;

    let distribution = index::select(
        &release,
        &settings.version,
        settings.maximum_version(),
        suffix,
    )?;

    tracing::info!(
        release = %release.tag_name,
        version = %distribution.version,
        "[tinyruntime-python] selected a standalone build for this host"
    );
    Ok(distribution)
}

/// Read one release from the channel, or its current one.
async fn fetch_release(
    client: &Client,
    releases_api: &str,
    tag: Option<&str>,
) -> Result<index::Release> {
    let url = match tag {
        Some(tag) => format!("{releases_api}/tags/{tag}"),
        None => format!("{releases_api}/latest"),
    };

    client
        .get(&url)
        .header(
            reqwest::header::USER_AGENT,
            concat!("tinyruntime-python/", env!("CARGO_PKG_VERSION")),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| Error::IndexUnavailable(describe(&error)))?
        .json::<index::Release>()
        .await
        .map_err(|error| Error::IndexUnavailable(describe(&error)))
}

/// Describe a request failure without putting the URL in a host-visible message.
fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "the request timed out".to_owned()
    } else if error.is_connect() {
        "the connection could not be established".to_owned()
    } else if let Some(status) = error.status() {
        format!("the channel answered with status {status}")
    } else if error.is_decode() {
        "the index was not in the expected shape".to_owned()
    } else {
        "the request failed".to_owned()
    }
}

#[cfg(test)]
mod test;

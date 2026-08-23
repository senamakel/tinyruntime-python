//! What counts as a compatible Python version.
//!
//! Unlike Node.js, where a request names one major line, a Python request names
//! a **floor**: `3.12` means "3.12 or newer". That difference is not a style
//! choice — it follows from what the two distribution channels publish. Node.js
//! ships one archive per exact version, so asking for one is natural. The
//! standalone Python channel publishes a moving set of builds, and pinning an
//! exact patch would break the moment that build rotated out.
//!
//! A caller that needs to stay off a newer series sets an exclusive upper bound.
//! That is what keeps selection away from, say, a 3.15 pre-release sitting in the
//! same index as the 3.12 builds it actually wants.

use std::fmt;

/// A parsed Python version.
///
/// Ordered by major, then minor, then patch, which is what makes the floor and
/// ceiling comparisons a plain `<` and `>=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    /// The major component.
    pub major: u32,
    /// The minor component.
    pub minor: u32,
    /// The patch component, `0` when the spelling omits it.
    pub patch: u32,
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parse a version from any spelling that turns up in practice.
///
/// Handles what `python --version` prints (`Python 3.12.4`), a bare version
/// (`3.12.4`), a series with no patch (`3.12`), and a patch carrying a suffix
/// (`3.13.0rc1`, which parses as `3.13.0` — a release candidate is that series,
/// and treating it as unparseable would silently drop it from selection).
///
/// # Examples
///
/// ```
/// # use tinyruntime_python::parse_version;
/// assert_eq!(parse_version("Python 3.12.4").map(|v| v.to_string()).as_deref(), Some("3.12.4"));
/// assert_eq!(parse_version("3.12").map(|v| v.to_string()).as_deref(), Some("3.12.0"));
/// assert_eq!(parse_version("latest"), None);
/// ```
#[must_use]
pub fn parse_version(raw: &str) -> Option<Version> {
    let trimmed = raw.trim();
    let stripped = trimmed.strip_prefix("Python ").unwrap_or(trimmed).trim();

    let mut parts = stripped.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts
        .next()
        .and_then(|segment| {
            let digits: String = segment.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<u32>().ok()
        })
        .unwrap_or(0);

    Some(Version {
        major,
        minor,
        patch,
    })
}

/// Whether `candidate` sits within `[minimum, maximum)`.
///
/// The upper bound is exclusive so a ceiling of `3.15` means "anything in 3.14
/// and below", which is how a person reading the configuration would read it.
/// An absent or blank ceiling means unbounded.
#[must_use]
pub fn satisfies(candidate: Version, minimum: &str, maximum: Option<&str>) -> bool {
    let Some(minimum) = parse_version(minimum) else {
        // A floor that is not a version is a configuration error. Accepting
        // anything would quietly install whatever the channel offered first.
        return false;
    };
    if candidate < minimum {
        return false;
    }
    match maximum.map(str::trim).filter(|value| !value.is_empty()) {
        Some(maximum) => parse_version(maximum).is_some_and(|maximum| candidate < maximum),
        None => true,
    }
}

#[cfg(test)]
mod test;

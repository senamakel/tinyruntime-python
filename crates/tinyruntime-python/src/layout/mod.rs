//! Where a Python install keeps its executables.
//!
//! Two shapes again, but with a wrinkle Node.js does not have: the interpreter's
//! filename varies with the version. A standalone build ships `bin/python3.12`
//! and usually symlinks `bin/python3` and `bin/python` to it, but which of those
//! exist is not guaranteed, so the search tries the specific name first and falls
//! back.
//!
//! * Unix: `<install>/python/bin/{python3.N,python3,python}`.
//! * Windows: `<install>/python/{python.exe}`, with no `bin` directory.
//!
//! The `python/` component is the standalone channel's own doing: every build,
//! of every version, unpacks into a directory with that name. It is also why the
//! install directory in the cache is named from the asset rather than from what
//! is inside the archive.

use std::path::{Path, PathBuf};

use tinyruntime_bus::{RuntimeLayout, RuntimeSettings};

use crate::version;

/// The logical executables this provider reports.
pub const TOOLS: &[&str] = &["python", "pip"];

/// Describe the interpreter in `install_dir`, if there is a usable one.
///
/// Returns `None` when no interpreter is found or when the one found is outside
/// the requested version range. Both are ordinary answers: the router is scanning
/// a cache that may hold several versions and several kinds of leftover.
pub async fn describe(install_dir: &Path, settings: &RuntimeSettings) -> Option<RuntimeLayout> {
    let binary = find_interpreter(install_dir)?;
    let reported = crate::system::probe_version(&binary).await?;
    let parsed = version::parse_version(&reported)?;

    if !version::satisfies(parsed, &settings.version, settings.maximum_version()) {
        tracing::debug!(
            reported = %parsed,
            "[tinyruntime-python] a cached install is outside the requested range"
        );
        return None;
    }

    let bin_dir = binary.parent()?;
    Some(from_parts(bin_dir, &binary, &parsed.to_string()))
}

/// Build a layout from an interpreter and the directory holding it.
///
/// Only tools that are actually present are recorded. An install without `pip`
/// is usable, and claiming a path that is not there would turn a clear "this
/// install has no pip" into a spawn failure much later.
#[must_use]
pub fn from_parts(bin_dir: &Path, interpreter: &Path, version: &str) -> RuntimeLayout {
    let mut layout = RuntimeLayout::new(version, bin_dir.to_string_lossy().into_owned())
        .with_executable("python", interpreter.to_string_lossy().into_owned());

    for name in pip_names() {
        let candidate = bin_dir.join(name);
        if candidate.is_file() {
            layout = layout.with_executable("pip", candidate.to_string_lossy().into_owned());
            break;
        }
    }
    layout
}

/// Find the interpreter inside an unpacked standalone build.
///
/// Tries the conventional locations rather than walking the tree: a standalone
/// build is a known shape, and a walk would happily find an interpreter bundled
/// inside some package's test fixtures.
#[must_use]
pub fn find_interpreter(install_dir: &Path) -> Option<PathBuf> {
    for root in [install_dir.join("python"), install_dir.to_path_buf()] {
        let bin_dir = if cfg!(windows) {
            root.clone()
        } else {
            root.join("bin")
        };
        for name in interpreter_names() {
            let candidate = bin_dir.join(&name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Interpreter filenames to try, most specific first.
///
/// The versioned names come first because a build may ship `python3.12` without
/// the generic symlinks, and because on a host install `python` may well be a
/// Python 2 left over from a previous decade.
fn interpreter_names() -> Vec<String> {
    let mut names = Vec::new();
    if cfg!(windows) {
        names.push("python.exe".to_owned());
        return names;
    }
    // A standalone build's minor version is not known here, so the generic
    // names do the work and the version is confirmed by probing.
    for name in ["python3", "python"] {
        names.push(name.to_owned());
    }
    names
}

/// Package-installer filenames to try.
fn pip_names() -> Vec<String> {
    if cfg!(windows) {
        vec!["pip.exe".to_owned(), "pip3.exe".to_owned()]
    } else {
        vec!["pip3".to_owned(), "pip".to_owned()]
    }
}

#[cfg(test)]
mod test;

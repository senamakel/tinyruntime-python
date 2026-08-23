//! Finding a Python interpreter the host already has.
//!
//! Cheap, and worth trying first: most machines have a usable `python3`, and one
//! `--version` probe is the difference between using it and downloading a
//! standalone build to sit beside it.
//!
//! The candidate order is the interesting part. A caller's preferred command
//! first, then the exact series the floor names (`python3.12`), then the generic
//! `python3`, then bare `python`. Trying the series-specific name before the
//! generic one matters on a machine with several interpreters installed: `python3`
//! is whatever the distribution decided, and it is often older than the versioned
//! binary sitting right next to it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tinyruntime_bus::{RuntimeLayout, RuntimeSettings};

use crate::layout;
use crate::version::{self, Version};

/// How long a `--version` probe may take before it is abandoned.
///
/// The probe runs an interpreter, which can hang on a network filesystem or
/// behind an antivirus scanner. A probe that does not answer is treated as no
/// interpreter rather than waited on.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Look for a host interpreter satisfying `settings`.
///
/// Returns `None` when nothing suitable is installed, which is the signal for
/// the router to install a managed build instead.
pub async fn detect(settings: &RuntimeSettings) -> Option<RuntimeLayout> {
    detect_in(settings, std::env::var_os("PATH").as_ref()).await
}

/// [`detect`] with the search path supplied explicitly.
///
/// Split out so a test can point the probe at a directory it controls. Rewriting
/// the process environment is not an option — `unsafe` is forbidden
/// workspace-wide, and a shared `PATH` would make concurrent tests interfere —
/// and the candidate ordering here is the whole point of the module.
pub async fn detect_in(
    settings: &RuntimeSettings,
    path_var: Option<&std::ffi::OsString>,
) -> Option<RuntimeLayout> {
    let Some(minimum) = version::parse_version(&settings.version) else {
        tracing::warn!(
            "[tinyruntime-python] the minimum version is not a version; skipping host detection"
        );
        return None;
    };

    for candidate in candidates(settings.preferred_command(), minimum) {
        let Some(path) = locate(&candidate, path_var) else {
            continue;
        };
        let Some(reported) = probe_version(&path).await else {
            tracing::debug!("[tinyruntime-python] a candidate did not answer `--version`");
            continue;
        };
        let Some(parsed) = version::parse_version(&reported) else {
            continue;
        };
        if !version::satisfies(parsed, &settings.version, settings.maximum_version()) {
            tracing::debug!(
                reported = %parsed,
                "[tinyruntime-python] a host interpreter is outside the requested range"
            );
            continue;
        }

        let Some(bin_dir) = path.parent() else {
            continue;
        };
        tracing::info!(
            reported = %parsed,
            "[tinyruntime-python] reusing a compatible host interpreter"
        );
        return Some(layout::from_parts(bin_dir, &path, &parsed.to_string()));
    }
    None
}

/// The commands to try, in order.
///
/// The series-specific name comes before the generic one: on a machine with
/// several interpreters, `python3` is whatever the distribution chose and is
/// often older than the `python3.12` sitting beside it.
fn candidates(preferred: Option<&str>, minimum: Version) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(preferred) = preferred {
        candidates.push(preferred.to_owned());
    }
    for fallback in [
        format!("python{}.{}", minimum.major, minimum.minor),
        "python3".to_owned(),
        "python".to_owned(),
    ] {
        if !candidates.contains(&fallback) {
            candidates.push(fallback);
        }
    }
    candidates
}

/// Resolve a command to an executable file, searching `PATH` for a bare name.
fn locate(command: &str, path_var: Option<&std::ffi::OsString>) -> Option<PathBuf> {
    let as_path = Path::new(command);
    if as_path.is_absolute() || as_path.components().count() > 1 {
        return is_executable(as_path).then(|| as_path.to_path_buf());
    }

    let path_var = path_var?;
    for directory in std::env::split_paths(path_var) {
        let candidate = directory.join(command);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        if let Some(found) = windows_executable(&directory, command, cfg!(windows)) {
            return Some(found);
        }
    }
    None
}

/// The `.exe` a bare command names on Windows, if it is there.
///
/// The platform is a parameter rather than a `cfg!`, so the Windows lookup is
/// exercised on the machines that actually run this suite.
fn windows_executable(directory: &Path, command: &str, windows: bool) -> Option<PathBuf> {
    if !windows {
        return None;
    }
    let candidate = directory.join(format!("{command}.exe"));
    is_executable(&candidate).then_some(candidate)
}

/// Whether `path` is a file this process could execute.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

/// Whether `path` is a file. Windows has no execute bit to consult.
#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Ask an interpreter what version it is, within a bounded time.
///
/// Older Python releases printed their version to standard error rather than
/// standard output, so both are read. Returns `None` for anything other than a
/// clean, prompt answer.
pub async fn probe_version(binary: &Path) -> Option<String> {
    let mut command = tokio::process::Command::new(binary);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    no_console_window(&mut command);

    let output = tokio::time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let reported = if stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_owned()
    } else {
        stdout
    };
    if reported.is_empty() {
        None
    } else {
        Some(reported)
    }
}

/// Suppress the console window Windows would flash for each probe.
#[cfg(windows)]
fn no_console_window(command: &mut tokio::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// No-op off Windows.
#[cfg(not(windows))]
fn no_console_window(_command: &mut tokio::process::Command) {}

#[cfg(test)]
mod test;

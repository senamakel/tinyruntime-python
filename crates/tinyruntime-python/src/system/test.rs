//! Unit tests for host interpreter detection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use tinyruntime_bus::RuntimeSettings;

use super::{candidates, detect, locate, probe_version};
use crate::version::parse_version;

#[test]
fn the_series_specific_name_is_tried_before_the_generic_one() {
    // On a machine with several interpreters, `python3` is whatever the
    // distribution chose and is often older than the versioned binary next to it.
    let ordered = candidates(None, parse_version("3.12").unwrap());
    assert_eq!(ordered, vec!["python3.12", "python3", "python"]);
}

#[test]
fn a_preferred_command_is_tried_first() {
    let ordered = candidates(Some("/opt/py/bin/python3"), parse_version("3.12").unwrap());
    assert_eq!(ordered[0], "/opt/py/bin/python3");
    assert_eq!(ordered[1], "python3.12");
}

#[test]
fn a_preferred_command_that_is_already_a_fallback_is_not_repeated() {
    let ordered = candidates(Some("python3"), parse_version("3.12").unwrap());
    assert_eq!(ordered, vec!["python3", "python3.12", "python"]);
}

#[test]
fn the_series_name_follows_the_configured_floor() {
    let ordered = candidates(None, parse_version("3.14").unwrap());
    assert_eq!(ordered[0], "python3.14");
}

#[test]
fn an_absolute_command_that_is_not_there_does_not_resolve() {
    assert!(locate("/nonexistent/path/to/python3").is_none());
}

#[cfg(unix)]
#[test]
fn a_bare_command_resolves_through_path() {
    // `sh` is on PATH on every Unix host, so this exercises the lookup without
    // depending on Python being installed.
    assert!(locate("sh").is_some(), "PATH lookup found nothing at all");
}

#[cfg(unix)]
#[tokio::test]
async fn a_binary_that_does_not_understand_the_flag_is_not_an_interpreter() {
    if !Path::new("/bin/false").exists() {
        return;
    }
    assert!(probe_version(Path::new("/bin/false")).await.is_none());
}

#[tokio::test]
async fn a_binary_that_is_not_there_is_not_probed_successfully() {
    assert!(
        probe_version(Path::new("/nonexistent/python3"))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn an_unparseable_floor_detects_nothing() {
    assert!(detect(&RuntimeSettings::new("latest")).await.is_none());
}

#[tokio::test]
async fn a_ceiling_below_everything_installed_detects_nothing() {
    // Even on a machine with Python, a range nothing satisfies must come back
    // empty rather than handing over an interpreter outside it.
    let mut settings = RuntimeSettings::new("3.0");
    settings.maximum_version = "3.1".to_string();
    assert!(
        detect(&settings).await.is_none(),
        "an interpreter outside the requested range was accepted"
    );
}

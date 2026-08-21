//! Unit tests for Python version handling.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Version, parse_version, satisfies};

fn version(raw: &str) -> Version {
    parse_version(raw).unwrap_or_else(|| panic!("`{raw}` should parse"))
}

#[test]
fn every_spelling_that_turns_up_in_practice_parses() {
    assert_eq!(version("Python 3.12.4").to_string(), "3.12.4");
    assert_eq!(version("3.12.4").to_string(), "3.12.4");
    assert_eq!(version("  Python 3.12.4\n").to_string(), "3.12.4");
}

#[test]
fn a_series_with_no_patch_reads_as_the_first_release_of_it() {
    assert_eq!(version("3.12").to_string(), "3.12.0");
}

#[test]
fn a_release_candidate_parses_as_its_series() {
    // Treating it as unparseable would silently drop it from selection rather
    // than letting the version bounds decide.
    assert_eq!(version("3.13.0rc1").to_string(), "3.13.0");
    assert_eq!(version("3.13.2b1").to_string(), "3.13.2");
}

#[test]
fn something_that_is_not_a_version_does_not_parse() {
    assert_eq!(parse_version("latest"), None);
    assert_eq!(
        parse_version("3"),
        None,
        "a bare major is not a python version"
    );
    assert_eq!(parse_version(""), None);
}

#[test]
fn versions_order_by_component_rather_than_lexically() {
    // The bug this rules out: `3.9` sorting above `3.12` as text.
    assert!(version("3.12.0") > version("3.9.20"));
    assert!(version("3.12.10") > version("3.12.9"));
    assert!(version("4.0.0") > version("3.99.99"));
}

#[test]
fn a_request_names_a_floor_rather_than_an_exact_version() {
    assert!(satisfies(version("3.12.4"), "3.12", None));
    assert!(
        satisfies(version("3.13.1"), "3.12", None),
        "newer satisfies a floor"
    );
    assert!(!satisfies(version("3.11.9"), "3.12", None));
}

#[test]
fn an_exclusive_ceiling_keeps_selection_off_a_newer_series() {
    // The case this exists for: pre-releases of a newer series sitting in the
    // same index as the builds actually wanted.
    assert!(satisfies(version("3.14.1"), "3.12", Some("3.15")));
    assert!(!satisfies(version("3.15.0"), "3.12", Some("3.15")));
    assert!(
        !satisfies(version("3.15.0"), "3.12", Some("3.15")),
        "the ceiling is exclusive, so 3.15.0 itself is out"
    );
}

#[test]
fn a_blank_ceiling_is_no_ceiling() {
    assert!(satisfies(version("3.99.0"), "3.12", Some("")));
    assert!(satisfies(version("3.99.0"), "3.12", Some("   ")));
    assert!(satisfies(version("3.99.0"), "3.12", None));
}

#[test]
fn a_floor_that_is_not_a_version_accepts_nothing() {
    // A misconfigured floor must not quietly install whatever came first.
    assert!(!satisfies(version("3.12.4"), "latest", None));
}

#[test]
fn a_ceiling_that_is_not_a_version_accepts_nothing() {
    assert!(!satisfies(version("3.12.4"), "3.12", Some("nonsense")));
}

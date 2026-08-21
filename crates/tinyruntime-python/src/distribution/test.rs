//! Unit tests for standalone build selection.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Release, host_suffix, select_from, suffix_for};
use crate::error::Error;

/// The Linux x86-64 suffix, which the fixtures below are written against.
const LINUX: &str = "x86_64-unknown-linux-gnu-install_only.tar.gz";

/// A release index shaped like the real one, including the assets that must be
/// ignored and the near-miss hosts that must not match.
fn release() -> Release {
    serde_json::from_value(serde_json::json!({
        "tag_name": "20240909",
        "assets": [
            {
                "name": "cpython-3.12.4+20240909-x86_64-unknown-linux-gnu-install_only.tar.gz",
                "browser_download_url": "https://example.invalid/3.12.4-full",
                "digest": "sha256:aa"
            },
            {
                "name": "cpython-3.12.4+20240909-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz",
                "browser_download_url": "https://example.invalid/3.12.4-stripped",
                "digest": "sha256:bb"
            },
            {
                "name": "cpython-3.13.1+20240909-x86_64-unknown-linux-gnu-install_only.tar.gz",
                "browser_download_url": "https://example.invalid/3.13.1",
                "digest": "sha256:cc"
            },
            {
                "name": "cpython-3.15.0rc1+20240909-x86_64-unknown-linux-gnu-install_only.tar.gz",
                "browser_download_url": "https://example.invalid/3.15.0rc1",
                "digest": "sha256:dd"
            },
            {
                "name": "cpython-3.13.1+20240909-aarch64-apple-darwin-install_only.tar.gz",
                "browser_download_url": "https://example.invalid/darwin",
                "digest": "sha256:ee"
            },
            {
                "name": "cpython-3.13.1+20240909-x86_64-unknown-linux-gnu-debug-full.tar.zst",
                "browser_download_url": "https://example.invalid/debug",
                "digest": "sha256:ff"
            },
            {
                "name": "SHA256SUMS",
                "browser_download_url": "https://example.invalid/sums",
                "digest": null
            }
        ]
    }))
    .expect("the fixture is a valid release")
}

#[test]
fn the_newest_build_within_the_bounds_is_chosen() {
    let chosen = select_from(&release(), "3.12", Some("3.15"), LINUX).expect("a build matches");
    assert_eq!(chosen.version, "3.13.1");
    assert_eq!(chosen.expected_sha256.as_deref(), Some("cc"));
}

#[test]
fn a_stripped_build_wins_a_tie_with_a_full_one() {
    // Stripped builds omit debug symbols and static libraries — hundreds of
    // megabytes nothing here uses.
    let chosen = select_from(&release(), "3.12", Some("3.13"), LINUX).expect("a build matches");
    assert!(
        chosen.archive_name.contains("install_only_stripped"),
        "chose {}",
        chosen.archive_name
    );
    assert_eq!(chosen.expected_sha256.as_deref(), Some("bb"));
}

#[test]
fn an_exclusive_ceiling_keeps_selection_off_a_pre_release_series() {
    // The 3.15 release candidate is in the same index. Without the ceiling it
    // would be the newest thing there and would win.
    let bounded = select_from(&release(), "3.12", Some("3.15"), LINUX).unwrap();
    assert_eq!(bounded.version, "3.13.1");

    let unbounded = select_from(&release(), "3.12", None, LINUX).unwrap();
    assert_eq!(
        unbounded.version, "3.15.0",
        "the ceiling was doing the work"
    );
}

#[test]
fn builds_for_another_host_are_not_considered() {
    // The darwin asset in the fixture is a newer-or-equal version; matching it
    // would install an interpreter that cannot run on this machine.
    let chosen = select_from(&release(), "3.12", Some("3.15"), LINUX).unwrap();
    assert!(
        !chosen.archive_name.contains("darwin"),
        "chose {}",
        chosen.archive_name
    );
}

#[test]
fn artifacts_that_are_not_a_runnable_interpreter_are_ignored() {
    // Debug archives and checksum files sit in the same release.
    let chosen = select_from(&release(), "3.12", None, LINUX).unwrap();
    assert!(chosen.archive_name.contains("install_only"));
    assert!(chosen.archive_name.ends_with(".tar.gz"));
}

#[test]
fn the_install_directory_is_named_from_the_asset_not_the_archive() {
    // Every standalone build unpacks into a directory called `python`. Naming
    // the install from that would make every version claim one cache directory
    // and silently replace the last.
    let chosen = select_from(&release(), "3.12", Some("3.15"), LINUX).unwrap();
    assert_eq!(
        chosen.install_dir_name,
        "cpython-3.13.1+20240909-x86_64-unknown-linux-gnu-install_only"
    );
    assert_ne!(chosen.install_dir_name, "python");
}

#[test]
fn a_floor_nothing_reaches_is_a_distinct_failure_from_an_unreadable_index() {
    let error = select_from(&release(), "3.99", None, LINUX).expect_err("nothing is that new");
    let Error::NoMatchingBuild { release, bounds } = &error else {
        panic!("got {error:?}");
    };
    assert_eq!(release, "20240909");
    assert!(
        bounds.contains("3.99"),
        "the bounds that excluded everything are named"
    );
}

#[test]
fn a_bound_that_is_not_a_version_is_refused_by_name() {
    let error = select_from(&release(), "latest", None, LINUX).expect_err("refused");
    assert!(matches!(
        error,
        Error::InvalidVersion {
            bound: "minimum",
            ..
        }
    ));

    let error = select_from(&release(), "3.12", Some("nonsense"), LINUX).expect_err("refused");
    assert!(matches!(
        error,
        Error::InvalidVersion {
            bound: "maximum",
            ..
        }
    ));
}

#[test]
fn a_release_with_no_digest_still_selects() {
    // The router installs it and says loudly that it could not verify it;
    // refusing here would make the language unusable rather than safer.
    let release: Release = serde_json::from_value(serde_json::json!({
        "tag_name": "20240909",
        "assets": [{
            "name": "cpython-3.12.4+20240909-x86_64-unknown-linux-gnu-install_only.tar.gz",
            "browser_download_url": "https://example.invalid/3.12.4",
            "digest": null
        }]
    }))
    .unwrap();
    let chosen = select_from(&release, "3.12", None, LINUX).expect("a build matches");
    assert!(chosen.expected_sha256.is_none());
}

#[test]
fn every_platform_the_channel_publishes_for_is_in_the_table() {
    for (os, arch, expected) in [
        ("linux", "x86_64", LINUX),
        (
            "linux",
            "aarch64",
            "aarch64-unknown-linux-gnu-install_only.tar.gz",
        ),
        (
            "macos",
            "aarch64",
            "aarch64-apple-darwin-install_only.tar.gz",
        ),
        ("macos", "x86_64", "x86_64-apple-darwin-install_only.tar.gz"),
        (
            "windows",
            "x86_64",
            "x86_64-pc-windows-msvc-install_only.tar.gz",
        ),
    ] {
        assert_eq!(
            suffix_for(os, arch).unwrap_or_else(|_| panic!("{os}/{arch} is missing")),
            expected
        );
    }
}

#[test]
fn a_host_the_channel_does_not_publish_for_is_refused_by_name() {
    let error = suffix_for("plan9", "x86_64").expect_err("no build exists");
    assert!(error.to_string().contains("plan9"), "got `{error}`");
}

#[test]
fn this_machine_is_one_the_channel_publishes_for() {
    assert!(
        host_suffix().is_ok(),
        "no build for {}",
        std::env::consts::ARCH
    );
}

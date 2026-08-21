//! Integration tests for the public crate surface.
//!
//! These link against the crate as a downstream consumer would: they can only
//! use what `src/lib.rs` re-exports. Treat them as the regression suite for the
//! crate's public contract — if a change breaks a test here, it is a breaking
//! change for users.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyruntime_python::{
    CONTRACT_VERSION, DEFAULT_VERSION, Error, Language, PYTHON, distribution, harness, layout,
    names, parse_version, satisfies,
};

#[test]
fn the_provider_serves_the_shared_interface_from_the_contract() {
    assert_eq!(
        names::PROVIDER_INTERFACE,
        tinyruntime_python::PROVIDER_INTERFACE
    );
    assert_eq!(
        names::providers::PYTHON_OBJECT_PATH,
        names::object_path_for(names::providers::PYTHON)
    );
    assert_eq!(names::PROVIDER_METHODS.len(), 5);
    assert_eq!(Language::python().as_str(), PYTHON);
}

#[test]
fn the_contract_is_re_exported_so_consumers_take_one_dependency() {
    let same: tinyruntime_bus::WorkerHarness = harness();
    assert_eq!(same, harness());
    assert!(tinyruntime_python::is_compatible_with_contract());
    let _ = CONTRACT_VERSION;
}

#[test]
fn the_default_floor_is_one_this_crate_can_reason_about() {
    let floor = parse_version(DEFAULT_VERSION).expect("the default floor is a version");
    assert!(satisfies(floor, DEFAULT_VERSION, None));
}

#[test]
fn the_host_table_and_the_layout_agree_about_this_platform() {
    // A host the channel publishes for must also be one the layout knows the
    // shape of, or an install would succeed and then be unusable.
    if distribution::host_suffix().is_ok() {
        let scratch = tempfile::tempdir().unwrap();
        assert!(
            layout::find_interpreter(scratch.path()).is_none(),
            "an empty directory must not read as an install"
        );
        assert!(layout::TOOLS.contains(&"python"));
    }
}

#[test]
fn an_unsupported_host_is_reported_by_name() {
    let error = distribution::suffix_for("plan9", "x86_64").expect_err("no build exists");
    assert!(matches!(error, Error::UnsupportedHost { .. }));
}

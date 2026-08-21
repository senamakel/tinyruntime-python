//! Unit tests for the Python install layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use super::{find_interpreter, from_parts};

/// Build an unpacked standalone build with the named files in its bin directory.
fn fabricate(root: &Path, files: &[&str]) -> std::path::PathBuf {
    let bin = if cfg!(windows) {
        root.join("python")
    } else {
        root.join("python").join("bin")
    };
    fs::create_dir_all(&bin).unwrap();
    for file in files {
        fs::write(bin.join(file), b"").unwrap();
    }
    bin
}

#[cfg(unix)]
#[test]
fn the_interpreter_is_found_inside_the_channels_python_directory() {
    // Every standalone build, of every version, unpacks into `python/`.
    let scratch = tempfile::tempdir().unwrap();
    fabricate(scratch.path(), &["python3"]);

    let found = find_interpreter(scratch.path()).expect("the interpreter is there");
    assert!(found.ends_with("python/bin/python3"), "found {}", found.display());
}

#[cfg(unix)]
#[test]
fn an_install_without_the_wrapper_directory_still_resolves() {
    // A host interpreter, or a build laid out differently, is still usable.
    let scratch = tempfile::tempdir().unwrap();
    let bin = scratch.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("python3"), b"").unwrap();

    let found = find_interpreter(scratch.path()).expect("the interpreter is there");
    assert!(found.ends_with("bin/python3"), "found {}", found.display());
}

#[test]
fn a_directory_with_no_interpreter_is_not_an_install() {
    let scratch = tempfile::tempdir().unwrap();
    fabricate(scratch.path(), &[]);
    assert!(find_interpreter(scratch.path()).is_none());
}

#[test]
fn an_empty_directory_is_not_an_install() {
    let scratch = tempfile::tempdir().unwrap();
    assert!(find_interpreter(scratch.path()).is_none());
}

#[cfg(unix)]
#[test]
fn a_layout_records_the_interpreter_and_its_package_installer() {
    let scratch = tempfile::tempdir().unwrap();
    let bin = fabricate(scratch.path(), &["python3", "pip3"]);

    let layout = from_parts(&bin, &bin.join("python3"), "3.12.4");
    assert_eq!(layout.version, "3.12.4");
    assert!(layout.executable("python").unwrap().ends_with("python3"));
    assert!(layout.executable("pip").unwrap().ends_with("pip3"));
}

#[cfg(unix)]
#[test]
fn an_install_without_pip_is_still_a_usable_layout() {
    // Claiming a pip that is not there turns a clear absence into a confusing
    // spawn failure much later.
    let scratch = tempfile::tempdir().unwrap();
    let bin = fabricate(scratch.path(), &["python3"]);

    let layout = from_parts(&bin, &bin.join("python3"), "3.12.4");
    assert!(layout.executable("python").is_some());
    assert!(layout.executable("pip").is_none());
}

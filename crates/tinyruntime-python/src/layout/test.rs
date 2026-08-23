//! Unit tests for the Python install layout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use tinyruntime_bus::RuntimeSettings;

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
    assert!(
        found.ends_with("python/bin/python3"),
        "found {}",
        found.display()
    );
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

#[test]
fn the_windows_layout_is_checked_everywhere_rather_than_only_on_windows() {
    // A standalone build on Windows has no `bin/` directory and ships
    // `python.exe`. Neither is exercised by a `cfg!` branch on Linux.
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("python");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("python.exe"), b"").unwrap();

    let found = super::find_interpreter_for(scratch.path(), true)
        .expect("the Windows interpreter is found");
    assert!(
        found.ends_with("python/python.exe"),
        "found {}",
        found.display()
    );

    assert!(
        super::find_interpreter_for(scratch.path(), false).is_none(),
        "the Unix search must not match a Windows layout"
    );
}

#[test]
fn the_package_installer_is_named_per_platform() {
    assert_eq!(super::pip_names_for(true), vec!["pip.exe", "pip3.exe"]);
    assert_eq!(super::pip_names_for(false), vec!["pip3", "pip"]);
}

/// Write an executable standing in for an interpreter, printing `version`.
///
/// Waits until the script actually runs before returning. A file written and
/// immediately executed can transiently fail — the kernel may still see a writer
/// on it — and a failed probe is reported as "no interpreter", which would
/// surface as a confusing assertion failure rather than as the flake it is.
#[cfg(unix)]
fn fake_python(bin: &Path, version: &str) {
    use std::os::unix::fs::PermissionsExt;

    let path = bin.join("python3");
    fs::write(&path, format!("#!/bin/sh\necho '{version}'\n")).expect("the script writes");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("it is executable");

    for _ in 0..50 {
        if std::process::Command::new(&path)
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!(
        "the fake interpreter at {} never became runnable",
        path.display()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_install_inside_the_requested_range_is_described() {
    let scratch = tempfile::tempdir().unwrap();
    let bin = fabricate(scratch.path(), &["pip3"]);
    fake_python(&bin, "Python 3.12.4");

    let layout = super::describe(scratch.path(), &RuntimeSettings::new("3.12"))
        .await
        .expect("a compatible install is described");

    assert_eq!(layout.version, "3.12.4");
    assert!(layout.executable("python").is_some());
    assert!(layout.executable("pip").is_some());
}

#[cfg(unix)]
#[tokio::test]
async fn an_install_outside_the_requested_range_is_not_described() {
    // The router scans a cache that may hold several series; reporting one the
    // caller excluded would run the wrong interpreter.
    let scratch = tempfile::tempdir().unwrap();
    let bin = fabricate(scratch.path(), &[]);
    fake_python(&bin, "Python 3.11.9");

    assert!(
        super::describe(scratch.path(), &RuntimeSettings::new("3.12"))
            .await
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_install_whose_interpreter_does_not_answer_is_not_described() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = tempfile::tempdir().unwrap();
    let bin = fabricate(scratch.path(), &[]);
    let path = bin.join("python3");
    fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        super::describe(scratch.path(), &RuntimeSettings::new("3.12"))
            .await
            .is_none()
    );
}

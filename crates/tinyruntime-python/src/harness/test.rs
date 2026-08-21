//! Unit tests for the shipped harness.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyruntime_bus::WORKER_PROTOCOL_VERSION;

use super::{FILENAME, SOURCE, harness};

#[test]
fn the_harness_runs_under_python_and_speaks_this_protocol() {
    let harness = harness();
    assert_eq!(harness.executable, "python");
    assert_eq!(harness.filename, FILENAME);
    assert_eq!(
        harness.protocol_version, WORKER_PROTOCOL_VERSION,
        "a harness on another protocol is refused at the handshake"
    );
}

#[test]
fn output_is_unbuffered_by_both_the_flag_and_the_environment() {
    // Without these a job's output can sit in a buffer until the process exits,
    // which for a worker that never exits means it is never seen at all.
    let harness = harness();
    assert!(harness.args_before_script.contains(&"-u".to_string()));
    assert!(
        harness
            .env
            .contains(&("PYTHONUNBUFFERED".to_string(), "1".to_string()))
    );
    assert_eq!(
        harness.command_args("/cache/pool_worker.py").last().map(String::as_str),
        Some("/cache/pool_worker.py"),
        "the script must come after the flags"
    );
}

#[test]
fn the_harness_announces_the_protocol_version_this_build_speaks() {
    // The script's constant and the contract's are two separate declarations of
    // one number; a mismatch fails every handshake at runtime.
    assert!(
        SOURCE.contains(&format!("PROTOCOL_VERSION = {WORKER_PROTOCOL_VERSION}")),
        "the harness declares a different protocol version than the contract"
    );
}

#[test]
fn the_harness_reads_the_protocol_environment_the_router_sets() {
    assert!(SOURCE.contains("TINYRUNTIME_PROTOCOL_ADDR"));
    assert!(SOURCE.contains("TINYRUNTIME_PROTOCOL_TOKEN"));
}

#[test]
fn output_is_captured_at_the_file_descriptor_level() {
    // Swapping `sys.stdout` would miss `os.write(1, ...)`, subprocess output,
    // and anything a native extension writes — all of which would then land on
    // whatever the real descriptor points at.
    assert!(SOURCE.contains("os.dup2("), "capture is not descriptor-level");
    assert!(
        SOURCE.contains("tempfile.TemporaryFile"),
        "a pipe would deadlock on a job that outproduces its buffer"
    );
}

#[test]
fn a_job_that_cannot_enter_its_directory_fails_rather_than_running_elsewhere() {
    assert!(SOURCE.contains("failed to set worker cwd"));
}

#[test]
fn a_job_calling_sys_exit_is_reported_rather_than_ending_the_worker() {
    // A long-lived worker must survive `sys.exit(3)`; the exit code belongs in
    // the reply instead.
    assert!(SOURCE.contains("except SystemExit"));
}

//! End-to-end tests for the shipped worker harness, against a real `python`.
//!
//! The harness is the one part of this crate that is not Rust, so nothing else
//! in the suite can check that it actually speaks the protocol. These tests
//! stand in for the router: they listen on loopback, launch the harness the way
//! the router would, complete the handshake, and run jobs through it.
//!
//! They skip when the machine has no Python 3, so the suite stays hermetic on a
//! runner without one rather than failing for the wrong reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};

use tinyruntime_python::{WORKER_PROTOCOL_VERSION, harness};

/// How long any single step may take before the test gives up.
const STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// Read and discard a child stream, so a job writing to it never blocks.
fn drain(stream: impl tokio::io::AsyncRead + Send + Unpin + 'static) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        while let Ok(Some(_)) = lines.next_line().await {}
    });
}

/// The first working Python 3 on this machine, or `None`.
async fn interpreter() -> Option<String> {
    for candidate in ["python3", "python"] {
        let usable = Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success());
        if usable {
            return Some(candidate.to_string());
        }
    }
    None
}

/// A harness process under test, plus the protocol connection to it.
struct Harness {
    _child: Child,
    writer: tokio::io::WriteHalf<TcpStream>,
    lines: Lines<BufReader<tokio::io::ReadHalf<TcpStream>>>,
    _scratch: tempfile::TempDir,
    cwd: PathBuf,
}

impl Harness {
    /// Launch the harness the way the router would, or `None` without Python.
    async fn launch() -> Option<Self> {
        let Some(binary) = interpreter().await else {
            eprintln!("skipped: this machine has no usable python");
            return None;
        };

        let scratch = tempfile::tempdir().unwrap();
        let harness = harness();
        let script = scratch.path().join(&harness.filename);
        std::fs::write(&script, &harness.source).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let token = "test-secret-token";

        let mut command = Command::new(binary);
        command
            .args(harness.command_args(&script.to_string_lossy()))
            .env("TINYRUNTIME_PROTOCOL_ADDR", address.to_string())
            .env("TINYRUNTIME_PROTOCOL_TOKEN", token)
            .env("PATH", std::env::var("PATH").unwrap_or_default());
        for (name, value) in &harness.env {
            command.env(name, value);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let (stream, _) = tokio::time::timeout(STEP_TIMEOUT, listener.accept())
            .await
            .expect("the harness connected back")
            .unwrap();
        let (reader, writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        let handshake: serde_json::Value = serde_json::from_str(
            &tokio::time::timeout(STEP_TIMEOUT, lines.next_line())
                .await
                .expect("the handshake arrived")
                .unwrap()
                .expect("the harness sent a handshake"),
        )
        .unwrap();

        assert_eq!(handshake["ready"], serde_json::json!(true));
        assert_eq!(
            handshake["protocol"],
            serde_json::json!(WORKER_PROTOCOL_VERSION)
        );
        assert_eq!(handshake["language"], serde_json::json!("python"));
        assert_eq!(
            handshake["token"],
            serde_json::json!(token),
            "the harness must echo the secret it was given"
        );

        // Drain the child's own descriptors, exactly as the router does. This is
        // not tidiness: a job that writes to fd 1 blocks once the pipe fills.
        if let Some(stdout) = child.stdout.take() {
            drain(stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            drain(stderr);
        }

        let cwd = scratch.path().to_path_buf();
        Some(Self {
            _child: child,
            writer,
            lines,
            _scratch: scratch,
            cwd,
        })
    }

    /// Run one job and return its reply.
    async fn run(&mut self, id: &str, code: &str, timeout_ms: Option<u64>) -> serde_json::Value {
        let mut request = serde_json::json!({
            "id": id,
            "code": code,
            "cwd": self.cwd.to_string_lossy(),
        });
        if let Some(timeout_ms) = timeout_ms {
            request["timeout_ms"] = serde_json::json!(timeout_ms);
        }

        let mut line = serde_json::to_string(&request).unwrap();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();

        let reply = tokio::time::timeout(STEP_TIMEOUT, self.lines.next_line())
            .await
            .expect("the harness replied")
            .unwrap()
            .expect("the harness sent a reply");
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(
            reply["id"],
            serde_json::json!(id),
            "reply was for another job"
        );
        reply
    }
}

#[tokio::test]
async fn the_harness_runs_a_job_and_reports_its_output() {
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let reply = harness.run("1", "print(6 * 7)", None).await;
    assert_eq!(reply["ok"], serde_json::json!(true));
    assert_eq!(reply["stdout"], serde_json::json!("42\n"));
    assert_eq!(reply["exit_code"], serde_json::json!(0));
}

#[tokio::test]
async fn one_warm_worker_serves_many_jobs() {
    // The entire reason the pool exists. If the harness exited after a job, this
    // would fail on the second one.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    for index in 0..3 {
        let reply = harness
            .run(&index.to_string(), &format!("print({index} + 1)"), None)
            .await;
        assert_eq!(
            reply["stdout"],
            serde_json::json!(format!("{}\n", index + 1))
        );
    }
}

#[tokio::test]
async fn each_job_gets_fresh_globals() {
    // The only isolation a Python worker can offer. Without it, a name defined
    // by one job would be visible to an unrelated later one.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    harness
        .run("1", "leaked = 'from the first job'", None)
        .await;

    let reply = harness.run("2", "print('leaked' in dir())", None).await;
    assert_eq!(reply["stdout"], serde_json::json!("False\n"));
}

#[tokio::test]
async fn a_job_that_raises_reports_a_traceback_without_killing_the_worker() {
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let raised = harness.run("1", "raise ValueError('boom')", None).await;
    assert_eq!(
        raised["ok"],
        serde_json::json!(true),
        "the harness ran it; the job failed"
    );
    assert_eq!(raised["exit_code"], serde_json::json!(1));
    assert!(raised["stderr"].as_str().unwrap().contains("boom"));

    let after = harness.run("2", "print('still here')", None).await;
    assert_eq!(after["stdout"], serde_json::json!("still here\n"));
}

#[tokio::test]
async fn a_job_calling_sys_exit_reports_its_code_and_leaves_the_worker_running() {
    // A long-lived worker must survive `sys.exit(3)`. Without the SystemExit
    // arm the interpreter would exit and take every queued job with it.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let exited = harness.run("1", "import sys; sys.exit(3)", None).await;
    assert_eq!(exited["exit_code"], serde_json::json!(3));

    let after = harness.run("2", "print('survived')", None).await;
    assert_eq!(after["stdout"], serde_json::json!("survived\n"));
}

#[tokio::test]
async fn output_written_past_the_python_layer_is_still_captured() {
    // `os.write(1, ...)` bypasses `sys.stdout` entirely. If capture were not at
    // the file-descriptor level this would land on the real descriptor instead
    // of in the reply.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let reply = harness
        .run("1", "import os; os.write(1, b'LOW_LEVEL')", None)
        .await;
    assert_eq!(reply["stdout"], serde_json::json!("LOW_LEVEL"));
}

#[tokio::test]
async fn a_subprocess_started_by_a_job_is_captured_too() {
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let reply = harness
        .run(
            "1",
            "import subprocess, sys; subprocess.run([sys.executable, '-c', \"print('FROM_CHILD')\"])",
            None,
        )
        .await;
    assert_eq!(reply["stdout"], serde_json::json!("FROM_CHILD\n"));
}

#[tokio::test]
async fn relative_paths_resolve_against_the_job_directory() {
    // A shared warm worker runs wherever the last job left it unless the harness
    // enters each job's directory. Getting this wrong escapes the caller's sandbox.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    std::fs::write(harness.cwd.join("probe.txt"), b"RELATIVE_OK").unwrap();

    let reply = harness
        .run("1", "print(open('./probe.txt').read(), end='')", None)
        .await;
    assert_eq!(reply["stdout"], serde_json::json!("RELATIVE_OK"));
}

#[tokio::test]
async fn a_job_whose_directory_is_missing_fails_rather_than_running_elsewhere() {
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let sentinel = harness.cwd.join("must-not-exist.txt");
    harness.cwd = harness.cwd.join("deleted-sandbox");

    let reply = harness
        .run(
            "1",
            "open('must-not-exist.txt', 'w').write('escaped')",
            None,
        )
        .await;
    assert_eq!(reply["ok"], serde_json::json!(false), "the job ran anyway");
    assert!(
        reply["error"]
            .as_str()
            .unwrap()
            .contains("failed to set worker cwd")
    );
    assert!(!sentinel.exists(), "the job escaped its sandbox");
}

#[tokio::test]
async fn the_working_directory_is_restored_between_jobs() {
    // Jobs share one interpreter, so a job that changed directory and was not
    // restored would silently relocate every job after it.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    std::fs::create_dir(harness.cwd.join("elsewhere")).unwrap();
    harness
        .run("1", "import os; os.chdir('elsewhere')", None)
        .await;

    std::fs::write(harness.cwd.join("probe.txt"), b"STILL_HERE").unwrap();
    let reply = harness
        .run("2", "print(open('./probe.txt').read(), end='')", None)
        .await;
    assert_eq!(reply["stdout"], serde_json::json!("STILL_HERE"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_job_that_never_finishes_is_aborted_at_its_deadline() {
    // Best effort, and only on Unix: the deadline is a SIGALRM, and there is no
    // equivalent on Windows. The router's hard deadline is the backstop there.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let reply = harness
        .run("1", "import time; time.sleep(60)", Some(1_000))
        .await;
    assert_eq!(reply["timed_out"], serde_json::json!(true));

    // And the worker is still usable afterwards.
    let after = harness.run("2", "print('alive')", None).await;
    assert_eq!(after["stdout"], serde_json::json!("alive\n"));
}

#[tokio::test]
async fn a_job_reads_end_of_file_on_standard_input() {
    // Standard input must never be the protocol stream, or a job that read it
    // would consume the next request.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let reply = harness
        .run(
            "1",
            "import os, sys; print(repr(sys.stdin.read())); print(os.read(0, 1))",
            Some(5_000),
        )
        .await;
    assert_eq!(reply["stdout"], serde_json::json!("''\nb''\n"));
}

#[tokio::test]
async fn a_job_cannot_forge_a_reply_over_its_own_descriptor() {
    // The reason the protocol has its own socket: a job writing a frame-shaped
    // line to fd 1 must not be able to answer its own request. Here the write is
    // captured as ordinary job output instead.
    let Some(mut harness) = Harness::launch().await else {
        return;
    };
    let reply = harness
        .run(
            "1",
            "import os, json; os.write(1, (json.dumps({'id': '1', 'ok': True, 'stdout': 'FORGED'}) + '\\n').encode()); print('REAL')",
            None,
        )
        .await;

    assert_eq!(reply["ok"], serde_json::json!(true));
    let stdout = reply["stdout"].as_str().unwrap();
    assert!(
        stdout.contains("FORGED"),
        "the forged frame was not captured as output"
    );
    assert!(stdout.ends_with("REAL\n"), "got {stdout:?}");
}

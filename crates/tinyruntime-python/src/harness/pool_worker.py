# The tinyruntime Python worker harness.
#
# One long-lived `python` process that runs many inline jobs, so a host pays for
# one warm interpreter instead of one child per execution.
#
# Protocol (newline-delimited JSON over an authenticated loopback socket):
#   1. Send exactly one handshake: {ready, protocol, language, token}
#   2. For each {id, code, cwd, timeout_ms} reply with
#      {id, ok, stdout, stderr, exit_code, timed_out, elapsed_ms, error}
#
# Python cannot isolate a job the way the Node harness does. There is no
# equivalent of a worker thread here — CPython cannot safely kill a running
# thread — so a job runs in this interpreter, and two things follow.
#
# Isolation is per-job globals plus the pool's recycle-after-N-jobs. Module
# state, `os.environ`, and logging handlers do leak between jobs on one worker,
# which is why the router leaves Python pooling off unless a host opts in.
#
# The soft deadline is best effort: a SIGALRM on Unix, and nothing on Windows.
# The router's own hard deadline is the backstop, and it kills and replaces the
# worker rather than waiting.

import json
import os
import socket
import sys
import tempfile
import time
import traceback

PROTOCOL_VERSION = 1

_TOKEN = os.environ.get("TINYRUNTIME_PROTOCOL_TOKEN")
_ADDRESS = os.environ.get("TINYRUNTIME_PROTOCOL_ADDR")

if not _ADDRESS:
    sys.stderr.write("tinyruntime: no protocol address was supplied\n")
    sys.exit(1)

_host, _port = _ADDRESS.rsplit(":", 1)
try:
    _SOCKET = socket.create_connection((_host, int(_port)))
except OSError as exc:
    sys.stderr.write(f"tinyruntime: protocol connection failed: {exc!r}\n")
    sys.exit(1)

# Separate file objects for the two directions. The protocol never touches file
# descriptors 0, 1, or 2 — those belong to the job, and the redirection below
# reassigns them freely.
_INCOMING = _SOCKET.makefile("r")
_OUTGOING = _SOCKET.makefile("w", buffering=1)

try:
    import signal

    _HAVE_ALARM = hasattr(signal, "SIGALRM") and hasattr(signal, "setitimer")
except ImportError:  # pragma: no cover - a platform without signal support
    signal = None
    _HAVE_ALARM = False


class _JobTimeout(Exception):
    """Raised from the alarm handler to unwind a job at its soft deadline."""


def _failure(job, started, message):
    return {
        "id": job.get("id") if isinstance(job, dict) else None,
        "ok": False,
        "stdout": "",
        "stderr": "",
        "exit_code": None,
        "timed_out": False,
        "elapsed_ms": int((time.time() - started) * 1000),
        "error": message,
    }


def _run_job(job):
    code = job.get("code") or ""
    cwd = job.get("cwd")
    timeout_ms = job.get("timeout_ms")
    started = time.time()
    exit_code = 0
    timed_out = False
    extra_stderr = ""

    # Entering the job's directory before running anything. Failing here rather
    # than running anyway matters: a job that silently ran in the previous job's
    # directory would escape whatever sandbox the caller set up.
    previous_cwd = None
    if cwd:
        try:
            previous_cwd = os.getcwd()
            os.chdir(cwd)
        except OSError as exc:
            return _failure(job, started, f"failed to set worker cwd: {exc!r}")

    # Capture at the file-descriptor level rather than by swapping `sys.stdout`.
    # A job can write with `os.write(1, ...)`, spawn a subprocess, or call into a
    # native extension, and none of those go through `sys.stdout`. Temporary
    # files rather than pipes, because a pipe would deadlock on a job that
    # produces more output than its buffer holds.
    stdin_capture = tempfile.TemporaryFile(mode="w+b")
    stdout_capture = tempfile.TemporaryFile(mode="w+b")
    stderr_capture = tempfile.TemporaryFile(mode="w+b")
    saved_stdin = os.dup(0)
    saved_stdout = os.dup(1)
    saved_stderr = os.dup(2)
    os.dup2(stdin_capture.fileno(), 0)
    os.dup2(stdout_capture.fileno(), 1)
    os.dup2(stderr_capture.fileno(), 2)

    armed = False
    if _HAVE_ALARM and timeout_ms and timeout_ms > 0:

        def _on_alarm(_signum, _frame):
            raise _JobTimeout()

        signal.signal(signal.SIGALRM, _on_alarm)
        signal.setitimer(signal.ITIMER_REAL, timeout_ms / 1000.0)
        armed = True

    try:
        # Fresh globals per job, so a name defined by one job is not visible to
        # the next one on this worker.
        namespace = {"__name__": "__main__", "__builtins__": __builtins__}
        exec(compile(code, "<inline>", "exec"), namespace, namespace)
    except _JobTimeout:
        timed_out = True
    except SystemExit as exc:  # honour sys.exit(n)
        if exc.code is None:
            exit_code = 0
        elif isinstance(exc.code, int):
            exit_code = exc.code
        else:
            exit_code = 1
            extra_stderr = str(exc.code) + "\n"
    except BaseException:  # noqa: BLE001 - every job failure belongs to the caller
        exit_code = 1
        extra_stderr = traceback.format_exc()
    finally:
        if armed:
            signal.setitimer(signal.ITIMER_REAL, 0)
        # Flush Python's own buffers into the redirected descriptors before
        # restoring them, or the last of a job's output is lost. A flush that
        # fails is reported in the job's stderr rather than discarded.
        for stream, label in ((sys.stdout, "stdout"), (sys.stderr, "stderr")):
            try:
                stream.flush()
            except Exception as exc:  # noqa: BLE001
                extra_stderr += f"[harness] {label} flush failed: {exc!r}\n"
        os.dup2(saved_stdin, 0)
        os.dup2(saved_stdout, 1)
        os.dup2(saved_stderr, 2)
        os.close(saved_stdin)
        os.close(saved_stdout)
        os.close(saved_stderr)
        if previous_cwd is not None:
            try:
                os.chdir(previous_cwd)
            except OSError:
                pass  # the next job sets its own directory anyway

    stdin_capture.close()
    stdout_capture.seek(0)
    stderr_capture.seek(0)
    stdout = stdout_capture.read().decode("utf-8", "replace")
    stderr = stderr_capture.read().decode("utf-8", "replace") + extra_stderr
    stdout_capture.close()
    stderr_capture.close()

    return {
        "id": job.get("id"),
        "ok": True,
        "stdout": stdout,
        "stderr": stderr,
        "exit_code": None if timed_out else exit_code,
        "timed_out": timed_out,
        "elapsed_ms": int((time.time() - started) * 1000),
        "error": None,
    }


def _send(frame):
    _OUTGOING.write(json.dumps(frame) + "\n")
    _OUTGOING.flush()


def main():
    _send(
        {
            "ready": True,
            "protocol": PROTOCOL_VERSION,
            "language": "python",
            "token": _TOKEN,
        }
    )

    for line in _INCOMING:
        line = line.strip()
        if not line:
            continue
        try:
            job = json.loads(line)
        except ValueError:
            continue  # an unparseable line is not a job
        try:
            reply = _run_job(job)
        except BaseException as exc:  # noqa: BLE001 - a harness-level failure
            reply = _failure(job, time.time(), repr(exc))
        _send(reply)


if __name__ == "__main__":
    main()

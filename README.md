# tinyruntime-python

The Python provider for [`tinyruntime`](https://github.com/tinyhumansai/tinyruntime).

## What this is

One half of a deliberate split.

`tinyruntime` — the router — owns everything that is the same for every
language: downloading an archive, verifying its digest, unpacking it, promoting
it into a cache atomically, reusing it on the next start, and keeping a bounded
set of warm interpreter processes in front of it.

This module owns everything that is true only of Python. It answers five
questions and does nothing else:

| Member | What it answers |
| --- | --- |
| `Describe` | what this provider is and what it targets by default |
| `DetectSystem` | whether the host already has a usable interpreter |
| `SelectDistribution` | which standalone build to install for this machine |
| `Layout` | where the interpreter is inside an unpacked install |
| `Harness` | what a warm Python worker is |

It downloads nothing, installs nothing, and starts no worker. Every answer it
gives is a description the router acts on.

## The Python knowledge, in four parts

**A request names a floor, not a version.** `3.12` means "3.12 or newer". That
follows from the channel: `astral-sh/python-build-standalone` publishes a moving
set of builds rather than one archive per version, so an exact pin would stop
resolving the moment that build rotated out. A caller that needs to stay off a
newer series sets an exclusive ceiling — which is what keeps selection away from
a 3.15 release candidate sitting in the same index as the 3.12 builds it wants.

**`python3.12` is tried before `python3`.** On a machine with several
interpreters installed, `python3` is whatever the distribution decided, and it is
often older than the versioned binary sitting right next to it.

**Every build unpacks into a directory called `python`.** Whatever the version.
So the install directory in the cache is named from the asset rather than from
the archive's contents — otherwise every version would claim the same directory
and each install would silently replace the last.

**A pooled job cannot be isolated, and the module says so.** There is no worker
thread to run it in and no safe way to kill one, so jobs on a warm worker share
module state, `os.environ`, and logging configuration. The harness gives each job
fresh globals, captures output at the file-descriptor level — so `os.write(1,
...)`, subprocesses, and native extensions are captured too — and enforces a soft
deadline with `SIGALRM` on Unix. The router recycles workers after a job budget,
which bounds the leakage without eliminating it. That is why a host opts into
Python pooling rather than getting it by default.

## Using it

Load it alongside `tinyruntime`, which routes `python` to the well-known name
this module claims (`ai.tinyhumans.runtime.python.Provider`). A host then asks
the router to run Python and never addresses this module directly.

```rust
use tinyruntime_python::{DEFAULT_VERSION, parse_version, satisfies};

let installed = parse_version("Python 3.13.1").expect("a version");
assert!(satisfies(installed, DEFAULT_VERSION, None));
assert!(!satisfies(installed, DEFAULT_VERSION, Some("3.13")));
```

## Supported hosts

Whatever the standalone channel publishes: macOS, Linux, and Windows on x86-64
and ARM64. Anything else is refused by name rather than guessed at.

## Linking into a host

The default build exports the TinyBus C ABI for dynamic loading. The pinned
TinyBus gitlink (11a7d0d, merged PR #28) supplies `module_export_static!`. To link the
module into a Rust host, enable its `static-link` feature and pass the three
`linked` entries (`TINYBUS_MODULE_ABI_V1`, `tinybus_module_manifest_v1`, and
`tinybus_module_init_v1`) to the TinyBus linked-module host API. This uses the
same declaration and manifest as the dynamic build, with Rust-addressable
symbols that can coexist with other linked modules.

## Building

```sh
git submodule update --init --recursive
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

The harness suite in `tests/harness_protocol.rs` launches a real `python` and
drives the protocol end to end. It skips when the machine has none, so the suite
stays hermetic on a runner without Python.

See [`AGENTS.md`](AGENTS.md) for the working agreement, and
[`MODULE.md`](MODULE.md) for installing a release artifact.

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).

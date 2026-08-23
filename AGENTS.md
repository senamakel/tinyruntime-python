# Repository Guidelines

This file is the single source of truth for how humans and coding agents work
in this repository. `CLAUDE.md` is a symlink to this file, so every agent reads
the same instructions.

`tinyruntime-python` is the Python **provider** for `tinyruntime`. It supplies
the language knowledge the router does not have, and it deliberately does none
of the work the router does — see "This is a provider, not a runtime manager"
below before adding anything that touches the network or the filesystem.

## Project Structure

This is a Rust 2024 cargo workspace rooted at a virtual `Cargo.toml`. There is
one member: `crates/tinyruntime-python`, built as both an `rlib` and the
`cdylib` TinyBus loads.

```text
Cargo.toml              # virtual workspace: members, [workspace.package],
                        # [workspace.dependencies], [workspace.lints]
crates/
└── tinyruntime-python/
    ├── src/
    │   ├── lib.rs           # crate docs + public surface
    │   ├── error/           # crate-wide `Error` and `Result<T>`
    │   ├── version/         # floors, ceilings, and how Python spells versions
    │   ├── system/          # finding an interpreter the host already has
    │   ├── distribution/    # searching the standalone release index
    │   │   ├── host.rs      # the host-triple table
    │   │   └── index.rs     # index shape and selection, testable offline
    │   ├── layout/          # where an install keeps its interpreter
    │   ├── harness/         # the warm-worker harness
    │   │   └── pool_worker.py
    │   └── tinybus_module/  # TinyBus interface, ABI exports, integration tests
    ├── tests/               # integration tests, including the harness suite
    └── examples/            # runnable, compiled-in-CI usage examples
vendor/tinybus/             # pinned TinyBus host types and module SDK
vendor/tinyruntime/         # pinned wire contract (`tinyruntime-bus`)
docs/
├── specs/                  # behaviour and architecture specifications
├── plans/                  # test-first implementation plans
└── adr/                    # immutable architecture decision records
```

### This is a provider, not a runtime manager

`tinyruntime` — the router, in its own repository — owns everything that is the
same for every language: downloading an archive, verifying its digest, unpacking
it, promoting it into a cache atomically, reusing it on the next start, and
keeping a bounded set of warm interpreter processes in front of it.

This repository answers five questions about Python and nothing else:

| Member | What it answers |
| --- | --- |
| `Describe` | what this provider is and what it targets by default |
| `DetectSystem` | whether the host already has a usable interpreter |
| `SelectDistribution` | which standalone build to install for this machine |
| `Layout` | where the interpreter is inside an unpacked install |
| `Harness` | what a warm Python worker is |

**This module downloads nothing, installs nothing, and starts no worker.** The
one network call it is allowed to make is reading the release index, so the
distribution it names carries the digest the router verifies against. A change
that fetches an archive, writes to a cache, or spawns a worker here is a change
that belongs in the router instead.

The contract lives in `vendor/tinyruntime/crates/tinyruntime-bus`, vendored so
the router and every provider share one definition of these types. Do not define
a local copy of a payload type: a parallel set would mean a conversion at every
call site that nothing checks.

### Two things Python does that Node.js does not

**A request names a version floor, not an exact version.** The standalone
channel publishes a moving set of builds rather than one archive per version, so
an exact pin would stop resolving the moment that build rotated out. Selection
is therefore a search — filter to this host, filter to the range, then rank — and
that search must stay testable without a network.

**A pooled job cannot be isolated.** There is no worker thread to run it in and
no safe way to kill one, so jobs on a warm worker share module state,
`os.environ`, and logging configuration. The harness gives each job fresh globals
and captures output at the file-descriptor level; the router recycles workers
after a job budget. That bounds the leakage without eliminating it, which is why
a host opts into Python pooling rather than getting it by default. Do not
document or assume isolation this harness cannot provide.

### The wire contract

`vendor/tinyruntime` is registered as a git submodule and pinned by its gitlink.
Do not edit vendored code from this repository. Make contract changes in the
`tinyruntime` repository, push them there, then update this repository's gitlink
in a separate commit.

Each feature area belongs in a focused module directory under `src/`. A module
root explains the module, wires its pieces together, and exposes the smallest
useful API. Move substantial type definitions into `types.rs` and put
module-local unit tests in a dedicated `test.rs`, wired from the bottom of the
module root with:

```rust
#[cfg(test)]
mod test;
```

Do not accumulate inline `mod tests` blocks in implementation files, and do not
let a general-purpose `utils.rs` or `helpers.rs` grow — those are a symptom of a
missing module.

Keep public exports centralized in `src/lib.rs` so downstream users have one
predictable surface. Put shared error variants in `src/error/mod.rs` and return
the crate-wide `Result<T>` from fallible public APIs.

## Build And Test

Run every command from the repository root. These four are the contract; CI
runs exactly them, so a green local run should mean a green CI run.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

Supporting commands:

- `cargo fmt --all` — format before committing.
- `cargo test <filter>` — run a focused subset while iterating.
- `cargo test --test harness_protocol` — run the harness suite against a real
  `python`. It skips when the machine has none.
- `cargo run -p tinyruntime-python --example basic` — run the bundled example.
- `cargo doc --no-deps --all-features` — build the rustdoc CI also builds with
  `RUSTDOCFLAGS="-D warnings"`.
- `cargo test --doc` — run doctests alone when editing documentation examples.

Never skip, ignore, or delete a failing test to make a command pass. Fix the
root cause, or stop and report the blocker.

## Coding Style

Use standard `rustfmt` output and Rust 2024 idioms. Do not hand-format around
`rustfmt`, and do not add `#[rustfmt::skip]` without a comment explaining why.

- `snake_case` for modules, files, functions, methods, fields, and locals.
- `PascalCase` for types, traits, and enum variants; `SCREAMING_SNAKE_CASE` for
  constants and statics.
- Name things for what they are, not for their layer: `RetryPolicy`, not
  `RetryHelper`.
- Prefer small, typed APIs over stringly-typed ones. Accept `&str` and generic
  `impl Into<String>` at boundaries; return owned, concrete types.
- Keep the public surface minimal: default to private, and export deliberately
  from `src/lib.rs`.
- `unsafe` is forbidden workspace-wide by `[workspace.lints]` in the root
  `Cargo.toml`. If a project genuinely needs it, relax the lint in its own
  commit and document every invariant with a `// SAFETY:` comment.

### Errors

- One crate-wide `Error` enum per crate, in `src/error/mod.rs`, built with
  `thiserror`.
- Fallible public functions return `Result<T>`, the crate alias.
- Add a specific variant instead of stuffing context into a string; error
  messages are lowercase, without trailing punctuation.
- Do not `unwrap()`, `expect()`, or `panic!` in library code paths. They are
  fine in tests, examples, and genuinely unreachable states — where `expect`
  must carry a message explaining the invariant.
- Document a `# Errors` section on every public fallible function and a
  `# Panics` section on anything that can panic.

### Dependencies

Adding a dependency is a design decision. Before adding one, check whether the
standard library or an existing dependency already covers the need. When you do
add one:

- pin a caret range (`serde = "1"`), not an exact version;
- enable only the features you need, with `default-features = false` when that
  meaningfully trims the tree;
- gate anything optional behind a Cargo feature, documented in `Cargo.toml`;
- declare it once in the root `[workspace.dependencies]` when more than one
  crate needs it, and take it with `{ workspace = true }`;
- leave a comment above the entry explaining *why* the crate is needed and what
  uses it — see the existing entries for the expected tone;
- prefer well-maintained crates with a compatible license.

Keep `Cargo.lock` committed; this workspace ships a single lockfile so CI and
releases are reproducible.

### Vendored dependencies

TinyBus is registered as the `vendor/tinybus` git submodule and pinned by its
gitlink. It supplies the host types and module-side SDK required to build this
crate's `cdylib`. Initialize it after cloning with:

```sh
git submodule update --init --recursive
```

Do not edit vendored code from the parent repository. Make TinyBus changes in
its own repository, push them there, then update this repository's gitlink in a
separate commit. Keep the exact path dependencies and minimal features unless a
new module capability requires more.

## Testing

- Module-local unit tests live in `src/<feature>/test.rs` and may touch private
  items.
- The harness is the one part of this crate that is not Rust, so
  `tests/harness_protocol.rs` stands in for the router: it listens on loopback,
  launches the harness the way the router would, completes the handshake, and
  runs jobs through it. Any change to `pool_worker.py` needs a test there —
  especially anything touching descriptor capture or the working directory.
- Distribution selection is testable without a network, and must stay that way.
  `src/distribution/index.rs` holds the shape and the ranking; test it against a
  realistic index body including the assets that must be ignored.
- Integration tests live in `crates/<crate>/tests/` and exercise only the public
  API — they are the regression suite for the crate's contract.
- Payload types pin their serde representation in a unit test. That
  representation is the wire form: a host and a module that disagree about a
  field name fail at runtime with a decode error.
- Use descriptive, behavioral test names: `rejects_an_empty_name`, not
  `test_greet_2`.
- Cover the failure paths, not just the happy path. Every new error variant
  needs a test that produces it.
- For async behavior, standardize on one runtime (`tokio` as a dev-dependency
  for tests) rather than mixing runtimes.
- Tests must be deterministic and independent of network, wall-clock time, and
  execution order. Gate any live/network test behind a feature or an env var and
  name it `live_*` so it is easy to exclude.
- Maintain at least 90% line coverage in every source file. Add or update tests
  with every behavior change, and note any deliberately untested edge case in
  the pull request description.

Write the test first when fixing a bug: a failing test that reproduces the
report, then the fix that turns it green.

## Documentation

Write documentation for the reader who has never seen the code.

- Every public item gets a rustdoc comment. `missing_docs` is a warning that CI
  treats as an error.
- Start every `mod.rs` and `test.rs` with a concise module-level `//!`
  description.
- Each crate's `src/lib.rs` carries its crate-level overview: what the crate
  does, the primary entry points, and a short runnable example. It should also
  say what the crate deliberately does *not* hold, and why.
- Prefer concrete examples over vague description. Doc examples are compiled and
  run by `cargo test`, so they cannot drift.
- Complex modules must include a module-level `README.md` covering their design,
  public surface, and important operational constraints.
- Keep `README.md`, `docs/`, and module docs aligned with code changes in the
  same commit that changes behavior.
- Write accepted behavior and constraints in `docs/specs/` before creating a
  linked, implementation-ordered plan in `docs/plans/`. Specs define what and
  why; plans define how and in what sequence.
- Keep every Markdown file, including this one, at 500 lines or fewer. When a
  topic outgrows that, split it into focused files and link them from the
  nearest `README.md`.

## Git Workflow

- Never commit directly to `main`. Branch first, one branch per logical change.
- Do feature work in a git worktree so the main checkout stays clean.
- Commit subjects are concise and imperative: `Add retry policy to the client`.
  Keep the subject specific to the change and under ~72 characters.
- Make small, focused commits. Each commit should cover one logical change,
  build independently, and avoid mixing formatting, refactors, and behavior
  changes unless they are inseparable.
- Never commit secrets. `.env` is git-ignored; document new variables in
  `.env.example` with placeholder values.
- Never force-push a shared branch, rewrite published history, or bypass hooks
  with `--no-verify`.

## Pull Requests

Open pull requests ready for review, not as drafts, unless the work genuinely
must not merge yet. A pull request should:

- summarize what changed and why, in a few sentences;
- call out public API or behavior changes explicitly, or state "None";
- list the validation commands actually run, with their outcome;
- link the related issue;
- include updated tests, docs, and examples in the same change.

The template in `.github/PULL_REQUEST_TEMPLATE.md` encodes this checklist.
Address review feedback by fixing it, and reply on each thread describing what
changed. Do not resolve a thread whose feedback you have not addressed or
explicitly declined with a reason.

## Releases

Releases run from `.github/workflows/release.yml` via a manual
`workflow_dispatch` with a `patch` / `minor` / `major` bump; `current` resumes
an interrupted release after its version commit and tag exist. The workflow
re-runs the full validation suite, computes the next version, updates
the root `[workspace.package]` version and `Cargo.lock`, commits and tags
`vX.Y.Z`, builds `crates/tinyruntime-python` as a TinyBus module for every supported
platform, pushes, and creates an immutable GitHub release with installable
native packages.

Consequently:

- Do not hand-edit the `version` field in the root `[workspace.package]`; the
  release workflow owns it. Every member inherits it with
  `version.workspace = true`, so the whole workspace releases as one version.
- Follow semantic versioning. Any change to the public surface that is not
  purely additive is a breaking change and needs a major bump (pre-1.0: a minor
  bump).
- The module must be packageable for every release target — `main` should
  always be green.

## Agent Working Agreement

For automated contributors specifically:

1. **Read before writing.** Inspect the surrounding module and match its
   conventions, comment density, and idiom rather than importing a house style.
2. **Verify, do not assume.** Run the four contract commands and read their
   output before reporting a task complete. Report failures with the output;
   never claim a check passed that you did not run.
3. **Stay in scope.** Implement what was asked. Do not opportunistically
   refactor, reformat, upgrade dependencies, or "fix" unrelated code — raise it
   instead.
4. **No placeholders in delivered code.** No `todo!()`, no stubbed functions, no
   commented-out alternatives left behind. If something cannot be finished, say
   so explicitly.
5. **Do not weaken the guardrails.** Never add blanket `#[allow(...)]`, relax a
   lint, mark a test `#[ignore]`, or loosen CI to get a green run. Fix the
   cause.
6. **Secrets stay out.** Never read, echo, or commit `.env` contents, tokens, or
   credentials, and never paste them into a pull request or issue.
7. **Ask only when blocked.** Make routine judgment calls yourself; escalate
   only irreversible decisions or genuine forks with no clear default.

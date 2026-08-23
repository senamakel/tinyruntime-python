# Roadmap

What is deliberately not built yet, and what would have to be true before it is.

## Shipped

- Host interpreter detection, series-specific candidates first, bounded so a
  wedged binary costs a probe rather than a hang.
- Standalone build selection: filtered to this host and to the requested range,
  newest first, stripped builds preferred, tested without a network.
- Install layout, including the `python/` wrapper directory every build ships.
- The warm-worker harness: fresh globals per job, descriptor-level capture,
  `SIGALRM` soft deadlines, and a protocol a job cannot forge.

## Next

**Package installation.** `pip` is reported in the layout but nothing calls it.
Whether installing dependencies belongs behind a provider member or stays a host
concern is an open question in the contract, not here.

**Virtual environments.** A host that wants isolated dependencies per workload
currently builds that itself. The layout has everything needed to create one; the
question is whose job it is.

**Free-threaded builds.** The channel publishes `freethreaded` variants. They
would change the isolation story for pooled jobs considerably, which makes them
interesting — and makes them something to adopt deliberately rather than by
having selection pick one up.

## Not planned

**Downloading or installing anything here.** That is the router's half, and
duplicating it would give every language its own subtly different pipeline.

**Claiming isolation the harness cannot provide.** CPython cannot safely kill a
running thread. The honest position — bounded leakage, opt-in pooling — stays.

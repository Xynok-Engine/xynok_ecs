# xynok_ecs
[![discord invite link](https://img.shields.io/discord/1495504680711880714?logo=discord)](https://discord.gg/a2qzfrFzWT)
## Current Benchmark(v0.1.0)
![almost_faster_2x_bevy_ecs](assets/benchmark_v0.1.0.png)

## Features
| status | feature | description or note |
| --------------- | --------------- | --------------- |
| ✅ | register archetype | register an archetype without spawning any data |
| ✅ | create/destroy | create and destroy entities and their associated data |
| ✅ | add/remove/merge component | add, remove, and merge components for an entity. Merging will override existing components. |
| ✅ | tuple archetype | clients can create an archetype with varying numbers of components. Currently, the maximum is 16 components per archetype. |
| ✅ | query | an iterator for querying components from an archetype |
| canceled | Shared - Archetype Component | [This is the reason](https://youtu.be/k_RyU6QKQ-A) |
| in progress | system & scheduler | [issue_link](https://github.com/Xynok-Engine/xynok_ecs/issues/3)|
| todo | changed/added detection | the foundation for the observer pattern and architectures related to asset and resource pipelines (mesh, texture, sound, etc.) |
| canceled | persistent query | canceled: avoids the overhead of recreating every query whenever the structure changes. Currently, the query refresh process is spread across an entire frame. |
| todo | singleton |  |
| todo | Benchmark with flec | https://www.flecs.dev/flecs/md_docs_2Docs.html |
| todo | entity graph |  |
| Item1.2 | Item2.2 | Item3.2 |
| Item1.2 | Item2.2 | Item3.2 |


## Install
```toml
[dependencies]
xynok_ecs = { git = "https://github.com/Xynok-Engine/xynok_ecs.git", tag = "v0.1.15" }
```
## Concepts
To understand how the entire codebase works, you can check out these [videos](https://www.youtube.com/@xynok_youtube/playlists). I created them when I first started this repo. They explain the core concepts and the most important ideas that the codebase implements. I might update them in the future, but for now, they are a close match to the current state of the code.

## Examples
You can find all the examples in the [examples directory](./examples). To run an example:
**Cmd:** 
```bash
cargo run --example <name_of_rust_file_in_examples_folder>
```
**Example:** This command runs the file `examples/archetype.rs`.
```bash
cargo run --example archetype
```

## For Contributors

**step 1:** join the [discord](https://discord.gg/a2qzfrFzWT) server to get in touch with the maintainers.

**step 2:**: After that, you can track our progress on the [project board](https://github.com/orgs/Xynok-Engine/projects/1).
> [!IMPORTANT] 
> Only pick tasks that haven't been assigned yet.


### Tests
Storage-layout unit tests (chunk alignment, entity packing) live inline next to the code they
test in `src/` (`#[cfg(test)] mod test`), since they need private access that code outside the
crate can't have.

**Cmd:**
```bash
cargo test --lib
```

World-behavior tests (create/destroy, add/remove/merge component, drop glue, query, stress) are
integration tests under `tests/`, split by topic (`tests/create_destroy.rs`, `tests/chunk.rs`,
`tests/add_component.rs`, `tests/query.rs`, ...) with shared fixtures in `tests/common/mod.rs`.
They only use the crate's public API, plus a narrow read-only introspection module
(`xynok_ecs::world::testing`) gated behind the `test-util` Cargo feature, for checking storage
invariants the public API can't observe directly (row-swap mapping, chunk reuse, free-chunk
count). `Cargo.toml` already enables `test-util` for this crate's own `[dev-dependencies]`, so
no extra flags are needed to run them.

**Cmd:**
```bash
cargo test
```

### Benchmarks
`benches/` is a separate crate (`xynok_ecs_benches`) comparing single-threaded query iteration
against `bevy_ecs` and a plain `std::Vec` baseline, across every combination of query arity
(1, 2 or 3 components), archetype layout (1 archetype or 5) and entity count (1k, 10k, 100k).

Timing is done by [criterion](https://github.com/criterion-rs/criterion.rs), which picks the
iteration counts, runs the warm-up, collects the samples, bootstraps the confidence intervals,
classifies outliers and compares each run against the previous one on disk. Memory is a different
question and a stopwatch is the wrong instrument for it, so a second binary measures that through
a counting global allocator over the same workload:

- **footprint**: bytes still held once the storage is built, and what that works out to per entity
- **setup allocation**: every byte requested while building, and how many allocator calls it took
- **query-loop allocation**: bytes allocated inside the pass criterion times (must be 0, otherwise
  the timing is not iteration-only)
- **leak**: live bytes still held after the storage is dropped (must be 0)

The report binary joins the two and writes `benches/output/results.json` plus
`benches/output/report.html`, a self-contained page with the comparison table and charts. It exits
non-zero if any scenario allocates in the timed loop or leaks, so it works as a CI check too.
Criterion's own report lands at `target/criterion/report/index.html`.

**Cmd:**
```bash
./benches/scripts/bench.sh          # bench, then report
./benches/scripts/bench.sh 1k       # only the benchmarks whose id contains "1k"
```

Or the two steps by hand (the report needs `--release`; a debug build makes the numbers meaningless):
```bash
cargo bench -p xynok_ecs_benches --bench query
cargo run --release -p xynok_ecs_benches --bin report
```


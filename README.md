# xynok_ecs
[![discord invite link](https://img.shields.io/discord/1495504680711880714?logo=discord)](https://discord.gg/a2qzfrFzWT)

## Features
| status | feature | description |
| --------------- | --------------- | --------------- |
| ✅ | register archetype | register an archetype without spawning any data |
| ✅ | create/destroy | create and destroy entities and their associated data |
| ✅ | add/remove/merge component | add, remove, and merge components for an entity. Merging will override existing components. |
| ✅ | tuple archetype | clients can create an archetype with varying numbers of components. Currently, the maximum is 16 components per archetype. |
| ✅ | query | an iterator for querying components from an archetype |
| in progress | Shared - Archetype Component | A component shared by an archetype. It uses a single allocation and persists for the lifetime of that archetype. |
| in progress | changed/added detection | the foundation for the observer pattern and architectures related to asset and resource pipelines (mesh, texture, sound, etc.) |
| todo | persistent query | queries can have a static lifetime, initialized once and reused indefinitely |
| todo | system & scheduler | a distributed modifier component system where users can register or remove systems for scheduling |
| Item1.2 | Item2.2 | Item3.2 |
| Item1.2 | Item2.2 | Item3.2 |
| Item1.2 | Item2.2 | Item3.2 |


## Concepts
To understand how the entire codebase works, you can check out these [videos](https://www.youtube.com/@xynok_youtube/playlists). I created them when I first started this repo. They explain the core concepts and the most important ideas that the codebase implements. I might update them in the future, but for now, they are a close match to the current state of the code.


## For Contributors
### Examples
**Cmd:** 
```bash
cargo run --example <name_of_rust_file_in_examples_folder>
```
**Example:** This command runs the file `examples/archetype.rs`.
```bash
cargo run --example archetype
```

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
against `bevy_ecs` and a plain `std::Vec` baseline — named `query_single_thread` since
`xynok_ecs` has no system scheduler or parallelism yet. For every entity count it measures:
- **allocation**: bytes/allocations made while creating the entities (setup only)
- **speed**: time per query pass, timed only after a warmup and with the sample buffer
  pre-allocated, so no allocation from the harness itself can leak into the measurement
- **leak**: live bytes still allocated after the storage is dropped (should be 0)

Results print as a table in the terminal and are also written to `benches/output/results.json`
and `benches/output/report.html` (a self-contained page with charts comparing all three).

**Cmd:** (always use `--release`; a debug build makes the speed numbers meaningless)
```bash
cargo run --release -p xynok_ecs_benches
```



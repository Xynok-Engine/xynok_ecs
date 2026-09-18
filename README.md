# xynok_ecs
[![discord invite link](https://img.shields.io/discord/1495504680711880714?logo=discord)](https://discord.gg/a2qzfrFzWT)

## Preface

This repository contains a lightweight ECS library designed specifically for the Xynok engine. My goal is not to build a feature-rich, general-purpose ECS, but rather to provide a lean implementation that covers the essential requirements for my engine's architecture.

## Overview

The library focuses on providing the fundamental building blocks necessary for entity and component management. Currently, it supports:

- Creating and destroying entities
- Adding, removing and merging components
- Querying entities by the components they carry, with filters for added, changed, enabled or disabled state
- Writing systems as plain functions, and running them yourself through a simple scheduler, one at a time or as a parallel group
- Making structural changes from inside a system through `Cmd`, safely, even while a parallel group is running

Notably, this library lacks complex abstractions such as job graphs or automatic parallelization. All operations must be scheduled manually by the developer.

## Getting started

Describe a component, spawn an entity, then read it back:

```rust
use xynok_ecs::component;
use xynok_ecs::world::World;

#[component]
#[derive(Debug, Default)]
struct Hp(u32);

#[component]
#[derive(Debug, Default)]
struct Mana(u32);

let mut world = World::default();
world.create((Hp(100), Mana(10)));

for (hp, mana) in world.create_query::<(&mut Hp, &Mana)>().into_iter()
{
    hp.0 += mana.0;
}
```

A system is a plain function that asks for what it needs. `Query` reads and writes components,
`Cmd` creates entities, destroys them, or changes which components they carry:

```rust
use xynok_ecs::cmd_buffer::cmd::Cmd;
use xynok_ecs::entity::Entity;
use xynok_ecs::query::Query;

fn regen(q: Query<&mut Mana>)
{
    for mana in q.into_iter()
    {
        mana.0 += 1;
    }
}

fn respawn(mut cmd: Cmd, q: Query<(&Entity, &Hp)>)
{
    for (e, hp) in q.into_iter()
    {
        if hp.0 == 0
        {
            cmd.destroy(*e);
            cmd.create((Hp(100), Mana(10)));
        }
    }
}
```

Hand the world to the scheduler, register the systems, and decide yourself when each session
runs:

```rust
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_std::unsafe_ptr::HeapPtr;

let mut world = HeapPtr::new(World::default());
world.create((Hp(0), Mana(10)));

let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
scheduler.add_system(DefaultScheduleSession::Update, regen);
scheduler.add_system(DefaultScheduleSession::Update, respawn);

loop
{
    scheduler.run(DefaultScheduleSession::Update);
    world.sync_point(); // applies whatever `Cmd` left queued
}
```

From here, the [overview](docs/xynok_ecs_overview.md) walks through queries, filters, change
detection, parallel groups and `Cmd` in order, each with a runnable example.

## Docs
- [details docs at here](docs/xynok_ecs_overview.md)

## Performance and Benchmarks

I have included a benchmark comparison against [Bevy](https://github.com/bevyengine/bevy), but I want to be clear about what these numbers represent. While the current benchmarks show this library outperforming Bevy in certain scenarios, this is a result of its simplicity rather than superior architectural optimization. 

Because this library implements a significantly smaller feature set compared to Bevy, it naturally incurs less overhead. It is not "faster" in the sense of being a more efficient implementation of equivalent features.


| Single Thread | Multi-threaded |
| -------------- | --------------- |
| ![a](assets/single_thread_v0.1.29.png) | ![a](assets/multi_thread_v0.1.29.png) |


## Current Limitations

This project is still in its early stages. Please keep the following in mind:

- The library has not been tested across a wide variety of hardware configurations.
- Testing & benchmarks has been confined to basic use cases.
- It has not yet been subjected to complex, real-world stress tests.


## Install
```toml
[dependencies]
xynok_ecs = { git = "https://github.com/Xynok-Engine/xynok_ecs.git" }
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

## Benchmarks

**Cmd:**
```bash
./benches/scripts/bench.sh                              # both benches, then the report
./benches/scripts/bench.sh 1k                           # the same, only ids containing "1k"
cargo bench -p xynok_ecs_benches --bench query          # single-threaded timings
cargo bench -p xynok_ecs_benches --bench parallel       # multi-threaded timings
cargo run --release -p xynok_ecs_benches --bin report   # memory + report, from the timings above
```
The report binary joins the two and writes `benches/output/results.json` plus
`benches/output/report.html`, a self-contained page with the comparison table and charts. It exits
non-zero if any scenario allocates in the timed loop or leaks, so it works as a CI check too.
Criterion's own report lands at `target/criterion/report/index.html`.


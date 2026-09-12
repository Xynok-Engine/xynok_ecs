---
title: Xynok Ecs Overview
excerpt: An overview of the features and architecture of Xynok ECS
cover img: "../images/xynok_ecs_overview.png"
tags:
  - ecs
  - tutorial
---

## Overview
It is important to note that these features are still in their early stages of development. You might encounter bugs or performance issues as we continue to benchmark and refine the implementation.
also suggest using Bevy ECS if you need more features, extensive documentation, and a larger community.

This page walks through the whole library from the outside in: what it can do, how the pieces
fit together, and then a set of small runnable snippets. Every snippet here has a matching file
in [`examples/`](../examples), so you can always run the real thing:

```bash
cargo run --example query
```

## Features

- **Archetype storage.** Entities that carry the same set of components live together in fixed
  16 KB chunks, so a query walks contiguous memory instead of chasing pointers.
- **Entity lifecycle.** `create`, `destroy`, `exists`. Ids are recycled with a version counter,
  so a stale `Entity` never points at the entity that took its slot.
- **Structural changes.** `add_component`, `remove_component`, `merge_component`, on a single
  component or on a tuple of them.
- **Queries.** `&T` and `&mut T`, alone or in tuples, iterated across every archetype that
  matches.
- **Query filters.** `Added`, `Changed`, `Enabled`, `Disabled` for state, and `Without` to
  exclude an archetype entirely.
- **Per-system change detection.** Each system gets its own baseline tick, so a system running
  every 10 frames still sees every change exactly once.
- **Systems and a demo scheduler.** Plain functions taking `Query` parameters, grouped into
  named sessions, run sequentially or in parallel.
- **Compile-time-ish safety checks.** Conflicting access inside a query, between two parameters
  of one system, or between two systems of a parallel group is rejected at the registration
  call site, not silently raced at runtime.

What it does not have: job graphs, automatic parallelization, resources/singletons, relations,
serialization, or a reflection layer. Those are on you.

## Architecture

### World

`World` owns everything: the entity allocator, the archetype table, the chunk memory, and the
global tick used for change detection. You create one, spawn into it, and query out of it.

```rust
use xynok_ecs::world::World;

let mut world = World::default();
```

Most calls take `&mut self`, including `create_query`, because building a query closes the
current round of change detection. If you hand the world to a scheduler, the scheduler owns it
from then on.

### Entity

An `Entity` is a single `u64`: 40 bits of index plus 24 bits of version. The index says which
slot the entity sits in, the version says how many times that slot has been reused.

```rust
let a = world.create(Hp(10));
world.destroy(a);
world.exists(a); // false, even after a new entity takes the same slot
```

When a slot is recycled the version goes up, so the old handle stops matching. That is the
whole trick behind `exists`, and it is why you can keep `Entity` values around without fear.

### Component

A component is any type marked with the `#[component]` attribute:

```rust
use xynok_ecs::component;

#[component]
#[derive(Debug, Default)]
struct Hp(i32);
```

By default a component carries no state tracking. If you want the state filters to work on it,
say so in the attribute:

```rust
#[component(EnableAble)]              // supports Enabled / Disabled
struct Visible(bool);

#[component(ChangeAble)]              // supports Added / Changed
struct Position(f32);

#[component(EnableAble, ChangeAble)]  // both
struct Hp(i32);
```

Tracking costs memory per row, which is why it is opt in. Asking for a filter a component was
not declared for is an error, not a silent empty result.

### Chunk

A chunk is a 16 KB block holding rows of one archetype. Inside it, components are stored
column by column: all the `Hp` values together, then all the `Mana` values, and so on. The
entity ids live in their own column too.

State tracking (enable bit, added tick, changed tick) does not sit next to the value. It lives
in a separate region of the same chunk, so a query that never asks about state never pays to
load those bytes.

How many rows fit depends on how wide the archetype is: a narrow archetype gets thousands of
rows per chunk, a wide one fewer. An archetype whose components add up to more than 16 KB per
row is rejected outright.

Removal is a swap-remove: the last row moves into the freed slot, and its state moves with it.
Rows stay packed, so iteration never has to skip holes.

### Archetype

An archetype is one exact set of component types, and it owns a list of chunks. Spawning
`(Hp, Mana)` and spawning `(Hp, Mana, Poison)` gives you two different archetypes, and a
`Query<&Hp>` walks both.

Adding or removing a component changes which archetype an entity belongs to, so the row gets
moved. That is not free, which is the usual ECS advice: prefer toggling a flag or an enable bit
over adding and removing components every frame.

### System

A system is a plain function whose parameters are queries:

```rust
use xynok_ecs::query::Query;

fn regen_mana(query: Query<&mut Mana>)
{
    for mut mana in query
    {
        mana.0 = (mana.0 + 5).min(50);
    }
}
```

No trait to implement, no macro on the function. A system with no parameters is fine too, it
just never touches storage.

The parameters of one system are checked against each other when you register it. `&mut Hp`
next to `&Poison` is fine. `&mut Hp` next to another `&Hp` is not, and it panics at the
`add_system` call rather than racing later.

## Example to use

### Create new entity

`world.create` takes one component or a tuple of them, and hands back the `Entity`.

```rust
let a = world.create((Hp(100), Mana(10)));
let b = world.create(Hp(50)); // no Mana: a different archetype from `a`

println!("{a} exists: {}", world.exists(a));
world.destroy(a);
println!("{a} exists: {}", world.exists(a)); // false
```

Runnable version: [`examples/archetype_creation.rs`](../examples/archetype_creation.rs).

### Modify component value and structural change

Changing a value is just a write through a `&mut` query. Changing the *shape* of an entity is
`add_component`, `remove_component`, or `merge_component`.

```rust
let a = world.create((Hp(10), Mana(0)));

// remove hands the value back to you
let mut hp = world.remove_component::<Hp>(a);
hp.0 = 100;
world.add_component(a, hp);

// tuples work everywhere a single component does
let (hp, mana) = world.remove_component::<(Hp, Mana)>(a);
```

`add_component` expects the component to be absent. `merge_component` does not care: if it is
already there the old value is overwritten, if it is not it gets added.

```rust
world.merge_component(a, Mana(30));            // added, `a` had no Mana
world.merge_component(a, Hp(999));             // overwritten
world.merge_component(a, (Hp(1), MoveSpeed(7))); // one of each, in one call
```

Runnable versions: [`examples/add_remove_component.rs`](../examples/add_remove_component.rs)
and [`examples/merge_component.rs`](../examples/merge_component.rs).

### Query

`create_query` gives you something you can iterate directly.

```rust
let a = world.create((Hp(100), Mana(10)));
let b = world.create((Hp(80), Mana(5)));
let c = world.create(Hp(50)); // no Mana

// every entity carrying Hp, across every archetype that has it: a, b, c
for hp in world.create_query::<&Hp>()
{
    println!("hp: {hp:?}");
}

// only entities carrying both: a and b, c is skipped
for (hp, mana) in world.create_query::<(&Hp, &Mana)>()
{
    println!("hp: {hp:?}, mana: {mana:?}");
}

// mutate in place, and mix a write with a read in the same pass
for (mut hp, mana) in world.create_query::<(&mut Hp, &Mana)>()
{
    hp.0 += mana.0;
}
```

A query names components, never archetypes. You never register a combination anywhere: spawn a
new mix of components and the existing queries pick it up on their next run.

Naming the same component twice in one query, for example `(&mut Hp, &Hp)`, is a conflict and
panics at the `create_query` call.

Runnable version: [`examples/query.rs`](../examples/query.rs).

#### Query filters

Filters live in `xynok_ecs::query::filter` and wrap the column they apply to.

| Filter | Yields a row when | Needs |
| --- | --- | --- |
| `Added<&T>` | `T` was inserted since this query's previous run | `ChangeAble` |
| `Changed<&T>` | `T` was written since this query's previous run | `ChangeAble` |
| `Enabled<&T>` | `T`'s enable bit is currently true | `EnableAble` |
| `Disabled<&T>` | `T`'s enable bit is currently false | `EnableAble` |
| `Without<T>` | the archetype does not carry `T` at all | nothing |

```rust
use xynok_ecs::query::filter::{Added, Changed, Disabled, Enabled, Without};

fn report_added(query: Query<Added<&Hp>>) { /* only fresh inserts */ }
fn report_changed(query: Query<Changed<&Hp>>) { /* only rows written since last run */ }

// mix a filter with a plain column: read Mana only for entities whose Hp is enabled
fn heal(query: Query<(&mut Mana, Enabled<&Hp>)>) { }

// Without names the component itself, not a reference, and hands out nothing
fn thaw(query: Query<(&Hp, Without<Frozen>)>)
{
    for (hp, _) in query { }
}
```

Two things worth knowing about `Added` and `Changed`:

- The baseline is *this query's own* previous run, not some global instant. A system running
  every 10 frames still sees every change exactly once.
- `Changed` fires on an actual write. Reading a row out of a `&mut` query does not count: the
  tick is stamped when you deref it mutably, not when the iterator hands it to you.

The enable bit defaults to true. To spawn an entity with a component already disabled, use the
wrappers from `xynok_ecs::wrapper`:

```rust
use xynok_ecs::wrapper::{Disable, Enable};

let hero  = world.create((Enable::new(Hp(100)), Mana(10))); // explicitly enabled
let ghost = world.create((Disable::new(Hp(0)), Mana(0)));   // starts disabled
let mage  = world.create((Hp(80), Mana(40)));               // default: enabled
```

`Without` is the cheapest filter here. It never reads anything, it just drops the archetypes
that carry the named component before iteration starts, so it costs nothing per row. On its
own, `Query<Without<Frozen>>` names no column to walk and yields nothing.

Runnable versions: [`examples/query_state.rs`](../examples/query_state.rs) and
[`examples/query_state_tuple.rs`](../examples/query_state_tuple.rs).

### System and Scheduler

`DefaultScheduler` takes ownership of the world and runs systems grouped into sessions:
`Start`, `PreUpdate`, `Update`, `LateUpdate`, `PreFixedUpdate`, `FixedUpdate`,
`LateFixedUpdate`, `AppQuit`. You decide when each session runs.

```rust
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_std::unsafe_ptr::HeapPtr;

// spawn everything before handing the world over
let mut world = HeapPtr::new(World::default());
world.create((Name("hero"), Hp(100), Mana(10)));

let mut scheduler = DefaultScheduler::new(world);
scheduler
    .add_system(DefaultScheduleSession::Start, announce_start)
    .add_system(DefaultScheduleSession::Update, regen_mana)
    .add_system(DefaultScheduleSession::Update, tick_poison)
    .add_system(DefaultScheduleSession::LateUpdate, report);

scheduler.run(DefaultScheduleSession::Start);
loop
{
    scheduler.run(DefaultScheduleSession::Update);
    scheduler.run(DefaultScheduleSession::LateUpdate);
}
```

Inside a session, systems run in the order you added them, sequentially, on the calling thread.
So a `&mut` query in one system is visible to the next one in the same session. A session with
nothing registered is skipped and costs nothing.

For parallelism, hand a whole group to `add_system_parallel`:

```rust
scheduler
    .add_system_parallel(DefaultScheduleSession::PreUpdate, (survey_speed, survey_spread))
    .add_system_parallel(DefaultScheduleSession::Update, (integrate, tick_poison, regen_mana));
```

Every system in a group starts at once, and the group is done when the slowest one finishes.
There is no ordering inside a group, only between groups. The scheduler checks the access
scopes when you register the group, so two systems that both write `Mana` are rejected right
at the `add_system_parallel` call with `ParallelGroupConflict`. Shared reads never conflict, so
any number of systems can read the same component together. A group of one skips the thread
pool and runs inline.

Runnable versions: [`examples/single_thread.rs`](../examples/single_thread.rs) and
[`examples/multi_thread.rs`](../examples/multi_thread.rs).

Just to be clear, the scheduler is currently a demo. You should implement your own if you want the looper to support more complex behaviors or to manage the system lifecycle more effectively.

If you do write your own, the two hooks you need are on `World`: `capture_current_tick()` gives
you a baseline to store per system, and `create_query_since(baseline)` builds a query against
it. That is exactly how `Added` and `Changed` stay per system.

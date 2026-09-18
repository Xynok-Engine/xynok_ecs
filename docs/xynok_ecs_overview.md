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

- **Archetype storage.** Entities that carry the same set of components live together in chunks
  of 16 KB by default, so a query walks contiguous memory instead of chasing pointers. You can
  pick another chunk size, or lock an archetype so its entities never change shape.
- **Entity lifecycle.** `create`, `destroy`, `exists`. Ids are recycled with a version counter,
  so a stale `Entity` never points at the entity that took its slot.
- **Structural changes.** `add_component`, `remove_component`, `merge_component`, on a single
  component or on a tuple of them.
- **Queries.** `&T` and `&mut T`, alone or in tuples, iterated across every archetype that
  matches. Add `&Entity` to get the handle of each row too.
- **Several ways to walk a query.** One entity at a time, one chunk at a time, in batches of a
  size you choose, or spread over threads with `par_for_each_chunk` / `par_for_each_batch`.
- **Query filters.** `Added`, `Changed`, `Enabled`, `Disabled` for state, and `Without` to
  exclude an archetype entirely. `Detail<&mut T>` keeps every row and lets a system read and
  flip the enable bit itself.
- **Singletons.** An archetype that holds one entity at most, for the one-of-a-kind things like
  game settings or the current input state.
- **Per-system change detection.** Each system gets its own baseline tick, so a system running
  every 10 frames still sees every change exactly once.
- **Systems and a demo scheduler.** Plain functions taking `Query` parameters, grouped into
  named sessions, run sequentially or in parallel.
- **Commands from systems.** `Cmd` lets a system create entities, destroy them, or change their
  components, safely, whether the system runs alone or in a parallel group.
- **Compile-time-ish safety checks.** Conflicting access inside a query, between two parameters
  of one system, or between two systems of a parallel group is rejected at the registration
  call site, not silently raced at runtime.

What it does not have: job graphs, automatic system parallelization (you say which systems run
together), relations, serialization, or a reflection layer. Those are on you.

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

A chunk is a block of memory holding rows of one archetype, 16 KB by default and configurable
per archetype. Inside it, components are stored
column by column: all the `Hp` values together, then all the `Mana` values, and so on. The
entity ids live in their own column too.

State tracking (enable bit, added tick, changed tick) does not sit next to the value. It lives
in a separate region of the same chunk, so a query that never asks about state never pays to
load those bytes.

How many rows fit depends on how wide the archetype is: a narrow archetype gets thousands of
rows per chunk, a wide one fewer. An archetype whose components add up to more than one chunk per
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

#### Querying the entity itself

Need to know *which* entity a row belongs to? Put `&Entity` in the query, next to your
components:

```rust
use xynok_ecs::entity::Entity;

for (e, mut hp, mana) in world.create_query::<(&Entity, &mut Hp, &Mana)>()
{
    hp.0 += mana.0;
    println!("{e} now has {} hp", hp.0);
}

// on its own, it walks every entity in the world
for e in world.create_query::<&Entity>() { }
```

`Entity` is not a component. Every chunk already keeps its entity handles in its header, and
`&Entity` just reads them from there. A few things follow from that:

- It is read-only. `Query<&mut Entity>` does not compile, because rewriting a handle in place
  would break the mapping the world keeps from handles to rows.
- Filters do not apply to it. `Changed<&Entity>` or `Enabled<&Entity>` do not compile either,
  since there is no state to check. Put the filter on a real component instead, for example
  `(&Entity, Changed<&Hp>)`.
- It does not touch any component, so it never conflicts with other queries in the scheduler.
- `world.create(some_entity)` does not compile. An entity cannot be stored as a component.

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

#### Choosing how to walk a query

`for x in query` and `query.iter()` give you one entity at a time, which is what you want most
of the time. When you need more control over how the work is shaped, there are three more ways:

```rust
let mut query = world.create_query::<(&mut Position, &Velocity)>();

// one chunk at a time, row count known before you start
for mut chunk in query.iter_chunk()
{
    println!("{} rows", chunk.len());
    for (pos, vel) in chunk.iter()
    {
        pos.0 += vel.0;
    }
}

// batches of about 64 rows, crossing chunk and archetype borders as needed
for batch in query.iter_batch(64)
{
    for (pos, vel) in batch { pos.0 += vel.0; }
}
```

A batch covers rows nobody else's batch covers, and it is `Send`, so you can hand each one to a
thread yourself. If you would rather not deal with threads, the query does it for you:

```rust
query.par_for_each_chunk(|(pos, vel)| pos.0 += vel.0);
query.par_for_each_batch(64, |(pos, vel)| pos.0 += vel.0);
```

Both return once all the work is done. `par_for_each_chunk` makes one job per chunk, which is a
good default. Reach for `par_for_each_batch` when your chunks hold too many rows to split the
work well, or so few that a job per chunk is not worth it. The closure runs on several threads
at once, so it has to be `Fn + Send + Sync`, and you cannot count on rows being visited in any
particular order. With no executor set on the world, both fall back to walking on the calling
thread, so the result is the same, only slower.

Runnable version: [`examples/query_iter.rs`](../examples/query_iter.rs).

#### Reading and flipping the enable bit

`Enabled<&T>` and `Disabled<&T>` pick rows for you. When a system wants to decide the bit
instead, use `Detail<&mut T>`: it keeps every row, enabled or not, and lets you read and set the
bit yourself.

```rust
use xynok_ecs::query::detail::Detail;

fn hide_the_dead(query: Query<(&Hp, Detail<&mut Mesh>)>)
{
    for (hp, mut mesh) in query
    {
        mesh.set_enabled(hp.0 > 0);
    }
}
```

The item reads like the component itself, so `mesh.vertex_count` still works, and
`mesh.value_mut()` gives you the usual mutable access. `Detail<&T>` can only read the bit, since
two systems are allowed to read the same component side by side and letting them both write
would be a race. Flipping the bit is not a value change, so `Changed<T>` stays quiet. As with
the state filters, `T` has to be declared `EnableAble`.

### Singleton

Some things exist once and only once: the game settings, the current input state, the score.
`create_singleton` puts them in an archetype that holds one entity at most.

```rust
#[component]
#[derive(Debug, Default)]
struct Score(u32);

let settings = world.create_singleton((Score(0), Difficulty::Hard));
```

Trying to spawn a second one fails, and so does a plain `create` of that same set of components.
Destroy the entity and you are free to spawn it again.

A singleton is queried like anything else, there is no special accessor:

```rust
for score in world.create_query::<&mut Score>()
{
    score.0 += 1;
}
```

Entities cannot move in or out of a singleton, so `add_component` and `remove_component` on it
are rejected, and so is a `merge_component` that would add something new. A `merge_component`
that only overwrites values it already has is fine.

If you want to declare the singleton up front, before anything spawns, use
`world.register_singleton::<(Score, Difficulty)>()`. Registering twice does nothing. The one
thing that fails is asking for a singleton of a component set that already exists as a regular
archetype, since an archetype cannot change kind once entities live in it.

### Archetype configuration

Registering an archetype yourself is optional, `create` does it for you. Do it when you want to
change the defaults:

```rust
use xynok_ecs::apis::ArchetypeCfg;

world.register_archetype::<(Hp, Mana)>(ArchetypeCfg {
    chunk_size_in_byte:     64 * 1024,
    allow_structure_change: false,
});
```

`chunk_size_in_byte` decides how many entities sit together in one block. It is also the unit a
parallel query hands to one thread, so a bigger chunk means fewer, larger jobs.

`allow_structure_change: false` locks the shape of the entities in this archetype. Anything that
would move an entity in or out of it is rejected instead of silently costing you a copy, which
is useful for the hot archetypes you never intend to reshape at runtime.

Register the same archetype again with different settings and you get an error, rather than one
half of your code quietly running with the other half's settings.

### System and Scheduler

> [!IMPORTANT]
> Just to be clear, the scheduler is currently a demo. You should implement your own if you want the looper to support more complex behaviors or to manage the system lifecycle more effectively.

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

A group counts as one step for change detection too, so every system in it shares one tick.
Nobody inside the group can see what another member writes anyway (that would be a conflict),
so giving each of them a different tick buys nothing. What you get from it:

- A system after the group sees everything the group wrote, just like after a single system.
- A system in the group never sees its own writes from the previous run as `Changed`, no matter
  how the threads happened to interleave.

Runnable versions: [`examples/single_thread.rs`](../examples/single_thread.rs) and
[`examples/multi_thread.rs`](../examples/multi_thread.rs).


If you do write your own, the two hooks you need are on `World`: `capture_current_tick()` gives
you a baseline to store per system, and `create_query_since(baseline)` builds a query against
it. That is exactly how `Added` and `Changed` stay per system.

### Cmd

`Cmd` is how a system creates entities, destroys them, or changes which components they carry.
Add it to the system's parameters and use it like `World`:

```rust
use xynok_ecs::cmd_buffer::cmd::Cmd;

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

Available calls: `create`, `destroy`, `add_component`, `remove_component`, `merge_component`.
Each one panics on failure and has a `try_` twin that returns a `Result` if you would rather
handle the error yourself. Singletons and archetype registration are not part of `Cmd`, they are
setup work you do on the `World` before handing it to the scheduler.

#### Why not call `World` directly?

A system only ever receives `Query` and `Cmd` parameters, so `Cmd` is the way in. That is on
purpose: adding or removing a component moves the entity to another archetype, which is exactly
the memory a query may be walking at that moment. The nice part is that you write the same code
whether the system runs alone or inside a parallel group.

#### When the change becomes visible

This is the one rule worth remembering:

- A system running **on its own** sees its changes applied immediately. The next system in the
  session already sees them.
- A system running **inside a parallel group** does not. The request is queued and applied when
  the world reaches a sync point.

So inside a parallel group, do not create an entity and then expect a `Query` in the same step
to find it. Write the change, move on, read it back in a later system.

#### Applying queued changes

`World::sync_point()` applies everything that is still queued:

```rust
scheduler.run(DefaultScheduleSession::Update);
world.sync_point();
```

Call it at a point in your frame where no system is running, usually right after a session. If
you would rather not think about it, use a scheduler with `AUTO_SYNC_POINT` turned on and it
will do this at the end of every `run`. `DefaultScheduler` leaves it off so the timing stays
yours.

Within one system, queued changes are applied in the order you wrote them, so `create` then
`add_component` on that same entity behaves as you would expect. Across two systems of the same
parallel group there is no order, just as there is no order between the systems themselves. If
two of them fight over the same entity, the result depends on who got there first.

#### Working with an entity you just created

`create` hands back a usable `Entity` right away, even when the change is still queued. You can
store it, pass it around, or keep building on it:

```rust
fn spawn_boss(mut cmd: Cmd)
{
    let boss = cmd.create(Hp(500));
    cmd.add_component(boss, Armor(20));
}
```

What you cannot do yet is read its components, since they only exist after the sync point.

One call behaves differently depending on the two cases above: `remove_component` returns
`Some(value)` when the change is applied immediately, and `None` when it is queued. In the
queued case the component value is dropped for you.

#### If something goes wrong later

A queued change can still fail when it is applied, for example if you add a component to an
entity another system destroyed in the same step. Since the original call already returned, the
error surfaces at `sync_point()` instead, as a panic naming the system's thread. When you see
one, look for two systems in the same parallel group touching the same entity.

See [`tests/cmd_buffer.rs`](../tests/cmd_buffer.rs) for the full set of cases.

# Shared components

A shared component has an immutable identity key and a cloneable payload. Each archetype stores one payload per shared component type, regardless of its entity or chunk count. Archetypes match by their normal component layout and **all** shared keys.

```rust
use xynok_ecs::{component, shared::TSharedComponent, world::World};

#[component]
struct Hp(u32);
struct Mesh;
impl TSharedComponent for Mesh {
    type Key = u32;
    type Data = Vec<f32>;
}

let mut world = World::default();
let entity = world.create(Hp(100));
world.add_shared::<Mesh>(entity, 7, vec![0.0, 1.0]).unwrap();

let mut query = world.create_shared_query::<&Hp, &mut Mesh>();
for (mut mesh, mut archetype) in query.iter_archetype() {
    assert_eq!(*mesh.key(), 7);
    mesh.data_mut().push(2.0);
    for chunk in archetype.iter_chunk() {
        for hp in chunk {
            assert_eq!(hp.0, 100);
        }
    }
}
```

`Query<T, S, F>` uses `T` for normal components, `S` for shared access, and `F` for an optional static key filter. Defaults preserve `Query<T>`. Shared parameters support `&Mesh`, `&mut Mesh`, and tuples of up to four parameters. `Query<(), &Mesh>` accesses shared components without requesting any normal component. Define shared types with `TSharedComponent`; the old `#[component(shared)]` placeholder is rejected instead of silently storing a normal component.

`query.iter()` and `for value in query` yield only normal components. `iter_archetype()` yields shared views once per nonempty matching archetype. `SharedRef` exposes `key()` and `data()`; `SharedMut` additionally exposes `data_mut()`. Neither exposes mutable access to the key or the complete entry.

## Filters and groups

Runtime filtering borrows the query and can be chained:

```rust,ignore
let mut filtered = query.filter_shared::<Mesh>(&7);
for hp in filtered.iter() { /* normal components */ }
```

A static filter implements `TSharedFilter` with `SharedComponents = Mesh` and `filter_key() -> Mesh::Key`. Use it in a system's `Query<&Hp, &Mesh, GrassMesh>`, or call `world.create_filtered_query::<&Hp, &Mesh, GrassMesh>()`. The world caches the filter key once per filter type across query preparations and frames.

`query.group_by_shared::<Mesh>()` yields `(owned_key, group)` pairs. `group.iter()` yields normal rows across matching archetypes; `group.iter_archetype()` yields each archetype's own shared view and chunks. A group intentionally has no single payload: equal keys in different archetypes may have different payloads. Filtering or grouping on a shared type absent from `S` panics.

## Structural changes and lifetime

- `add_shared::<Mesh>(entity, key, data)` adds a shared component. An existing destination keeps its current payload; otherwise the supplied data initializes the new entry.
- `remove_shared::<Mesh>(entity)` removes it from that entity.
- `set_shared::<Mesh>(entity, key)` moves to an existing destination with the full layout and all other keys unchanged. It returns `SharedError::UnresolvedKey` if that destination does not exist.
- `set_shared_with_data::<Mesh>(entity, key, data)` can initialize a missing destination. The same key is a no-op for both set operations.
- `add_shared_from_data` derives the insertion key through the optional `TSharedComponentKey<Data>` trait. Payload edits never recompute it.

Adding, merging, or removing normal components retains shared keys. When a structural move creates a new archetype, it clones retained shared payloads. Existing destinations keep their own values. Source payloads are dropped when the source archetype becomes empty; dropping the world releases remaining entries. Shared storage grows independently of the fixed chunk size. Keys and data must be `Send + Sync`; data must be `Clone`. Use internal shared ownership, such as `Arc`, when deep cloning is undesirable.

Keep equality and hashes stable even when a key contains interior mutability. Changing identity is always an explicit structural operation.

Queries are no longer `Copy` or `Clone`. Direct world queries borrow `&mut World`; reacquire a query after structural changes to reuse its cached specification. Iteration and filtered views exclusively borrow their parent, preventing overlapping mutable access. The scheduler checks shared read/write conflicts between parameters and systems; payload views also carry runtime lock guards.

`shared::SharedCommands` (also exported from `cmd_buffer`) queues `set_shared` and `set_shared_with_data`. Build commands while iterating, then call `apply(&mut world)` after query borrows end. It drains commands in order and returns one result per command; errors do not roll back earlier successful commands. No implicit asset resolver is configured.

Run the example with `cargo run --example shared_component`.

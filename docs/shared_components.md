# Shared components

A shared component is a plain value that every entity of an archetype uses together. Its key, returned by `shared_key`, decides which archetype an entity lives in. Each archetype stores one value per shared component type, however many entities or chunks it has. Archetypes match by their normal component layout and **all** shared keys.

```rust
use xynok_ecs::{component, shared::TSharedComponent, world::World};

#[component]
struct Hp(u32);

#[derive(Clone)]
struct Mesh {
    id: u32,
    vertices: Vec<f32>,
}
impl TSharedComponent for Mesh {
    type Key = u32;
    fn shared_key(&self) -> u32 {
        self.id
    }
}

let mut world = World::default();
let entity = world.create(Hp(100));
world.add_shared_component(entity, Mesh { id: 7, vertices: vec![0.0, 1.0] }).unwrap();

let mut query = world.create_shared_query::<&Hp, &mut Mesh>();
for (mut mesh, mut archetype) in query.iter_archetype() {
    assert_eq!(mesh.id, 7);
    mesh.vertices.push(2.0);
    for chunk in archetype.iter_chunk() {
        for hp in chunk {
            assert_eq!(hp.0, 100);
        }
    }
}
```

`Query<T, S, F>` uses `T` for normal components, `S` for shared access, and `F` for an optional static key filter. The defaults keep `Query<T>` working as before. `S` can be `&Mesh`, `&mut Mesh`, `WithoutShared<Mesh>`, or a tuple of up to four of them. `Query<(), &Mesh>` reads shared components without asking for any normal component. Shared types are declared with `TSharedComponent`, and `#[component(shared)]` is rejected so it never silently stores a normal component.

`query.iter()` and `for value in query` yield only normal components. `iter_archetype()` yields the shared values once per nonempty matching archetype: `&Mesh` for a read, and a `SharedMut<Mesh>` view for a write. The view derefs to `&Mesh` and `&mut Mesh`.

## Keys

The key is read once, when an archetype is created, and stored next to the value. From then on the archetype is found through that stored key. Two things check that a value still gives it:

- `World::set_shared_component` returns `XynokEcsError::SharedKeyMismatch` and leaves the old value alone.
- A `SharedMut` view checks when it is dropped and panics on a mismatch. You can change the key fields while the view is alive, as long as they are back when it ends. The check is skipped while a panic is already unwinding, so the first panic is the one you see.

`World::override_shared_component` writes a value without any check. The archetype keeps its old key, so `add_shared_component_by_key`, `set_shared_component_by_key`, `filter_shared` and `group_by_shared` keep finding it through that old key. Keeping `shared_key` consistent is up to you. It runs on every check, so keep it cheap.

## World API

"The destination" below is the archetype with the entity's normal components, its other shared values, and the key in question.

| Method | Entity without `Mesh` | Entity with `Mesh` |
|---|---|---|
| `add_shared_component(e, value)` | creates the destination with `value` and moves there. If the destination already exists, returns `SharedDestinationExists` | `SharedComponentAlreadyExists` |
| `add_shared_component_by_key::<Mesh>(e, key)` | moves to the destination, or `UnresolvedSharedKey` when it does not exist | `SharedComponentAlreadyExists` |
| `remove_shared_component::<Mesh>(e)` | `SharedComponentDoesNotExist` | removes it and moves |
| `set_shared_component_by_key::<Mesh>(e, key)` | `SharedComponentDoesNotExist` | moves to the destination, or `UnresolvedSharedKey` when it does not exist. The current key does nothing |
| `set_shared_component(e, value)` | `SharedComponentDoesNotExist` | replaces the value, or `SharedKeyMismatch` |
| `override_shared_component(e, value)` | `SharedComponentDoesNotExist` | replaces the value, no key check |
| `get_shared_component::<Mesh>(e)` | `None` | `Some(&Mesh)` |
| `has_shared_component::<Mesh>(e)` | `false` | `true` |

Every method returns `EntityDoesNotExist` for a dead entity (`get` returns `None`, `has` returns `false`).

A few things worth knowing:

- Replacing a value affects every entity of that archetype. An archetype with the same key but other normal components has a value of its own and does not change.
- Moving to a key that no archetype holds yet takes two steps: `remove_shared_component`, then `add_shared_component` with the new value.
- A value you pass in and that is not used, because the call failed, is dropped.

## Filters and groups

Runtime filtering borrows the query and can be chained:

```rust,ignore
let mut filtered = query.filter_shared::<Mesh>(&7);
for hp in filtered.iter() { /* normal components */ }
```

A static filter implements `TSharedFilter` with `SharedComponents = Mesh` and `filter_key() -> Mesh::Key`. Use it in a system's `Query<&Hp, &Mesh, GrassMesh>`, or call `world.create_filtered_query::<&Hp, &Mesh, GrassMesh>()`. `filter_key` is called once per world, when the query is first created. The list of matching archetypes is cached together with the rest of the query and refreshed only when archetypes change, so a static filter costs nothing per frame.

`query.group_by_shared::<Mesh>()` yields `(owned_key, group)` pairs. `group.iter()` yields normal rows across matching archetypes, and `group.iter_archetype()` yields each archetype's own shared value and chunks. A group has no single value on purpose: equal keys in different archetypes may hold different values. Filtering or grouping on a shared type that is not in `S` or `F` panics.

## Normal components and lifetime

`add_component`, `merge_component` and `remove_component` only deal with normal components, and an entity keeps its shared values through them. When the new layout has no archetype with those shared values yet, one is created with clones of the current values. When it already has one, the entity uses the values that archetype holds.

A value is dropped when its archetype loses its last entity. After that, the `*_by_key` methods cannot find that key anymore, so use `add_shared_component` to bring it back. Dropping the world releases every remaining value. Shared storage is not limited by the chunk size. Values and keys must be `Send + Sync`, and values must be `Clone`. Wrap the heavy parts in an `Arc` if deep clones are too costly.

## How it is stored

Each world gives every key a small id per shared type the first time it sees it. An archetype is identified by its normal component set plus the sorted list of `(shared component id, key id)` pairs, and the world finds it with one hash map lookup. Archetypes without shared components keep using the old lookup, so they pay nothing for this feature. The key table only grows: it remembers every distinct key the world has created an archetype for, not only the live ones. Looking up a key through the `*_by_key` methods does not add it.

When a shared archetype loses its last entity, its values are dropped and the archetype is parked. The next shared archetype with the same normal components takes over that slot, so changing keys over and over does not keep growing the archetype list.

Shared components and normal components share one id space. A query's access scope is a single set of read, write, and exclude ids covering both kinds, and the scheduler's existing conflict check works on it unchanged.

## Borrowing

Queries are not `Copy` or `Clone`. Direct world queries borrow `&mut World`, so reacquire a query after structural changes to reuse its cached specification. Iteration and filtered views borrow their parent exclusively, so the compiler rules out two views handing out the same value at once. Across systems, the scheduler rejects parameters and parallel groups whose access conflicts. With both checks in place, shared views are plain references into the archetype's storage, with no lock. The only runtime work is the key check when a `SharedMut` is dropped.

`shared::SharedCommands` (also exported from `cmd_buffer`) queues every structural and write method above: `add_shared_component`, `add_shared_component_by_key`, `remove_shared_component`, `set_shared_component_by_key`, `set_shared_component` and `override_shared_component`. Record commands while iterating, then call `apply(&mut world)` once the query borrows end. It runs them in order and returns one result per command. A failed command does not undo the ones before it.

Run the example with `cargo run --example shared_component`.

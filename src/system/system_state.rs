use std::marker::PhantomData;

use crate::world::query_spec::QuerySpecAccessor;

/// What a `Query` param keeps between runs: the resolved accessor plus the archetype version it
/// was resolved at. Re-resolving costs a hash lookup and, when the version moved, a full rescan of
/// every archetype — doing that once per frame per query instead of once per iteration is the
/// reason this state exists at all.
pub struct SystemState<T>
{
    pub accessor: QuerySpecAccessor,
    pub version:  usize,
    pub p:        PhantomData<fn() -> T>,
}

// SAFETY: the accessor is two raw pointers into `World`-owned storage. The state never dereferences
// them on its own; the only reads happen inside `fetch`, which is handed a `UnsafeWorldCell` and is
// therefore already bound to the world's lifetime and to the caller's access-scope check. Sending
// the state to another thread moves no data the world does not still own.
unsafe impl<T> Send for SystemState<T> {}
unsafe impl<T> Sync for SystemState<T> {}

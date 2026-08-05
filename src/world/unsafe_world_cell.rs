use std::marker::PhantomData;

use crate::world::World;

/// A `&mut World` that has given up Rust's aliasing guarantee, on purpose.
///
/// A system takes several params at once (`fn(Query<&mut Hp>, Query<&Mana>)`) and each of them
/// needs to reach into the same world. Borrowck cannot express "these two disjoint views are fine";
/// only the [`AccessScope`](crate::query::access_scope::AccessScope) check can. So the scope check
/// happens once at `into_system()` time and the world is handed to the params through this cell,
/// which carries the invariant in its name and its `unsafe fn`s instead of passing a bare
/// `*mut World` around.
///
/// The lifetime `'w` is not decoration: it keeps the cell, and every `Item<'w>` a param derives
/// from it, from outliving the `&mut World` it was made from. That is the one check borrowck can
/// still perform here, so the only thing left unchecked is the aliasing *between params*, which is
/// exactly what `AccessScope` already validated.
///
/// `PhantomData` is what makes `'w` legal to declare at all — a raw pointer mentions no lifetime,
/// and an unused lifetime parameter is an error.
#[derive(Clone, Copy)]
pub struct UnsafeWorldCell<'w>(*mut World, PhantomData<&'w World>);

// SAFETY: the pointer is only ever dereferenced under the `unsafe fn`s below, whose contract puts
// the burden of non-aliasing on the caller. `World` itself owns no thread-affine state.
//
// Nothing needs these yet — the cell never leaves the `run` that created it. They are here for the
// parallel executor, which will hand one cell to several threads at once; without them the raw
// pointer makes the cell `!Send + !Sync` and that is the point at which it would matter.
unsafe impl Send for UnsafeWorldCell<'_> {}
unsafe impl Sync for UnsafeWorldCell<'_> {}

impl<'w> UnsafeWorldCell<'w>
{
    pub(crate) fn new(world: &'w mut World) -> Self
    {
        Self(world as *mut World, PhantomData)
    }

    /// # Safety
    /// No other live view of this world may be writing while the returned reference is alive.
    #[allow(dead_code)]
    pub(crate) unsafe fn world(self) -> &'w World
    {
        unsafe { &*self.0 }
    }

    /// # Safety
    /// The caller must have checked, via `AccessScope`, that whatever this reference touches is
    /// disjoint from every other view handed out from the same cell.
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn world_mut(self) -> &'w mut World
    {
        unsafe { &mut *self.0 }
    }

    /// Reading the archetype version needs no exclusivity: it is a plain `usize` and every writer
    /// of it holds `&mut World`, which cannot coexist with this cell's `'w`.
    pub(crate) fn archetype_version(self) -> usize
    {
        unsafe { (*self.0).archetype_version() }
    }
}

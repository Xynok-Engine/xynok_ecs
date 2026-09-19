use std::ops::{Deref, DerefMut};

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{QueryTicks, TQueryColumn, TQueryParam};
use crate::apis::traits::TArchetype;
use crate::world::World;

/// The one row of a singleton archetype, handed straight to a system:
///
/// ```ignore
/// fn tick_time(mut time: Singleton<&mut Time>)
/// {
///     time.update();
/// }
///
/// fn read_time(time: Singleton<&Time>)
/// {
///     let delta = time.delta();
/// }
/// ```
///
/// `P` is `&T` or `&mut T`, and `T` names the archetype, not just a component of it. The world
/// must already know `T` as a singleton archetype (`World::register_singleton` or
/// `World::create_singleton`); anything else is an error rather than a fallback to a normal
/// query, which is the whole point of asking for a `Singleton`.
///
/// It reads and writes like the reference it wraps, through `Deref`/`DerefMut`. A `&mut T` on a
/// `ChangeAble` component still goes through [`Mut`](crate::query::mut_ref::Mut), so only a real
/// write stamps the changed tick.
pub struct Singleton<'a, P: TQueryParam>
{
    item: P::QueryItem<'a>,
}

impl<'a, P> Singleton<'a, P>
where
    P: TQueryColumn,
    P::Component: TArchetype + 'static,
{
    /// Reads the single row of `P::Component`'s archetype.
    ///
    /// Only needs `&World`: like a prepared query, this is built beside the other parameters of
    /// a system, which may already hold shared borrows into the world.
    pub(crate) fn new(world: &'a World, last_run_tick: ChangedTick) -> Result<Self, XynokEcsError>
    {
        let arch_spec = world.singleton_archetype_spec::<P::Component>()?;

        // A singleton keeps at most one chunk, and that chunk is empty while the entity has not
        // been spawned yet (or was destroyed and left the chunk behind for reuse).
        let not_spawned = || XynokEcsError::SingletonEntityIsNotSpawned(std::any::type_name::<P::Component>());
        if arch_spec.arch.chunk_count() == 0
        {
            return Err(not_spawned());
        }
        let chunk = arch_spec.arch.chunk_at(0);
        if chunk.is_empty()
        {
            return Err(not_spawned());
        }

        let ticks = QueryTicks {
            last_run: last_run_tick,
            this_run: world.current_tick(),
        };
        let state = P::arch_state(&arch_spec.layout);
        // SAFETY: `state` was resolved from the very layout this chunk was built from, and row 0
        // is live. Nothing else reaches this row: the access scope of `P` keeps every other
        // system that touches the component off this thread's back.
        let item = unsafe {
            let fetch = P::fetch_init(state, chunk.ptr());
            P::fetch(&fetch, 0, ticks)
        };

        Ok(Self { item: item })
    }

    /// The wrapped reference itself, when `Deref` is not enough (passing a plain `&mut T` along,
    /// for instance)
    #[inline]
    pub fn into_inner(self) -> P::QueryItem<'a>
    {
        self.item
    }
}

impl<'a, P> Deref for Singleton<'a, P>
where
    P: TQueryColumn,
    P::QueryItem<'a>: Deref<Target = P::Component>,
{
    type Target = P::Component;

    #[inline]
    fn deref(&self) -> &P::Component
    {
        Deref::deref(&self.item)
    }
}

impl<'a, P> DerefMut for Singleton<'a, P>
where
    P: TQueryColumn,
    P::QueryItem<'a>: DerefMut<Target = P::Component>,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut P::Component
    {
        DerefMut::deref_mut(&mut self.item)
    }
}

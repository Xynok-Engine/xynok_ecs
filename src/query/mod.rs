use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryParam, TReadOnlyQueryParam};
use crate::apis::traits::TArchetype;
use crate::query::query_iter::QueryIter;
use crate::world::World;
use crate::world::query_spec::QuerySpecAccessor;
use std::marker::PhantomData;

pub mod query_iter;

pub(crate) mod access_scope;
pub(crate) mod src_access;
pub(crate) mod src_access_enable;
pub(crate) mod src_access_changed;
pub(crate) mod src_access_added;
mod tuple;
mod variant;

/// A `Query` is `Copy` only when it is read-only:
///
/// ```
/// use xynok_ecs::query::Query;
/// fn needs_copy<T: Copy>() {}
/// #[xynok_ecs::component]
/// struct Hp(u32);
/// #[xynok_ecs::component]
/// struct Mana(u32);
/// needs_copy::<Query<'static, &Hp>>();
/// needs_copy::<Query<'static, (&Hp, &Mana)>>();
/// ```
///
/// Any `&mut` in the query makes it non-copyable:
///
/// ```compile_fail
/// use xynok_ecs::query::Query;
/// fn needs_copy<T: Copy>() {}
/// #[xynok_ecs::component]
/// struct Hp(u32);
/// needs_copy::<Query<'static, &mut Hp>>();
/// ```
///
/// ```compile_fail
/// use xynok_ecs::query::Query;
/// fn needs_copy<T: Copy>() {}
/// #[xynok_ecs::component]
/// struct Hp(u32);
/// #[xynok_ecs::component]
/// struct Mana(u32);
/// needs_copy::<Query<'static, (&Hp, &mut Mana)>>();
/// ```
pub struct Query<'a, T: TQueryParam + 'static>
{
    accessor: QuerySpecAccessor<'a>,
    phantom:  PhantomData<T>,
}

// Only read-only queries may be copied. Every copy starts its own iterator from row 0, so a
// copyable `Query<&mut Hp>` would hand out two `&mut Hp` for the same entity.
impl<'a, T: TReadOnlyQueryParam + 'static> Clone for Query<'a, T>
{
    fn clone(&self) -> Self
    {
        *self
    }
}
impl<'a, T: TReadOnlyQueryParam + 'static> Copy for Query<'a, T> {}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    pub(crate) fn new(world: &'a mut World, last_run_tick: crate::apis::constants::ChangedTick) -> Result<Self, XynokEcsError>
    {
        let accessor = world.get_or_create_query_src_access::<T>(last_run_tick)?;
        Ok(Self {
            accessor: accessor,
            phantom:  PhantomData,
        })
    }
}

impl<'a, T: TQueryParam + 'static> IntoIterator for Query<'a, T>
{
    type Item = T::QueryItem<'a>;

    type IntoIter = QueryIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter
    {
        QueryIter::new(&self.accessor)
    }
}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    pub fn with_shared_component_filter<TFilter: TArchetype>() {}
}

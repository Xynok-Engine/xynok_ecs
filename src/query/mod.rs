use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::apis::traits::TArchetype;
use crate::query::query_iter::QueryIter;
use crate::world::query_spec::QuerySpecAccessor;
use crate::world::World;
use std::marker::PhantomData;

pub mod query_iter;

mod chunk_view;
pub use chunk_view::ChunkView;

pub(crate) mod access_scope;
mod src_access;
mod tuple;
mod variant;

pub struct Query<'a, T: TQueryParam + 'static>
{
    pub(crate) accessor: QuerySpecAccessor,
    phantom:  PhantomData<(&'a (), T)>,
}

// Not derived: `#[derive(Clone, Copy)]` would add a spurious `T: Clone + Copy` bound, which
// breaks queries like `Query<&mut Hp>` even though neither field actually depends on it
impl<'a, T: TQueryParam + 'static> Clone for Query<'a, T>
{
    fn clone(&self) -> Self
    {
        *self
    }
}
impl<'a, T: TQueryParam + 'static> Copy for Query<'a, T> {}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    pub(crate) fn new(world: &mut World) -> Result<Self, XynokEcsError>
    {
        let accessor = world.get_or_create_query_src_access::<T>()?;
        Ok(Self {
            accessor: accessor,
            phantom:  PhantomData,
        })
    }

    /// Builds a query from a spec that already exists, reading `world` only.
    ///
    /// Returns `None` when nobody has built that spec yet, or when it has gone stale. See
    /// `World::query_src_access`.
    pub(crate) fn from_prepared(world: &World) -> Option<Self>
    {
        world.query_src_access::<T>().map(|accessor| Self {
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
        QueryIter::new(self.accessor)
    }
}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    pub fn with_shared_component_filter<TFilter: TArchetype>() {}
}

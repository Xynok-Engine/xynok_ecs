use std::marker::PhantomData;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::query::query_iter::QueryIter;
use crate::world::query_spec::QuerySpecAccessor;
use crate::world::World;

pub mod query_iter;
pub(crate) mod access_scope;
mod src_access;
mod tuple;
mod variant;
pub struct Query<T: TQueryParam + 'static>
{
    accessor: QuerySpecAccessor,
    phantom:  PhantomData<T>,
}

// Not derived: `#[derive(Clone, Copy)]` would add a spurious `T: Clone + Copy` bound, which
// breaks queries like `Query<&mut Hp>` even though neither field actually depends on it.
impl<T: TQueryParam + 'static> Clone for Query<T>
{
    fn clone(&self) -> Self
    {
        *self
    }
}
impl<T: TQueryParam + 'static> Copy for Query<T> {}

impl<T: TQueryParam + 'static> Query<T>
{
    pub(crate) fn new(world: &mut World) -> Result<Self, XynokEcsError>
    {
        let accessor = world.get_or_create_query_src_access::<T>()?;
        Ok(Self {
            accessor: accessor,
            phantom:  PhantomData,
        })
    }
}

impl<T: TQueryParam + 'static> IntoIterator for Query<T>
{
    type Item = T::QueryItem<'static>;

    type IntoIter = QueryIter<'static, T>;

    fn into_iter(self) -> Self::IntoIter
    {
        QueryIter::new(self.accessor)
    }
}

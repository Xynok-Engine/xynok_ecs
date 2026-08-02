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
#[derive(Clone, Copy)]
pub struct Query<T: TQueryParam + 'static>
{
    accessor: QuerySpecAccessor,
    phantom:  PhantomData<T>,
}

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

impl<'a, T: TQueryParam + 'static> IntoIterator for &'a Query<T>
{
    type Item = T::QueryItem<'a>;

    type IntoIter = QueryIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter
    {
        QueryIter::new(self.accessor)
    }
}

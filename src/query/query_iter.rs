use std::marker::PhantomData;

use crate::apis::internal_traits::TQueryParam;
use crate::world::query_spec::QuerySpecAccessor;

pub struct QueryIter<'a, T: TQueryParam + 'static>
{
    src_access: T::SrcAccess<'a>,
    phantom:    PhantomData<(&'a (), T)>,
}

impl<'a, T: TQueryParam + 'static> Iterator for QueryIter<'a, T>
{
    type Item = T::QueryItem<'a>;

    #[inline]
    #[track_caller]
    fn next(&mut self) -> Option<Self::Item>
    {
        T::next(&mut self.src_access)
    }
}

impl<'a, T: TQueryParam + 'static> QueryIter<'a, T>
{
    pub(crate) fn new(access: QuerySpecAccessor) -> Self
    {
        Self {
            src_access: T::build_src_access(&access),
            phantom:    PhantomData,
        }
    }
}

use std::marker::PhantomData;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryParam, TSystemParam};
use crate::query::access_scope::AccessScope;
use crate::query::Query;
use crate::world::query_spec::QuerySpecAccessor;
use crate::world::unsafe_world_cell::UnsafeWorldCell;
use crate::world::World;

/// What a `Query` param keeps between runs: the resolved accessor plus the archetype version it
/// was resolved at. Re-resolving costs a hash lookup and, when the version moved, a full rescan of
/// every archetype — doing that once per frame per query instead of once per iteration is the
/// reason this state exists at all.
pub struct QueryState<T>
{
    accessor: QuerySpecAccessor,
    version:  usize,
    _p:       PhantomData<fn() -> T>,
}

// SAFETY: the accessor is two raw pointers into `World`-owned storage. The state never dereferences
// them on its own; the only reads happen inside `fetch`, which is handed a `UnsafeWorldCell` and is
// therefore already bound to the world's lifetime and to the caller's access-scope check. Sending
// the state to another thread moves no data the world does not still own.
unsafe impl<T> Send for QueryState<T> {}
unsafe impl<T> Sync for QueryState<T> {}

impl<T: TQueryParam + 'static> TSystemParam for Query<T>
{
    type Item<'w> = Query<T>;
    type State = QueryState<T>;

    fn init_state(world: &mut World) -> Result<Self::State, XynokEcsError>
    {
        Ok(QueryState {
            accessor: world.get_or_create_query_src_access::<T>()?,
            version:  world.archetype_version(),
            _p:       PhantomData,
        })
    }

    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        T::access_scope()
    }

    unsafe fn fetch<'w>(state: &'w mut Self::State, world: UnsafeWorldCell<'w>) -> Result<Self::Item<'w>, XynokEcsError>
    {
        // a new archetype may have appeared since the last run and it may match this query, so the
        // cached archetype list has to be re-checked here — at the point of use — not just at init
        let version = world.archetype_version();
        if state.version != version
        {
            state.accessor = unsafe { world.world_mut() }.get_or_create_query_src_access::<T>()?;
            state.version = version;
        }
        Ok(Query::from_accessor(state.accessor))
    }
}

/// A system that takes no params at all. `()` is its param list, so `fn()` needs no special case in
/// `FunctionSystem`.
impl TSystemParam for ()
{
    type Item<'w> = ();
    type State = ();

    fn init_state(_world: &mut World) -> Result<Self::State, XynokEcsError>
    {
        Ok(())
    }

    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }

    unsafe fn fetch<'w>(_state: &'w mut Self::State, _world: UnsafeWorldCell<'w>) -> Result<Self::Item<'w>, XynokEcsError>
    {
        Ok(())
    }
}

macro_rules! impl_tuple_system_param {
    ($($p:ident),+) => {
        #[allow(non_snake_case)]
        impl<$($p: TSystemParam),+> TSystemParam for ($($p,)+)
        {
            type Item<'w> = ($($p::Item<'w>,)+);
            type State = ($($p::State,)+);

            fn init_state(world: &mut World) -> Result<Self::State, XynokEcsError>
            {
                Ok(($($p::init_state(world)?,)+))
            }

            fn access_scope() -> Result<AccessScope, XynokEcsError>
            {
                let mut scope = AccessScope::default();
                // `extend` is what rejects `fn(Query<&mut Hp>, Query<&Hp>)`: two params of the same
                // system may not touch one component if either of them writes
                $(scope.extend($p::access_scope()?)?;)+
                Ok(scope)
            }

            unsafe fn fetch<'w>(state: &'w mut Self::State, world: UnsafeWorldCell<'w>) -> Result<Self::Item<'w>, XynokEcsError>
            {
                let ($($p,)+) = state;
                // SAFETY: `access_scope` above already proved these params are disjoint, and it ran
                // at `into_system()` time — before any of this could be reached
                Ok(($(unsafe { $p::fetch($p, world)? },)+))
            }
        }
    };
}

#[rustfmt::skip] impl_tuple_system_param!(P0);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14);
#[rustfmt::skip] impl_tuple_system_param!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15);

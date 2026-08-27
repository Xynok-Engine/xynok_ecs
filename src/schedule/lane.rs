//! The parameter that lets a system reach the pool running it.

use xynok_concurrency::pool::ThreadPool;
use xynok_std::unsafe_ptr::HeapMut;

use crate::world::World;

/// Lane A's pool, seen from inside a system.
///
/// This is what [`crate::query::Query::par_for_each_chunk`] needs, and also where to open a scope
/// for parallel work that does not go through a query.
///
/// A world not yet attached to a scheduler hands back a pool with no threads, so everything runs
/// sequentially. That way a system never has to branch on whether a scheduler exists.
///
/// ```no_run
/// use xynok_ecs::query::Query;
/// use xynok_ecs::schedule::lane::Lane;
/// # use xynok_ecs::apis::traits::TComponent;
/// # use xynok_ecs_proc_macro::component;
/// # #[component]
/// # struct Position(f32);
/// fn advance(query: Query<&mut Position>, lane: Lane)
/// {
///     query.par_for_each_chunk(lane.pool(), 8, |view| {
///         for position in view.columns
///         {
///             position.0 += 1.0;
///         }
///     });
/// }
/// ```
pub struct Lane
{
    pub(crate) world: HeapMut<World>,
}

impl Lane
{
    #[inline]
    pub fn pool(&self) -> &ThreadPool
    {
        self.world.as_ref_with_caller_lifetime().lane()
    }

    /// How many seats the pool has: its worker threads plus the calling thread.
    #[inline]
    pub fn worker_count(&self) -> usize
    {
        self.pool().worker_count()
    }

    /// Which seat the thread running this system occupies.
    #[inline]
    pub fn worker_index(&self) -> usize
    {
        self.world.as_ref_with_caller_lifetime().worker_index()
    }
}

impl std::fmt::Debug for Lane
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        f.debug_struct("Lane").field("worker_count", &self.worker_count()).finish_non_exhaustive()
    }
}

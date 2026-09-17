use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryParam, TReadOnlyQueryParam};
use crate::query::query_iter::{ParWalker, QueryBatches, QueryChunks, QueryIter};
use crate::schedule::executor::TJobExecutor;
use crate::world::query_spec::QuerySpecAccessor;
use crate::world::World;
use std::marker::PhantomData;
use std::ptr::NonNull;

pub(crate) mod access_scope;
mod tuple;
mod variant;
mod entity;

pub mod query_iter;
pub mod filter;
pub mod mut_ref;
pub mod detail;

/// A `Query` is `Copy` only when it is read-only. Any `&mut` in the query makes it non-copyable.
///
/// A query cannot outlive a change to the world it reads, since it borrows the world mutably
/// for as long as it lives.
pub struct Query<'a, T: TQueryParam + 'static>
{
    accessor: QuerySpecAccessor<'a>,
    // Only ever read through, to reach the world's executor. `accessor` is derived from this
    // pointer, so reading the world through it leaves the accessor's borrows valid.
    world:    NonNull<World>,
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
        let world = NonNull::from(world);
        // SAFETY: `world` came from a `&'a mut World` that this query now stands in for
        let accessor = unsafe { &mut *world.as_ptr() }.get_or_create_query_src_access::<T>(last_run_tick)?;
        Ok(Self {
            accessor: accessor,
            world:    world,
            phantom:  PhantomData,
        })
    }

    /// The read-only build, for a world whose spec for `T` is already registered and up to date
    /// (see [`World::prepare_query_src_access`]). Returns `None` otherwise, and the caller then
    /// falls back to [`Query::new`].
    ///
    /// This is what a job inside a parallel group uses: it never needs `&mut World`, so several
    /// jobs building their queries at once stay sound.
    pub(crate) fn new_prepared(world: &'a World, last_run_tick: crate::apis::constants::ChangedTick) -> Option<Self>
    {
        let world_ptr = NonNull::from(world);
        Some(Self {
            accessor: world.query_src_access::<T>(last_run_tick)?,
            world:    world_ptr,
            phantom:  PhantomData,
        })
    }

    /// Every entity of the query, one at a time.
    ///
    /// Takes `&mut self` even for a read-only query, so the items of two `iter` calls can never
    /// both be alive. A read-only `Query` is `Copy`, so copy it when you need two walks at once.
    #[inline]
    pub fn iter(&mut self) -> QueryIter<'_, T>
    {
        QueryIter::new(&self.accessor)
    }

    /// Every non-empty chunk of the query. Walk each one with `chunk.iter()` or `for item in chunk`.
    #[inline]
    pub fn iter_chunk(&mut self) -> QueryChunks<'_, T>
    {
        QueryChunks::new(&self.accessor)
    }

    /// Cuts the query into batches of about `batch_amount` rows each, crossing chunk and
    /// archetype borders as needed. Rows are counted before filters. A cut inside a chunk snaps
    /// down to a multiple of [`MIN_BATCH_AMOUNT`](query_iter::MIN_BATCH_AMOUNT), so a batch can
    /// be up to 7 rows short, and the last one can be smaller still. Batches cover disjoint rows
    /// and are `Send`, so each can go to its own thread.
    ///
    /// # Panics
    /// If `batch_amount` is below [`MIN_BATCH_AMOUNT`](query_iter::MIN_BATCH_AMOUNT).
    #[inline]
    #[track_caller]
    pub fn iter_batch(&mut self, batch_amount: usize) -> QueryBatches<'_, T>
    {
        QueryBatches::new(&self.accessor, batch_amount)
    }

    /// Calls `f` on every entity of the query, with each non-empty chunk sent as one job to the
    /// world's executor. Returns once every job is done.
    ///
    /// When the world has no executor (see [`World::set_executor`]) it walks the rows on the
    /// calling thread instead, so the result is the same either way, only slower.
    ///
    /// `f` runs on several threads at once, which is why it has to be `Fn + Send + Sync`. Keep
    /// in mind that the order rows are visited in is not fixed.
    #[inline]
    pub fn par_for_each_chunk<'s, F>(&'s mut self, f: F)
    where F: Fn(T::QueryItem<'s>) + Send + Sync
    {
        let walker = ParWalker::<T>::new(&self.accessor);
        match self.executor()
        {
            // SAFETY: every start comes from `chunk_starts` and names a different chunk
            Some(executor) => executor.run_jobs(&mut walker.chunk_starts(), &|start| unsafe { walker.walk_chunk(start, &f) }),
            None => self.iter().for_each(f),
        }
    }

    /// Same as [`par_for_each_chunk`](Self::par_for_each_chunk), but one job is one batch of
    /// about `batch_amount` rows, cut the way [`iter_batch`](Self::iter_batch) cuts them. Handy
    /// when a chunk holds too many or too few rows to be a good job on its own.
    ///
    /// # Panics
    /// If `batch_amount` is below [`MIN_BATCH_AMOUNT`](query_iter::MIN_BATCH_AMOUNT).
    #[inline]
    #[track_caller]
    pub fn par_for_each_batch<'s, F>(&'s mut self, batch_amount: usize, f: F)
    where F: Fn(T::QueryItem<'s>) + Send + Sync
    {
        let walker = ParWalker::<T>::new(&self.accessor);
        let mut starts = walker.batch_starts(batch_amount);
        match self.executor()
        {
            // SAFETY: every start comes from `batch_starts` with this same `batch_amount`, and
            // batches cut that way never overlap
            Some(executor) => executor.run_jobs(&mut starts, &|start| unsafe { walker.walk_batch(start, batch_amount, &f) }),
            None => self.iter().for_each(f),
        }
    }

    #[inline]
    fn executor(&self) -> Option<&dyn TJobExecutor>
    {
        // SAFETY: only a shared read, and the world cannot change while this query borrows it
        unsafe { self.world.as_ref() }.executor()
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

#[cfg(test)]
mod test
{
    use crate::apis::internal_traits::TQueryParam;
    use crate::component;
    use crate::query::detail::Detail;
    use crate::query::filter::{Added, Changed, Disabled, Enabled, Without};
    use crate::world::World;

    #[component(EnableAble, ChangeAble)]
    #[derive(Default)]
    struct Hp(#[allow(dead_code)] u32);

    #[component(EnableAble, ChangeAble)]
    #[derive(Default)]
    struct Mana(#[allow(dead_code)] u32);

    /// The world keys its query registry (and with it the access scope the scheduler reads) by
    /// `TYPE_ID`, so two queries that touch a component differently must never share one.
    #[test]
    fn every_query_shape_gets_its_own_type_id()
    {
        let ids = [
            <&Hp as TQueryParam>::TYPE_ID,
            <&mut Hp as TQueryParam>::TYPE_ID,
            <&Mana as TQueryParam>::TYPE_ID,
            <Added<&Hp> as TQueryParam>::TYPE_ID,
            <Added<&mut Hp> as TQueryParam>::TYPE_ID,
            <Changed<&Hp> as TQueryParam>::TYPE_ID,
            <Enabled<&Hp> as TQueryParam>::TYPE_ID,
            <Disabled<&Hp> as TQueryParam>::TYPE_ID,
            <(&Hp, &Mana) as TQueryParam>::TYPE_ID,
            <(&Hp, &mut Mana) as TQueryParam>::TYPE_ID,
            <(&mut Hp, &Mana) as TQueryParam>::TYPE_ID,
            <(Changed<&Hp>, &Mana) as TQueryParam>::TYPE_ID,
            <(&Hp, Without<Mana>) as TQueryParam>::TYPE_ID,
            <(&Hp, Without<Hp>) as TQueryParam>::TYPE_ID,
            <Detail<&Hp> as TQueryParam>::TYPE_ID,
            <Detail<&mut Hp> as TQueryParam>::TYPE_ID,
            <(Detail<&Hp>, &Mana) as TQueryParam>::TYPE_ID,
        ];

        for (i, a) in ids.iter().enumerate()
        {
            for (j, b) in ids.iter().enumerate()
            {
                if i != j
                {
                    assert_ne!(a, b, "query shapes {i} and {j} collide on one TYPE_ID and would share a QuerySpec");
                }
            }
        }
    }

    /// `Shape` also shows up in panic messages and debugger output, so it should read like the
    /// query you wrote. Run with `--nocapture` to see it.
    #[test]
    fn shape_reads_like_the_query_it_came_from()
    {
        fn shape_of<Q: TQueryParam>() -> String
        {
            // strip module paths so `xynok_ecs::query::mod::test::Hp` reads as `Hp`
            let full = std::any::type_name::<Q::Shape>();
            let mut out = String::new();
            let mut segment = String::new();
            for ch in full.chars()
            {
                if ch.is_alphanumeric() || ch == '_' || ch == ':'
                {
                    segment.push(ch);
                }
                else
                {
                    out.push_str(segment.rsplit("::").next().unwrap_or(&segment));
                    out.push(ch);
                    segment.clear();
                }
            }
            out.push_str(segment.rsplit("::").next().unwrap_or(&segment));
            out
        }

        let cases = [
            (shape_of::<&Hp>(), "&Hp"),
            (shape_of::<&mut Hp>(), "&mut Hp"),
            (shape_of::<Added<&Hp>>(), "Added<&Hp>"),
            (shape_of::<Changed<&mut Hp>>(), "Changed<&mut Hp>"),
            (shape_of::<Disabled<&Hp>>(), "Disabled<&Hp>"),
            (shape_of::<(Added<&Hp>, Enabled<&mut Mana>)>(), "(Added<&Hp>, Enabled<&mut Mana>)"),
            (shape_of::<(&Hp, Without<Mana>)>(), "(&Hp, Without<Mana>)"),
        ];

        for (got, want) in &cases
        {
            println!("{got}");
            assert_eq!(got, want);
        }
    }

    /// The same shape asked for twice must land on the same registry slot, otherwise every
    /// `create_query` call would rebuild the archetype list from scratch.
    #[test]
    fn the_same_query_shape_keeps_one_type_id()
    {
        assert_eq!(<&Hp as TQueryParam>::TYPE_ID, <&Hp as TQueryParam>::TYPE_ID);
        assert_eq!(<Changed<&Hp> as TQueryParam>::TYPE_ID, <Changed<&Hp> as TQueryParam>::TYPE_ID);
    }

    /// Registering the read query first must not leave the write query with a read-only scope:
    /// the scheduler would then happily run it next to other readers of the same component.
    #[test]
    fn a_write_query_does_not_inherit_a_read_querys_scope()
    {
        let mut w = World::default();
        w.create(Hp(1));

        w.create_query::<&Hp>();
        w.create_query::<&mut Hp>();

        assert_eq!(w.registered_query_is_read_only::<&Hp>(), Some(true), "`&Hp` only reads");
        assert_eq!(w.registered_query_is_read_only::<&mut Hp>(), Some(false), "`&mut Hp` writes");
        assert_eq!(
            w.registered_query_is_read_only::<Changed<&mut Hp>>(),
            None,
            "a shape nobody asked for must not be answered by someone else's spec"
        );
    }
}

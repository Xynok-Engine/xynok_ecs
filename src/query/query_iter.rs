use std::marker::PhantomData;

use crate::apis::internal_traits::{QueryTicks, TQueryParam};
use crate::archetype::Archetype;
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::QuerySpecAccessor;

/// Walks the non-empty chunks of every archetype a query matched and resolves each one into a
/// `Fetch`. `iter`, `iter_chunk` and `iter_batch` all sit on top of this one walker.
struct ChunkCursor<'q, T: TQueryParam>
{
    archetypes:   &'q ArchetypeSpecs,
    arch_indices: &'q [usize],
    arch_pos:     usize,
    arch:         Option<(&'q Archetype, T::ArchState)>,
    chunk_idx:    usize,
}

impl<'q, T: TQueryParam> ChunkCursor<'q, T>
{
    fn new(accessor: &QuerySpecAccessor<'q>) -> Self
    {
        Self {
            archetypes:   accessor.archetypes,
            // a shape with no column has nothing to walk, so it starts out already finished
            arch_indices: if T::NAMES_NO_COLUMN { &[] } else { accessor.arch_indices() },
            arch_pos:     0,
            arch:         None,
            chunk_idx:    0,
        }
    }

    /// The next non-empty chunk, as its resolved pointers and its row count
    #[inline]
    #[track_caller]
    fn next_chunk(&mut self) -> Option<(T::Fetch, usize)>
    {
        loop
        {
            if let Some((arch, state)) = self.arch
            {
                while self.chunk_idx < arch.chunk_count()
                {
                    let chunk = arch.chunk_at(self.chunk_idx);
                    self.chunk_idx += 1;

                    if chunk.is_empty()
                    {
                        continue;
                    }
                    // SAFETY: `state` was resolved from the layout this chunk was built from
                    return Some((unsafe { T::fetch_init(state, chunk.ptr()) }, chunk.len()));
                }
            }

            let &arch_idx = self.arch_indices.get(self.arch_pos)?;
            self.arch_pos += 1;

            let arch_spec = match self.archetypes.value_at(arch_idx)
            {
                Some(arch_spec) => arch_spec,
                None => panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry"),
            };
            self.arch = Some((&arch_spec.arch, T::arch_state(&arch_spec.layout)));
            self.chunk_idx = 0;
        }
    }
}

/// Rows `start..end` of one chunk.
///
/// Every iterator in this file hands out items with a lifetime that is not tied to its own
/// borrow, so the one rule that keeps `&mut` items sound is: two live row ranges never overlap.
/// A single query walk visits each row once, and `iter_batch` cuts the rows into disjoint
/// pieces, so the rule holds as long as the `Query` stays borrowed for as long as they live.
pub struct RowRange<T: TQueryParam>
{
    fetch: T::Fetch,
    start: usize,
    end:   usize,
}

impl<T: TQueryParam> Clone for RowRange<T>
{
    fn clone(&self) -> Self
    {
        *self
    }
}
impl<T: TQueryParam> Copy for RowRange<T> {}

/// Hands out the rows of one [`RowRange`] that pass the query's filters
pub struct ChunkIter<'q, T: TQueryParam>
{
    range:   RowRange<T>,
    ticks:   QueryTicks,
    phantom: PhantomData<&'q ()>,
}

impl<'q, T: TQueryParam> ChunkIter<'q, T>
{
    #[inline]
    fn new(range: RowRange<T>, ticks: QueryTicks) -> Self
    {
        Self {
            range:   range,
            ticks:   ticks,
            phantom: PhantomData,
        }
    }
}

impl<'q, T: TQueryParam> Iterator for ChunkIter<'q, T>
{
    type Item = T::QueryItem<'q>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        while self.range.start < self.range.end
        {
            let row = self.range.start;
            self.range.start = row + 1;

            // SAFETY: `row` is below the chunk's length, and no other live range covers it
            unsafe {
                if T::accepts(&self.range.fetch, row, self.ticks)
                {
                    return Some(T::fetch(&self.range.fetch, row, self.ticks));
                }
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>)
    {
        (0, Some(self.range.end - self.range.start))
    }
}

/// Every row of a query, one entity at a time. Returned by [`Query::iter`](crate::query::Query::iter).
pub struct QueryIter<'q, T: TQueryParam>
{
    cursor: ChunkCursor<'q, T>,
    rows:   Option<ChunkIter<'q, T>>,
    ticks:  QueryTicks,
}

impl<'q, T: TQueryParam> QueryIter<'q, T>
{
    pub(crate) fn new(accessor: &QuerySpecAccessor<'q>) -> Self
    {
        Self {
            cursor: ChunkCursor::new(accessor),
            rows:   None,
            ticks:  ticks_of(accessor),
        }
    }
}

impl<'q, T: TQueryParam> Iterator for QueryIter<'q, T>
{
    type Item = T::QueryItem<'q>;

    #[inline]
    #[track_caller]
    fn next(&mut self) -> Option<Self::Item>
    {
        loop
        {
            if let Some(rows) = &mut self.rows
                && let Some(item) = rows.next()
            {
                return Some(item);
            }

            let (fetch, len) = self.cursor.next_chunk()?;
            self.rows = Some(ChunkIter::new(RowRange { fetch, start: 0, end: len }, self.ticks));
        }
    }
}

/// One non-empty chunk of a query. Iterate it with [`QueryChunk::iter`] or by value
/// (`for item in chunk {}`).
pub struct QueryChunk<'q, T: TQueryParam>
{
    range:   RowRange<T>,
    ticks:   QueryTicks,
    phantom: PhantomData<&'q ()>,
}

impl<'q, T: TQueryParam> QueryChunk<'q, T>
{
    /// Rows in this chunk, before any filter is applied
    #[inline]
    pub fn len(&self) -> usize
    {
        self.range.end - self.range.start
    }

    /// Always `false`, `iter_chunk` skips empty chunks. Here to keep clippy happy about `len`.
    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.len() == 0
    }

    /// Walks this chunk's rows. The items borrow the chunk, so calling this twice cannot hand
    /// out two `&mut` for the same row.
    #[inline]
    pub fn iter(&mut self) -> ChunkIter<'_, T>
    {
        ChunkIter::new(self.range, self.ticks)
    }
}

impl<'q, T: TQueryParam> IntoIterator for QueryChunk<'q, T>
{
    type Item = T::QueryItem<'q>;
    type IntoIter = ChunkIter<'q, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter
    {
        ChunkIter::new(self.range, self.ticks)
    }
}

/// Every non-empty chunk of a query. Returned by [`Query::iter_chunk`](crate::query::Query::iter_chunk).
pub struct QueryChunks<'q, T: TQueryParam>
{
    cursor: ChunkCursor<'q, T>,
    ticks:  QueryTicks,
}

impl<'q, T: TQueryParam> QueryChunks<'q, T>
{
    pub(crate) fn new(accessor: &QuerySpecAccessor<'q>) -> Self
    {
        Self {
            cursor: ChunkCursor::new(accessor),
            ticks:  ticks_of(accessor),
        }
    }
}

impl<'q, T: TQueryParam> Iterator for QueryChunks<'q, T>
{
    type Item = QueryChunk<'q, T>;

    #[inline]
    #[track_caller]
    fn next(&mut self) -> Option<Self::Item>
    {
        let (fetch, len) = self.cursor.next_chunk()?;
        Some(QueryChunk {
            range:   RowRange { fetch, start: 0, end: len },
            ticks:   self.ticks,
            phantom: PhantomData,
        })
    }
}

/// Up to `batch_amount` rows of a query, which may span several chunks and archetypes.
///
/// The count is taken before filters, so a batch of a `Changed<&Hp>` query can yield fewer
/// items than its size, or none at all. Only the last batch of a query can be smaller than
/// `batch_amount`.
///
/// A batch is `Send`, so it can be handed to another thread and walked there.
pub struct QueryBatch<'q, T: TQueryParam>
{
    ranges:  Vec<RowRange<T>>,
    ticks:   QueryTicks,
    phantom: PhantomData<&'q ()>,
}

// SAFETY: a batch is only raw pointers into chunks plus two ticks. Sending it is sound because:
// - `TComponent` requires `Send + Sync`, so handing out `&T` or `&mut T` on another thread is fine
// - batches of one query cover disjoint rows, so no two threads get a `&mut` to the same row,
//   and a `Mut` stamps the changed tick of its own row only
// - the `Query` stays borrowed while any batch lives, so the world cannot move or free a chunk
//   under it
unsafe impl<'q, T: TQueryParam> Send for QueryBatch<'q, T> {}

impl<'q, T: TQueryParam> QueryBatch<'q, T>
{
    /// Rows in this batch, before any filter is applied
    #[inline]
    pub fn len(&self) -> usize
    {
        self.ranges.iter().map(|range| range.end - range.start).sum()
    }

    /// Always `false`, `iter_batch` never hands out an empty batch
    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.ranges.is_empty()
    }

    /// Walks this batch's rows. The items borrow the batch, so calling this twice cannot hand
    /// out two `&mut` for the same row.
    #[inline]
    pub fn iter(&mut self) -> BatchIter<'_, T, &[RowRange<T>]>
    {
        BatchIter::new(&self.ranges, self.ticks)
    }
}

impl<'q, T: TQueryParam> IntoIterator for QueryBatch<'q, T>
{
    type Item = T::QueryItem<'q>;
    type IntoIter = BatchIter<'q, T, Vec<RowRange<T>>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter
    {
        BatchIter::new(self.ranges, self.ticks)
    }
}

/// Hands out the rows of a [`QueryBatch`]. `R` is either the batch's own `Vec` (iterating by
/// value) or a slice of it (iterating through [`QueryBatch::iter`]).
pub struct BatchIter<'q, T: TQueryParam, R: AsRef<[RowRange<T>]>>
{
    ranges:    R,
    range_pos: usize,
    rows:      Option<ChunkIter<'q, T>>,
    ticks:     QueryTicks,
}

impl<'q, T: TQueryParam, R: AsRef<[RowRange<T>]>> BatchIter<'q, T, R>
{
    #[inline]
    fn new(ranges: R, ticks: QueryTicks) -> Self
    {
        Self {
            ranges:    ranges,
            range_pos: 0,
            rows:      None,
            ticks:     ticks,
        }
    }
}

impl<'q, T: TQueryParam, R: AsRef<[RowRange<T>]>> Iterator for BatchIter<'q, T, R>
{
    type Item = T::QueryItem<'q>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        loop
        {
            if let Some(rows) = &mut self.rows
                && let Some(item) = rows.next()
            {
                return Some(item);
            }

            let range = *self.ranges.as_ref().get(self.range_pos)?;
            self.range_pos += 1;
            self.rows = Some(ChunkIter::new(range, self.ticks));
        }
    }
}

/// Cuts a query into batches of `batch_amount` rows. Returned by
/// [`Query::iter_batch`](crate::query::Query::iter_batch).
pub struct QueryBatches<'q, T: TQueryParam>
{
    cursor:       ChunkCursor<'q, T>,
    ticks:        QueryTicks,
    batch_amount: usize,
    // the rest of a chunk the previous batch did not have room for
    leftover:     Option<RowRange<T>>,
}

impl<'q, T: TQueryParam> QueryBatches<'q, T>
{
    #[track_caller]
    pub(crate) fn new(accessor: &QuerySpecAccessor<'q>, batch_amount: usize) -> Self
    {
        assert!(batch_amount > 0, "`iter_batch` needs a batch_amount above 0");
        Self {
            cursor:       ChunkCursor::new(accessor),
            ticks:        ticks_of(accessor),
            batch_amount: batch_amount,
            leftover:     None,
        }
    }
}

impl<'q, T: TQueryParam> Iterator for QueryBatches<'q, T>
{
    type Item = QueryBatch<'q, T>;

    #[track_caller]
    fn next(&mut self) -> Option<Self::Item>
    {
        let mut ranges = Vec::new();
        let mut room = self.batch_amount;

        while room > 0
        {
            let range = match self.leftover.take()
            {
                Some(range) => range,
                None => match self.cursor.next_chunk()
                {
                    Some((fetch, len)) => RowRange { fetch, start: 0, end: len },
                    None => break,
                },
            };

            let take = room.min(range.end - range.start);
            let split = range.start + take;
            ranges.push(RowRange { end: split, ..range });
            room -= take;

            if split < range.end
            {
                self.leftover = Some(RowRange { start: split, ..range });
            }
        }

        if ranges.is_empty()
        {
            return None;
        }
        Some(QueryBatch {
            ranges:  ranges,
            ticks:   self.ticks,
            phantom: PhantomData,
        })
    }
}

#[inline]
fn ticks_of(accessor: &QuerySpecAccessor<'_>) -> QueryTicks
{
    QueryTicks {
        last_run: accessor.last_run_tick,
        this_run: accessor.this_run_tick,
    }
}

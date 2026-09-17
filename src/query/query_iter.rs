use std::marker::PhantomData;
use std::mem::MaybeUninit;

use crate::apis::internal_traits::{QueryTicks, TQueryParam};
use crate::archetype::Archetype;
use crate::world::arch_spec::{ArchetypeSpec, ArchetypeSpecs};
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
            arch_indices: if T::EXCLUDE_ONLY { &[] } else { accessor.arch_indices() },
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
///
/// The current chunk's cursor is kept flat in here rather than as an `Option<ChunkIter>`. Once
/// this loop gets inlined into the caller, the nested form keeps LLVM from hoisting the
/// `row < end` check and costs about 3x on a plain `position += velocity` pass.
pub struct QueryIter<'q, T: TQueryParam>
{
    cursor:  ChunkCursor<'q, T>,
    // only initialized once the first chunk is reached, and only read while `row < end`
    fetch:   MaybeUninit<T::Fetch>,
    row:     usize,
    end:     usize,
    ticks:   QueryTicks,
    phantom: PhantomData<&'q ()>,
}

impl<'q, T: TQueryParam> QueryIter<'q, T>
{
    pub(crate) fn new(accessor: &QuerySpecAccessor<'q>) -> Self
    {
        Self {
            cursor:  ChunkCursor::new(accessor),
            fetch:   MaybeUninit::uninit(),
            row:     0,
            end:     0,
            ticks:   ticks_of(accessor),
            phantom: PhantomData,
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
            let row = self.row;
            if row < self.end
            {
                self.row = row + 1;
                // SAFETY: `end` only goes above 0 after `fetch` was written for the same chunk,
                // `row` is below that chunk's length, and no other live walk covers this row
                unsafe {
                    let fetch = self.fetch.assume_init_ref();
                    if T::accepts(fetch, row, self.ticks)
                    {
                        return Some(T::fetch(fetch, row, self.ticks));
                    }
                }
                continue;
            }

            let (fetch, len) = self.cursor.next_chunk()?;
            self.fetch = MaybeUninit::new(fetch);
            self.row = 0;
            self.end = len;
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
/// items than its size, or none at all. A batch can hold up to 7 rows fewer than `batch_amount`,
/// since a cut inside a chunk snaps to [`MIN_BATCH_AMOUNT`], and the last one can be smaller still.
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

/// The smallest `batch_amount` [`Query::iter_batch`](crate::query::Query::iter_batch) accepts,
/// and the grid every cut inside a chunk snaps to.
///
/// The enable bits of a chunk pack 8 rows into one byte, and `Detail<&mut T>` writes that byte
/// from whatever thread holds the batch. So two batches must never share a byte: a cut in the
/// middle of a chunk is rounded down to a multiple of 8. If some code ever reads or writes that
/// region a whole `u64` at a time while batches run, this has to grow to 64.
pub const MIN_BATCH_AMOUNT: usize = crate::apis::constants::BITS_PER_BYTE;

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
        assert!(
            batch_amount >= MIN_BATCH_AMOUNT,
            "`iter_batch` needs a batch_amount of at least {MIN_BATCH_AMOUNT}, got {batch_amount}"
        );
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

            // A cut inside a chunk has to land on a byte border of the enable region, see
            // `MIN_BATCH_AMOUNT`. The end of a chunk is always fine, the next chunk has its own
            // region. Every `start` is 0 or an earlier cut, so it is already on a border.
            debug_assert!(range.start % MIN_BATCH_AMOUNT == 0);
            let split = if range.start + room >= range.end
            {
                range.end
            }
            else
            {
                (range.start + room) / MIN_BATCH_AMOUNT * MIN_BATCH_AMOUNT
            };

            if split == range.start
            {
                // not enough room left for a whole byte, the batch closes a little early. Cannot
                // happen on the first range: `room` then is at least `MIN_BATCH_AMOUNT`.
                self.leftover = Some(range);
                break;
            }

            ranges.push(RowRange { end: split, ..range });
            room -= split - range.start;

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

/// A row of a query, as `[matched archetype, chunk, row]`. This is what a parallel job gets as
/// its starting point, see [`TJobExecutor`](crate::schedule::executor::TJobExecutor).
pub(crate) type RowPos = crate::schedule::executor::JobArgs;

/// The walking side of `Query::par_for_each_chunk` and `Query::par_for_each_batch`.
///
/// It keeps no cursor state of its own, so one walker is shared by every job: the caller lists
/// where each job starts ([`chunk_starts`](Self::chunk_starts), [`batch_starts`](Self::batch_starts))
/// and every job walks forward from its own start.
pub(crate) struct ParWalker<'q, T: TQueryParam>
{
    archetypes:   &'q ArchetypeSpecs,
    arch_indices: &'q [usize],
    ticks:        QueryTicks,
    phantom:      PhantomData<T>,
}

// SAFETY: the walker only reads the world's registries, which do not change while the `Query`
// that built it stays borrowed. Rows it hands out follow the same rules as `QueryBatch`.
unsafe impl<'q, T: TQueryParam> Sync for ParWalker<'q, T> {}

impl<'q, T: TQueryParam> ParWalker<'q, T>
{
    pub(crate) fn new(accessor: &QuerySpecAccessor<'q>) -> Self
    {
        Self {
            archetypes:   accessor.archetypes,
            arch_indices: if T::EXCLUDE_ONLY { &[] } else { accessor.arch_indices() },
            ticks:        ticks_of(accessor),
            phantom:      PhantomData,
        }
    }

    #[inline]
    #[track_caller]
    fn arch_at(&self, arch_pos: usize) -> &'q ArchetypeSpec
    {
        let arch_idx = self.arch_indices[arch_pos];
        match self.archetypes.value_at(arch_idx)
        {
            Some(arch_spec) => arch_spec,
            None => panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry"),
        }
    }

    /// The first row of the first non-empty chunk at or after `[arch_pos, chunk_idx]`, or `None`
    /// once every matched archetype is used up
    #[inline]
    fn next_non_empty(&self, mut arch_pos: usize, mut chunk_idx: usize) -> Option<RowPos>
    {
        while arch_pos < self.arch_indices.len()
        {
            let arch = &self.arch_at(arch_pos).arch;
            while chunk_idx < arch.chunk_count()
            {
                if !arch.chunk_at(chunk_idx).is_empty()
                {
                    return Some([arch_pos, chunk_idx, 0]);
                }
                chunk_idx += 1;
            }
            arch_pos += 1;
            chunk_idx = 0;
        }
        None
    }

    /// Where every non-empty chunk starts
    pub(crate) fn chunk_starts(&self) -> impl Iterator<Item = RowPos> + '_
    {
        std::iter::successors(self.next_non_empty(0, 0), |&[arch_pos, chunk_idx, _]| self.next_non_empty(arch_pos, chunk_idx + 1))
    }

    /// Where every batch of `batch_amount` rows starts, cut exactly like `QueryBatches` cuts
    #[track_caller]
    pub(crate) fn batch_starts(&self, batch_amount: usize) -> impl Iterator<Item = RowPos> + '_
    {
        assert!(
            batch_amount >= MIN_BATCH_AMOUNT,
            "`par_for_each_batch` needs a batch_amount of at least {MIN_BATCH_AMOUNT}, got {batch_amount}"
        );
        std::iter::successors(self.next_non_empty(0, 0), move |&start| self.cut_batch(start, batch_amount, |_, _, _| {}))
    }

    /// Hands `f` every row of the chunk starting at `start`
    ///
    /// # Safety
    /// `start` must come from [`chunk_starts`](Self::chunk_starts) of this walker, and no other
    /// live walk may cover the same chunk.
    #[inline]
    pub(crate) unsafe fn walk_chunk<F: Fn(T::QueryItem<'q>)>(&self, start: RowPos, f: &F)
    {
        let [arch_pos, chunk_idx, _] = start;
        let arch_spec = self.arch_at(arch_pos);
        let chunk = arch_spec.arch.chunk_at(chunk_idx);
        self.walk_rows(arch_spec, chunk_idx, 0, chunk.len(), f);
    }

    /// Hands `f` every row of the batch starting at `start`
    ///
    /// # Safety
    /// `start` must come from [`batch_starts`](Self::batch_starts) of this walker with the same
    /// `batch_amount`, and no other live walk may cover the same rows.
    #[inline]
    pub(crate) unsafe fn walk_batch<F: Fn(T::QueryItem<'q>)>(&self, start: RowPos, batch_amount: usize, f: &F)
    {
        self.cut_batch(start, batch_amount, |arch_spec, chunk_idx, [row, end]| self.walk_rows(arch_spec, chunk_idx, row, end, f));
    }

    #[inline]
    fn walk_rows<F: Fn(T::QueryItem<'q>)>(&self, arch_spec: &'q ArchetypeSpec, chunk_idx: usize, start: usize, end: usize, f: &F)
    {
        let state = T::arch_state(&arch_spec.layout);
        let chunk = arch_spec.arch.chunk_at(chunk_idx);
        // SAFETY: `state` was resolved from the layout this chunk was built from
        let fetch = unsafe { T::fetch_init(state, chunk.ptr()) };
        ChunkIter::<'q, T>::new(RowRange { fetch, start, end }, self.ticks).for_each(f);
    }

    /// Walks one batch from `start`, calling `visit` on each `[row, end]` piece of a chunk it
    /// covers, and returns where the next batch starts. The cut rules are the ones of
    /// `QueryBatches::next`, so both ways of batching agree on every border.
    #[inline]
    fn cut_batch(&self, start: RowPos, batch_amount: usize, mut visit: impl FnMut(&'q ArchetypeSpec, usize, [usize; 2])) -> Option<RowPos>
    {
        let mut pos = start;
        let mut room = batch_amount;

        while room > 0
        {
            let [arch_pos, chunk_idx, row] = pos;
            let arch_spec = self.arch_at(arch_pos);
            let len = arch_spec.arch.chunk_at(chunk_idx).len();

            // see `MIN_BATCH_AMOUNT` for why a cut inside a chunk snaps to a byte border
            debug_assert!(row % MIN_BATCH_AMOUNT == 0);
            let split = if row + room >= len { len } else { (row + room) / MIN_BATCH_AMOUNT * MIN_BATCH_AMOUNT };

            if split == row
            {
                // not enough room left for a whole byte, the next batch starts right here
                return Some(pos);
            }

            visit(arch_spec, chunk_idx, [row, split]);
            room -= split - row;

            if split < len
            {
                return Some([arch_pos, chunk_idx, split]);
            }
            pos = self.next_non_empty(arch_pos, chunk_idx + 1)?;
        }
        Some(pos)
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

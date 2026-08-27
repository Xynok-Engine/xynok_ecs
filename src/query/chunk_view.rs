//! Splitting a [`Query`] by chunk, so each job gets one contiguous block.
//!
//! # Why the unit is a chunk, not an entity
//!
//! A chunk is already contiguous in memory: its `&[C]` is a real slice, not a list of pointers.
//! Splitting by entity means cutting a slice in half; splitting by archetype gives batches whose
//! sizes differ by an order of magnitude, because every archetype holds a different number of
//! entities. A chunk sits in between: fixed size, and its boundary is already a memory boundary.
//!
//! # Picking a batch size
//!
//! `batch` counts **chunks**, not entities, and no default value is right everywhere. The question
//! that derives it is "how long does one chunk take": aim for 20 microseconds per job, so a chunk
//! measured at 2 µs gives `batch = 10`. Below the threshold, meaning `batch` exceeds the total
//! chunk count, nothing is spawned and everything runs on the calling thread.
//!
//! # Structural change
//!
//! While `par_for_each_chunk` runs, nobody may create, destroy, add or remove: those move rows out
//! from under the jobs reading them. They go through [`crate::cmd_buffer::Commands`] instead, and
//! become real at the synchronisation point that ends the step.

use xynok_concurrency::pool::ThreadPool;

use crate::apis::internal_traits::TQueryParam;
use crate::entity::Entity;
use crate::query::Query;
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::QuerySpecAccessor;

/// One chunk, already opened up into column slices.
///
/// `entities[i]` is the entity of row `i` in every slice of [`Self::columns`], which also makes
/// this the only place to get an entity id at all: [`Entity`] is not a component column, it lives
/// in the chunk header.
pub struct ChunkView<'a, T: TQueryParam + 'static>
{
    /// The entity of each row, in the same order as the column slices.
    pub entities: &'a [Entity],
    /// `&[C]`, `&mut [C]`, or a tuple of those, depending on the query.
    pub columns:  T::ChunkColumns<'a>,
}

impl<'a, T: TQueryParam + 'static> ChunkView<'a, T>
{
    /// Rows in this chunk. Never zero: empty chunks are skipped before the closure sees them.
    #[inline]
    pub fn len(&self) -> usize
    {
        self.entities.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.entities.is_empty()
    }
}

/// Lets a [`QuerySpecAccessor`] travel into a job.
///
/// It is a handful of pointers into the world's boxed registries. They outlive the
/// `par_for_each_chunk` call, and for the whole of that call nobody may change the world's
/// structure, so reading them from several threads is reading data that does not move. That is
/// exactly the condition `Sync` asks for.
struct SharedAccessor(QuerySpecAccessor);
unsafe impl Send for SharedAccessor {}
unsafe impl Sync for SharedAccessor {}

impl SharedAccessor
{
    /// The world's archetype registry.
    ///
    /// A method rather than a field read, and that is deliberate: Rust closures capture individual
    /// fields, so `accessor.0.archetypes` would capture the raw pointer itself, which is not
    /// `Sync`, and the `unsafe impl` above would buy nothing. A method call captures the whole
    /// `accessor`.
    ///
    /// # Safety
    ///
    /// The world must still be alive.
    #[inline]
    unsafe fn archetypes(&self) -> &ArchetypeSpecs
    {
        unsafe { &*self.0.archetypes }
    }
}

/// Where each archetype sits in a query's flattened chunk sequence.
///
/// One `Vec` entry per **archetype**, not per chunk: a large world has tens of thousands of chunks
/// but only a few dozen archetypes, so this table stays small and rebuilding it per call costs
/// nothing worth measuring.
struct ChunkIndex
{
    /// `(archetype index, global chunk number of its first chunk)`, ascending.
    entries: Vec<(usize, usize)>,
    total:   usize,
}

impl ChunkIndex
{
    fn build(archetypes: &ArchetypeSpecs, arch_indices: &[usize]) -> Self
    {
        let mut entries = Vec::with_capacity(arch_indices.len());
        let mut total = 0usize;

        for arch_idx in arch_indices
        {
            let Some(arch_spec) = archetypes.value_at(*arch_idx)
            else
            {
                panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry");
            };
            let count = arch_spec.arch.chunk_count();
            if count == 0
            {
                continue;
            }
            entries.push((*arch_idx, total));
            total += count;
        }

        Self {
            entries: entries,
            total:   total,
        }
    }

    /// Global chunk number -> `(archetype index, chunk index inside that archetype)`.
    #[inline]
    fn locate(&self, global: usize) -> (usize, usize)
    {
        let slot = self.entries.partition_point(|(_, start)| *start <= global) - 1;
        let (arch_idx, start) = self.entries[slot];
        (arch_idx, global - start)
    }
}

/// Builds the view for one chunk, or `None` when that chunk is empty.
///
/// # Safety
///
/// No other job may hold the same `(arch_idx, chunk_idx)`, because `T::ChunkColumns` can be a
/// `&mut` slice.
#[inline]
#[track_caller]
unsafe fn view_at<'v, T: TQueryParam + 'static>(archetypes: &ArchetypeSpecs, arch_idx: usize, chunk_idx: usize) -> Option<ChunkView<'v, T>>
{
    let Some(arch_spec) = archetypes.value_at(arch_idx)
    else
    {
        panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry");
    };

    let chunk = arch_spec.arch.chunk_at(chunk_idx);
    if chunk.is_empty()
    {
        return None;
    }

    let entities = match chunk.get_entities(&arch_spec.layout)
    {
        Ok(r) => r,
        Err(e) => panic!("{}", e),
    };

    Some(ChunkView {
        entities: entities,
        // Safety: this archetype comes from the query's pre-filtered list, so it carries every
        // column; the one-chunk-per-job condition is this function's own contract.
        columns:  unsafe { T::chunk_columns(arch_spec, chunk) },
    })
}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    /// Runs `f` once per non-empty chunk, on the calling thread.
    ///
    /// The sequential twin of [`Self::par_for_each_chunk`], and also the way to write a loop over
    /// slices rather than rows when parallelism is not wanted.
    #[track_caller]
    pub fn for_each_chunk<F>(self, mut f: F)
    where F: FnMut(ChunkView<'_, T>)
    {
        // Safety: the accessor stays valid as long as the world lives, which is what the query's
        // `'a` states.
        let archetypes = unsafe { &*self.accessor.archetypes };
        let arch_indices = unsafe { self.accessor.arch_indices() };

        for arch_idx in arch_indices
        {
            let Some(arch_spec) = archetypes.value_at(*arch_idx)
            else
            {
                panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry");
            };

            for chunk_idx in 0..arch_spec.arch.chunk_count()
            {
                // Safety: one thread, one chunk at a time.
                if let Some(view) = unsafe { view_at::<T>(archetypes, *arch_idx, chunk_idx) }
                {
                    f(view);
                }
            }
        }
    }

    /// Runs `f` once per non-empty chunk, handing the pool batches of `batch` chunks.
    ///
    /// Every chunk reaches exactly one job, so two `&mut` slices never point at the same memory.
    /// The calling thread does not park while it waits: it picks up a batch and runs it.
    ///
    /// `batch >= total chunk count` means everything runs on the calling thread with nothing
    /// spawned. See the module notes for how to derive `batch`.
    ///
    /// # Panics
    ///
    /// If `f` panics. The panic is caught in the job and rethrown here once every other batch has
    /// finished.
    #[track_caller]
    pub fn par_for_each_chunk<F>(self, pool: &ThreadPool, batch: usize, f: F)
    where F: Fn(ChunkView<'_, T>) + Sync
    {
        // Safety: as in `for_each_chunk`.
        let archetypes = unsafe { &*self.accessor.archetypes };
        let arch_indices = unsafe { self.accessor.arch_indices() };

        let index = ChunkIndex::build(archetypes, arch_indices);
        if index.total == 0
        {
            return;
        }

        let accessor = SharedAccessor(self.accessor);

        pool.parallel_for(index.total, batch.max(1), |global| {
            // Safety: `parallel_for` hands each index to exactly one job, so each chunk reaches
            // exactly one job too.
            let archetypes = unsafe { accessor.archetypes() };
            let (arch_idx, chunk_idx) = index.locate(global);

            if let Some(view) = unsafe { view_at::<T>(archetypes, arch_idx, chunk_idx) }
            {
                f(view);
            }
        });
    }
}

use xynok_concurrency::thread_pool::ThreadPool;

/// What one job gets to know which piece of work is its own. Three `usize` are enough to point
/// at a row of a query (matched archetype, chunk, row), and small enough that a job still fits
/// in the pool's inline job slot, so nothing is ever boxed.
pub type JobArgs = [usize; 3];

/// Where the world sends work that can run in parallel, like the jobs of
/// `Query::par_for_each_chunk` or a parallel system group.
///
/// The scheduler only needs somewhere to run jobs, not the whole `TScheduler`, so this is kept
/// apart and small enough to live behind `Box<dyn _>` inside the `World`. Plug your own in with
/// [`World::set_executor`](crate::world::World::set_executor).
///
/// There is no generic closure type here on purpose: every job is the same shared `job` function
/// called with its own arguments, which keeps the trait object-safe without a heap allocation
/// per job.
pub trait TJobExecutor: Send + Sync
{
    /// Calls `job(args)` once for every item of `args`, and only returns once all of those calls
    /// are done, so `job` may borrow anything the caller holds. `args` is fully read before any
    /// job starts, and the calling thread may run some of the jobs itself.
    fn run_jobs(&self, args: &mut dyn Iterator<Item = JobArgs>, job: &(dyn Fn(JobArgs) + Sync));

    /// Calls `job(0)`, `job(1)`, ..., `job(count - 1)`, the same way [`run_jobs`](Self::run_jobs) does
    #[inline]
    fn run_indexed(&self, count: usize, job: &(dyn Fn(usize) + Sync))
    {
        self.run_jobs(&mut (0..count).map(|i| [i, 0, 0]), &|[i, _, _]| job(i));
    }
}

impl TJobExecutor for ThreadPool
{
    #[inline]
    fn run_jobs(&self, args: &mut dyn Iterator<Item = JobArgs>, job: &(dyn Fn(JobArgs) + Sync))
    {
        self.run_batch(args.map(|args| move || job(args)));
    }
}

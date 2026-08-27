//! A schedule is a list of **steps**, run one after another.
//!
//! Each step is either a single system or a group of systems that run at the same time. A list of
//! steps is all an ECS needs here: a full job graph would be more precise in theory, but most games
//! have no cross-dependency to exploit, while "split into steps and join at the end of each" reads
//! plainly and diffs cleanly.
//!
//! # Parallelism is declared, not inferred
//!
//! Xynok does not guess which systems may run together. The game author says so via
//! [`TScheduler::add_system_parallel`], and `AccessScopes::can_parallel_with` acts as the referee:
//! a wrong declaration panics at the call site, not during some later `run`.
//!
//! What that buys: an inferred DAG can reorder itself between two builds just because someone added
//! a component to a query, and a bug caused by systems changing order takes a week to find. Declared
//! by hand, the order lives in user code where it can be read and diffed.

use std::collections::HashMap;
use std::hash::Hash;

use xynok_concurrency::pool::{Config as LaneConfig, ThreadPool};
use xynok_std::unsafe_ptr::{HeapMut, HeapPtr};

use crate::schedule::system_spec::SystemSpecs;
use crate::system::traits::{SystemTypeStorage, TIntoSystem, TIntoSystems};
use crate::world::World;

pub trait TScheduler: Sized
{
    type SessionType: Eq + Hash + Clone;

    #[track_caller]
    fn new(world: HeapPtr<World>) -> Self;

    #[track_caller]
    fn add_system<P, T: TIntoSystem<P>>(&mut self, session: Self::SessionType, system: T) -> &mut Self;

    /// Declares a group of systems that run at the same time, as **one** step.
    ///
    /// # Panics
    ///
    /// If any two systems in the group have conflicting access scopes. Checked here rather than at
    /// `run`, in the same spirit as [`Self::add_system`] catching a system whose own parameters
    /// alias each other.
    #[track_caller]
    fn add_system_parallel<P, T: TIntoSystems<P>>(&mut self, session: Self::SessionType, systems: T) -> &mut Self;

    #[track_caller]
    fn run(&mut self, session: Self::SessionType);
}

/// One link in a session's list of steps.
pub enum ScheduleStep
{
    /// A single system, run on the calling thread.
    Single(SystemTypeStorage),
    /// A group declared safe to run together: spawn, then join.
    Parallel(Vec<SystemTypeStorage>),
}

impl ScheduleStep
{
    /// How many systems this step holds.
    pub fn len(&self) -> usize
    {
        match self
        {
            Self::Single(_) => 1,
            Self::Parallel(group) => group.len(),
        }
    }

    pub fn is_empty(&self) -> bool
    {
        self.len() == 0
    }
}

pub struct DefaultScheduler
{
    world: HeapPtr<World>,
    steps: HashMap<DefaultScheduleSession, Vec<ScheduleStep>>,
    /// Keyed by system type, so the same `fn` added to two sessions is described once. Only
    /// safe because everything in a spec is derived from the system's type - anything
    /// per-instance would have to live beside the boxed system in `steps` instead.
    specs: SystemSpecs,
    /// Lane A's pool. `world` holds a clone of it, see [`World::bind_lane`].
    pool:  ThreadPool,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum DefaultScheduleSession
{
    Start,
    PreUpdate,
    Update,
    LateUpdate,
    PreFixedUpdate,
    FixedUpdate,
    LateFixedUpdate,
    AppQuit,
}

impl TScheduler for DefaultScheduler
{
    type SessionType = DefaultScheduleSession;

    fn add_system<P, T: TIntoSystem<P>>(&mut self, session: Self::SessionType, s: T) -> &mut Self
    {
        let s = match s.into_system()
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };

        self.register(&s);
        self.steps.entry(session).or_default().push(ScheduleStep::Single(s));
        self
    }

    fn add_system_parallel<P, T: TIntoSystems<P>>(&mut self, session: Self::SessionType, systems: T) -> &mut Self
    {
        let group = match systems.into_systems()
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };

        for s in group.iter()
        {
            self.register(s);
        }
        // The user's declaration is judged here, at the call site, not at `run`
        if let Err(e) = self.specs.check_group_can_parallel(&group)
        {
            panic!("{}", e);
        }

        self.steps.entry(session).or_default().push(ScheduleStep::Parallel(group));
        self
    }

    fn run(&mut self, session: Self::SessionType)
    {
        // Destructured so that `steps` can be borrowed mutably while `world` and `pool` stay
        // usable inside the same loop
        let Self { world, steps, pool, .. } = self;

        let Some(steps) = steps.get_mut(&session)
        else
        {
            return;
        };

        let world_ptr = world.as_ref_mut();
        for step in steps.iter_mut()
        {
            match step
            {
                ScheduleStep::Single(s) => run_system(s, world_ptr),
                ScheduleStep::Parallel(group) => run_group(pool, group, world_ptr),
            }
            // The synchronisation point that ends the step: every job of this step is done, so
            // this is the only place in the frame where a structural change pulls no row out from
            // under anybody
            world.apply_commands();
        }
    }

    fn new(world: HeapPtr<World>) -> Self
    {
        Self::with_lane_config(world, LaneConfig::from_env())
    }
}

impl DefaultScheduler
{
    /// [`TScheduler::new`] with a pool config of your own.
    ///
    /// `LaneConfig::inline()` gives a scheduler that spawns no thread at all: parallel groups still
    /// run, just one system after another on the calling thread. It is the fastest way to answer
    /// "is this bug about threading?".
    pub fn with_lane_config(world: HeapPtr<World>, config: LaneConfig) -> Self
    {
        Self::with_lane(world, ThreadPool::new(config))
    }

    /// [`TScheduler::new`] sharing a pool that already exists.
    ///
    /// What you want once the engine builds lane A in `main`: the ECS should be a guest of that
    /// pool rather than stand up a second one beside it.
    pub fn with_lane(mut world: HeapPtr<World>, pool: ThreadPool) -> Self
    {
        world.bind_lane(&pool);
        Self {
            world: world,
            steps: HashMap::new(),
            specs: SystemSpecs::default(),
            pool:  pool,
        }
    }

    /// The pool that parallel steps run on.
    #[inline]
    pub fn lane(&self) -> &ThreadPool
    {
        &self.pool
    }

    /// A session's steps, in the order they run.
    pub fn steps(&self, session: DefaultScheduleSession) -> &[ScheduleStep]
    {
        match self.steps.get(&session)
        {
            Some(steps) => steps.as_slice(),
            None => &[],
        }
    }

    /// Records a system's spec before it ever gets a chance to run.
    ///
    /// That is what reports a system whose parameters alias each other at the `add_system` call
    /// site rather than at the first `run`.
    #[track_caller]
    fn register(&mut self, s: &SystemTypeStorage)
    {
        if let Err(e) = self.specs.register(s.as_ref(), &mut self.world.component_counter)
        {
            panic!("{}: {}", s.name(), e);
        }
    }
}

/// Performs every parameter's writing half up front, on the calling thread. See
/// [`crate::system::traits::TSystem::prepare`].
#[track_caller]
fn prepare_system(system: &SystemTypeStorage, world: HeapMut<World>)
{
    if let Err(e) = system.prepare(world)
    {
        panic!("{}: {}", system.name(), e);
    }
}

#[track_caller]
fn run_prepared(system: &mut SystemTypeStorage, world: HeapMut<World>)
{
    match system.run(world)
    {
        Ok(_) =>
        {}
        Err(e) => panic!("{}", e),
    }
}

#[track_caller]
fn run_system(system: &mut SystemTypeStorage, world: HeapMut<World>)
{
    prepare_system(system, world);
    run_prepared(system, world);
}

/// Spawns the whole group, then waits.
///
/// The calling thread does **not** park while waiting: `scope` waits by working, so it picks up one
/// of the jobs it just spawned. That is why the pool runs `N = cores - 1` threads.
///
/// A one-element group runs straight through: pushing a single job into the pool and then waiting
/// for exactly that job means paying the spawn cost to buy back the seat you already had.
#[track_caller]
fn run_group(pool: &ThreadPool, group: &mut [SystemTypeStorage], world: HeapMut<World>)
{
    if let [only] = group
    {
        run_system(only, world);
        return;
    }

    // The preparation pass, on this very thread: initialising a query writes into the world's
    // registries, and two jobs writing there at once is a race. After this pass, `init` inside a
    // job is a table lookup, which is a read, and concurrent reads are fine.
    for system in group.iter()
    {
        prepare_system(system, world);
    }

    pool.scope(|s| {
        for system in group.iter_mut()
        {
            s.spawn(move || run_prepared(system, world));
        }
    });
}

#[allow(unused)]
#[cfg(test)]
mod test
{
    use std::collections::HashMap;

    use crate::apis::traits::TComponent;
    use crate::query::Query;
    use crate::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
    use crate::world::World;
    use xynok_ecs_proc_macro::component;
    use xynok_std::unsafe_ptr::HeapPtr;

    #[component]
    struct Hp(u64);
    #[component]
    struct Mana(u64);
    fn system_a()
    {
        println!("system a running !")
    }
    fn system_b(query: Query<&Hp>)
    {
        println!("system b running !");
        for hp in query
        {
            println!("hp({})", hp.0);
        }
    }

    /// Both parameters match the `(Hp, Mana)` archetype, so the body would hold `&Hp` and
    /// `&mut Hp` for the same row
    fn system_aliasing_hp(query: Query<&Hp>, query2: Query<&mut Hp>)
    {
        for hp in query
        {
            println!("hp({})", hp.0);
        }
    }
    fn system_c(query: Query<(&Hp, &Mana)>)
    {
        println!("system c running !");
        for (hp, mana) in query
        {
            println!("hp({}) - mana({})", hp.0, mana.0);
        }
    }

    /// Both parameters are initialised before the body runs, so two accessors are alive at
    /// once. Building the second one registers a new `QuerySpec`, which grows the query
    /// registry and relocates the first one's spec - the first accessor has to survive that.
    fn system_two_queries(hp_query: Query<&Hp>, mana_query: Query<&Mana>)
    {
        let hp_total: u64 = hp_query.into_iter().map(|hp| hp.0).sum();
        let mana_total: u64 = mana_query.into_iter().map(|mana| mana.0).sum();

        assert_eq!(hp_total, 24, "the first query read through a relocated QuerySpec");
        assert_eq!(mana_total, 12, "the second query read the wrong rows");
    }

    #[test]
    fn test_two_queries_in_one_system()
    {
        let mut world = HeapPtr::new(World::default());
        world.create(Hp(12));
        world.create((Hp(12), Mana(12)));
        let mut scheduler = DefaultScheduler::new(world);

        scheduler.add_system(DefaultScheduleSession::Start, system_two_queries);
        // Runs twice on purpose: the first pass registers both queries (the relocating case),
        // the second takes the already-cached path
        scheduler.run(DefaultScheduleSession::Start);
        scheduler.run(DefaultScheduleSession::Start);
    }

    #[test]
    fn test_scheduler()
    {
        let mut world = HeapPtr::new(World::default());
        world.create(Hp(12));
        world.create((Hp(12), Mana(12)));
        let mut scheduler = DefaultScheduler::new(world);

        scheduler
            .add_system(DefaultScheduleSession::Start, system_a)
            .add_system(DefaultScheduleSession::Start, system_b)
            .add_system(DefaultScheduleSession::Start, system_c);

        scheduler.run(DefaultScheduleSession::Start);
    }

    /// The conflict is rejected when the system is added, not when it first runs, and it is
    /// rejected regardless of how many threads the schedule would use
    #[test]
    #[should_panic(expected = "conflicting")]
    fn aliasing_parameters_are_rejected_at_add_system()
    {
        let mut world = HeapPtr::new(World::default());
        world.create((Hp(12), Mana(12)));
        let mut scheduler = DefaultScheduler::new(world);

        scheduler.add_system(DefaultScheduleSession::Start, system_aliasing_hp);
    }

    /// Two queries naming the same component are fine as long as neither writes it
    #[test]
    fn two_readers_of_one_component_are_accepted()
    {
        fn reads_hp_twice(a: Query<&Hp>, b: Query<(&Hp, &Mana)>)
        {
            assert_eq!(a.into_iter().map(|hp| hp.0).sum::<u64>(), 24);
            assert_eq!(b.into_iter().map(|(hp, _)| hp.0).sum::<u64>(), 12);
        }

        let mut world = HeapPtr::new(World::default());
        world.create(Hp(12));
        world.create((Hp(12), Mana(12)));
        let mut scheduler = DefaultScheduler::new(world);

        scheduler.add_system(DefaultScheduleSession::Start, reads_hp_twice);
        scheduler.run(DefaultScheduleSession::Start);
    }

    /// The registry is keyed by system type, so this must not report a conflict with the copy
    /// registered for `Start`
    #[test]
    fn the_same_system_can_join_two_sessions()
    {
        let mut world = HeapPtr::new(World::default());
        world.create(Hp(12));
        let mut scheduler = DefaultScheduler::new(world);

        scheduler
            .add_system(DefaultScheduleSession::Start, system_b)
            .add_system(DefaultScheduleSession::Update, system_b);

        scheduler.run(DefaultScheduleSession::Start);
        scheduler.run(DefaultScheduleSession::Update);
    }
}

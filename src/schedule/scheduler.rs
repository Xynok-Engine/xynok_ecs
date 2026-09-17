use std::collections::HashMap;
use std::hash::Hash;

use xynok_concurrency::thread_pool::ThreadPool;
use xynok_concurrency::thread_pool::cfg::CfgThreadPool;
use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::constants::ChangedTick;
use crate::schedule::system_spec::{ScheduleStep, SystemSpecs};
use crate::system::traits::{SystemTypeStorage, TIntoSystem, TIntoSystems};
use crate::world::World;

/// THIS IS FOR DEMO PURPOSES ONLY.
///
/// This scheduler serves as an example of how to use xynok_ecs,
/// but you should implement your own. YOUR WORLD, YOUR RULE !  
pub trait TScheduler: Sized
{
    type SessionType: Hash + PartialEq + Eq;

    #[track_caller]
    fn new(world: HeapMut<World>) -> Self;

    #[track_caller]
    fn add_system<P, T: TIntoSystem<P>>(&mut self, session: Self::SessionType, system: T) -> &mut Self;

    #[track_caller]
    fn add_system_parallel<P, T: TIntoSystems<P>>(&mut self, session: Self::SessionType, systems: T) -> &mut Self;

    #[track_caller]
    fn run(&mut self, session: Self::SessionType);
}
/// THIS IS FOR DEMO PURPOSES ONLY.
///
/// This scheduler serves as an example of how to use xynok_ecs,
/// but you should implement your own. YOUR WORLD, YOUR RULE !  
pub type DefaultScheduler = Scheduler<DefaultScheduleSession>;

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
pub struct Scheduler<T: Hash + Eq>
{
    world:        HeapMut<World>,
    system_specs: SystemSpecs,
    steps:        HashMap<T, Vec<ScheduleStep>>,
}
impl<T: Hash + PartialEq + Eq> Scheduler<T>
{
    /// Records a system's spec before it ever gets a chance to run.
    ///
    /// That is what reports a system whose parameters alias each other at the `add_system` call
    /// site rather than at the first `run`.
    #[track_caller]
    fn register(&mut self, s: &SystemTypeStorage)
    {
        if let Err(e) = self.system_specs.register(s.as_ref(), self.world.component_specs_mut())
        {
            panic!("{}: {}", s.name(), e);
        }
    }
}
impl<TS: Hash + PartialEq + Eq> TScheduler for Scheduler<TS>
{
    type SessionType = TS;

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
        if let Err(e) = self.system_specs.check_group_can_parallel(&group)
        {
            panic!("{}", e);
        }

        self.steps.entry(session).or_default().push(ScheduleStep::Parallel(group));
        self
    }

    #[inline]
    fn run(&mut self, session: Self::SessionType)
    {
        let Some(steps) = self.steps.get_mut(&session)
        else
        {
            return;
        };
        let world = self.world;

        for step in steps.iter_mut()
        {
            match step
            {
                ScheduleStep::Single(tsystem) => run_system(tsystem, world),

                ScheduleStep::Parallel(tsystems) => run_system_group(tsystems, world),
            }
        }
    }

    fn new(world: HeapMut<World>) -> Self
    {
        // The pool lives in the world, so a `Query` can reach it too. An executor the user
        // already plugged in is kept as it is.
        let w = world.as_ref_mut();
        if w.executor().is_none()
        {
            w.set_executor(Some(Box::new(ThreadPool::new(CfgThreadPool::new("Default Xynok ECS Scheduler ThreadPool", 4)))));
        }

        Self {
            world:        world,
            steps:        HashMap::new(),
            system_specs: SystemSpecs::default(),
        }
    }
}
#[track_caller]
fn run_system(system: &mut SystemTypeStorage, world: HeapMut<World>)
{
    // one tick per step: every write this system makes is stamped with it, and the next step
    // sees a strictly later one
    let this_run = world.as_ref_mut().advance_tick();
    prepare_system(system, world);
    run_system_at(system, world, this_run);
}

/// Registers every query the system needs, on the calling thread. `run` only reads the world's
/// registries, so this has to happen before it, otherwise the system has nothing to read.
#[track_caller]
fn prepare_system(system: &SystemTypeStorage, world: HeapMut<World>)
{
    match system.prepare(world)
    {
        Ok(_) =>
        {}
        Err(e) => panic!("{}", e),
    }
}

/// Runs a system whose tick was already taken by the caller.
///
/// Queries read their tick back through `World::current_tick`, so `this_run` has to still be
/// the world's current tick while the system runs. Nobody may call `advance_tick` in between.
#[track_caller]
fn run_system_at(system: &mut SystemTypeStorage, world: HeapMut<World>, this_run: ChangedTick)
{
    match system.run(world)
    {
        Ok(_) =>
        {}
        Err(e) => panic!("{}", e),
    }

    system.set_last_run_tick(this_run);
}

#[track_caller]
fn run_system_group(group: &mut [SystemTypeStorage], world: HeapMut<World>)
{
    // The whole group is one step, so it shares one tick. Systems in a group never touch what
    // another one writes, so nobody inside it needs to tell their writes apart. Taking a tick per
    // system would also break change detection: a query reads `current_tick` when it starts, and
    // by then another job may have moved the tick past the one its system stores as `last_run_tick`.
    let this_run = world.as_ref_mut().advance_tick();

    // With no executor there is nowhere to fan out, so the group just runs one system at a time
    let pool = match world.as_ref_with_caller_lifetime().executor()
    {
        Some(pool) if group.len() > 1 => pool,
        _ =>
        {
            group.iter_mut().for_each(|system| {
                prepare_system(system, world);
                run_system_at(system, world, this_run);
            });
            return;
        }
    };

    // The preparation pass, on this very thread: initialising a query writes into the world's
    // registries, and two jobs writing there at once is a race. After this pass, `init` inside a
    // job is a table lookup, which is a read, and concurrent reads are fine.
    group.iter().for_each(|system| prepare_system(system, world));

    // Job `i` runs system `i`. Every index is handed out exactly once, so no two jobs ever
    // reach the same system even though they all go through this one shared pointer.
    let systems = SyncMutPtr(group.as_mut_ptr());
    pool.run_indexed(group.len(), &|i| {
        // SAFETY: `i < group.len()`, only this job gets `i`, and `group` outlives `run_indexed`
        let system = unsafe { systems.at(i) };
        run_system_at(system, world, this_run);
    });
}

/// Lets a raw pointer into a slice be shared by jobs that each touch a different element
struct SyncMutPtr<T>(*mut T);

// SAFETY: only used by `run_system_group`, where each job reaches a distinct element
unsafe impl<T: Send> Sync for SyncMutPtr<T> {}

impl<T> SyncMutPtr<T>
{
    /// Going through a method makes a closure capture the whole wrapper, not the bare pointer
    /// field, which is what keeps the `Sync` impl above in play
    ///
    /// # Safety
    /// `i` must be in bounds of the slice, and nobody else may hold element `i` at the same time
    #[inline]
    unsafe fn at<'a>(&self, i: usize) -> &'a mut T
    {
        unsafe { &mut *self.0.add(i) }
    }
}

#[allow(unused)]
#[cfg(test)]
mod test
{
    use std::collections::HashMap;

    use crate::apis::traits::TComponent;
    use crate::query::Query;
    use crate::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
    use crate::schedule::system_spec::ScheduleStep;
    use crate::world::World;
    use xynok_ecs_proc_macro::component;
    use xynok_std::unsafe_ptr::HeapPtr;

    #[component(EnableAble)]
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
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());

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
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());

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
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());

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
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());

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
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());

        scheduler
            .add_system(DefaultScheduleSession::Start, system_b)
            .add_system(DefaultScheduleSession::Update, system_b);

        scheduler.run(DefaultScheduleSession::Start);
        scheduler.run(DefaultScheduleSession::Update);
    }
    #[test]
    fn parallel_batch_finishes_before_the_next_step()
    {
        fn add_hp(query: Query<&mut Hp>)
        {
            for mut hp in query
            {
                hp.0 += 1;
            }
        }
        fn add_mana(query: Query<&mut Mana>)
        {
            for mut mana in query
            {
                mana.0 += 2;
            }
        }
        fn check(query: Query<(&Hp, &Mana)>)
        {
            let mut count = 0;
            for (hp, mana) in query
            {
                assert!(hp.0 >= 2);
                assert_eq!(mana.0, hp.0 * 2 + 8);
                count += 1;
            }
            assert_eq!(count, 257);
        }
        let mut world = HeapPtr::new(World::default());
        for _ in 0..257
        {
            world.create((Hp(1), Mana(10)));
        }
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
        scheduler
            .add_system_parallel(DefaultScheduleSession::Update, (add_hp, add_mana))
            .add_system(DefaultScheduleSession::Update, check);
        scheduler.run(DefaultScheduleSession::Update);
        scheduler.run(DefaultScheduleSession::Update);
        for (hp, mana) in scheduler.world.create_query::<(&Hp, &Mana)>()
        {
            assert_eq!((hp.0, mana.0), (3, 14));
        }
    }

    #[component(ChangeAble)]
    struct Armor(u64);
    #[component(ChangeAble)]
    struct Speed(u64);

    /// A parallel group is one step with one tick. Each system must not see its own writes from
    /// the last frame as `Changed`, and a system after the group must still see them.
    #[test]
    fn parallel_group_shares_one_tick()
    {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use crate::query::filter::Changed;

        static ARMOR_SEEN: AtomicUsize = AtomicUsize::new(0);
        static SPEED_SEEN: AtomicUsize = AtomicUsize::new(0);
        static AFTER_SEEN: AtomicUsize = AtomicUsize::new(0);

        fn bump_armor(query: Query<Changed<&mut Armor>>)
        {
            for mut armor in query
            {
                armor.0 += 1;
                ARMOR_SEEN.fetch_add(1, Ordering::Relaxed);
            }
        }
        fn bump_speed(query: Query<Changed<&mut Speed>>)
        {
            for mut speed in query
            {
                speed.0 += 1;
                SPEED_SEEN.fetch_add(1, Ordering::Relaxed);
            }
        }
        fn after(query: Query<Changed<&Armor>>)
        {
            AFTER_SEEN.fetch_add(query.into_iter().count(), Ordering::Relaxed);
        }

        let mut world = HeapPtr::new(World::default());
        for _ in 0..64
        {
            world.create((Armor(0), Speed(0)));
        }
        let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
        scheduler
            .add_system_parallel(DefaultScheduleSession::Update, (bump_armor, bump_speed))
            .add_system(DefaultScheduleSession::Update, after);

        scheduler.run(DefaultScheduleSession::Update);
        // checked straight on the ticks too, since the race above only shows up now and then
        let group_ticks: Vec<_> = match &scheduler.steps[&DefaultScheduleSession::Update][0]
        {
            ScheduleStep::Parallel(group) => group.iter().map(|s| s.last_run_tick()).collect(),
            ScheduleStep::Single(_) => unreachable!(),
        };
        let world_tick = scheduler.world.current_tick();
        assert!(
            group_ticks.iter().all(|&t| t == world_tick - 1),
            "group ticks {group_ticks:?}, world tick {world_tick}"
        );
        assert_eq!(ARMOR_SEEN.load(Ordering::Relaxed), 64);
        assert_eq!(SPEED_SEEN.load(Ordering::Relaxed), 64);
        assert_eq!(AFTER_SEEN.load(Ordering::Relaxed), 64);

        // nothing else wrote, so the group has nothing new to see, and neither does `after`
        for _ in 0..8
        {
            scheduler.run(DefaultScheduleSession::Update);
        }
        assert_eq!(ARMOR_SEEN.load(Ordering::Relaxed), 64, "bump_armor saw its own writes again");
        assert_eq!(SPEED_SEEN.load(Ordering::Relaxed), 64, "bump_speed saw its own writes again");
        assert_eq!(AFTER_SEEN.load(Ordering::Relaxed), 64);
    }
}

use std::collections::HashMap;
use std::hash::Hash;

use xynok_std::unsafe_ptr::HeapPtr;

use crate::schedule::system_spec::SystemSpecs;
use crate::system::traits::{SystemTypeStorage, TIntoSystem};
use crate::world::World;

pub trait TScheduler: Sized
{
    type SessionType: Eq + Hash + Clone;

    #[track_caller]
    fn new(world: HeapPtr<World>) -> Self;
    #[track_caller]
    fn add_system<P, T: TIntoSystem<P>>(&mut self, session: Self::SessionType, system: T) -> &mut Self;

    #[track_caller]
    fn run(&mut self, session: Self::SessionType);
}
pub struct DefaultScheduler
{
    world:   HeapPtr<World>,
    systems: HashMap<DefaultScheduleSession, Vec<SystemTypeStorage>>,
    /// Keyed by system type, so the same `fn` added to two sessions is described once. Only
    /// safe because everything in a spec is derived from the system's type - anything
    /// per-instance would have to live beside the boxed system in `systems` instead.
    specs:   SystemSpecs,
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

        // Before the system is ever run, so a system whose parameters alias each other is
        // reported at the `add_system` call site rather than at the first `run`
        if let Err(e) = self.specs.register(s.as_ref(), &mut self.world.component_counter)
        {
            panic!("{}: {}", s.name(), e);
        }
        if let Some(systems) = self.systems.get_mut(&session)
        {
            systems.push(s);
        }
        else
        {
            let systems = vec![s];
            self.systems.insert(session, systems);
        }
        self
    }

    fn run(&mut self, session: Self::SessionType)
    {
        if let Some(systems) = self.systems.get_mut(&session)
        {
            for s in systems
            {
                match s.run(self.world.as_ref_mut())
                {
                    Ok(_) =>
                    {}
                    Err(e) => panic!("{}", e),
                }
            }
        }
    }

    fn new(world: HeapPtr<World>) -> Self
    {
        Self {
            world,
            systems: HashMap::new(),
            specs: SystemSpecs::default(),
        }
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

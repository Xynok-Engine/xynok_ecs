use std::collections::HashMap;
use std::hash::Hash;

use xynok_std::unsafe_ptr::HeapMut;

use crate::system::traits::{SystemTypeStorage, TIntoSystem};
use crate::world::World;

pub trait TScheduler: Sized
{
    type SessionType: Hash + PartialEq + Eq + std::fmt::Debug + Clone + Copy + 'static;
    #[track_caller]
    fn add_system<P, T: TIntoSystem<P>>(&mut self, session: Self::SessionType, system: T) -> &mut Self;

    #[track_caller]
    fn run(&mut self, session: Self::SessionType, world: HeapMut<World>);
}
#[derive(Default)]
pub struct DefaultScheduler
{
    systems: HashMap<DefaultScheduleSession, Vec<SystemTypeStorage>>,
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

    fn run(&mut self, session: Self::SessionType, world: HeapMut<World>)
    {
        if let Some(systems) = self.systems.get_mut(&session)
        {
            for s in systems
            {
                match s.run(world)
                {
                    Ok(_) =>
                    {}
                    Err(e) => panic!("{}", e),
                }
            }
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
    fn system_c(query: Query<(&Hp, &Mana)>)
    {
        println!("system c running !");
        for (hp, mana) in query
        {
            println!("hp({}) - mana(){}", hp.0, mana.0);
        }
    }

    #[test]
    fn test_scheduler()
    {
        let mut scheduler = DefaultScheduler::default();
        let mut world = HeapPtr::new(World::default());
        world.create(Hp(12));
        world.create((Hp(12), Mana(12)));

        scheduler
            .add_system(DefaultScheduleSession::Start, system_a)
            .add_system(DefaultScheduleSession::Start, system_b)
            .add_system(DefaultScheduleSession::Start, system_c);

        scheduler.run(DefaultScheduleSession::Start, world.as_ref_mut());
    }
}

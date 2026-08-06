use crate::system::traits::{SystemTypeStorage, TIntoSystem};

pub trait TScheduler: Sized
{
    #[track_caller]
    fn add_system<P, T: TIntoSystem<P>>(&mut self, s: T) -> &mut Self;
}
pub struct Scheduler
{
    systems: Vec<SystemTypeStorage>,
}

impl TScheduler for Scheduler
{
    fn add_system<P, T: TIntoSystem<P>>(&mut self, s: T) -> &mut Self
    {
        let s = match s.into_system()
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        self.systems.push(s);
        self
    }
}
#[allow(unused)]
#[cfg(test)]
mod test
{
    use crate::apis::traits::TComponent;
    use crate::query::Query;
    use crate::schedule::scheduler::{Scheduler, TScheduler};
    use xynok_ecs_proc_macro::component;

    #[component]
    struct Hp(u64);
    #[component]
    struct Mana(u64);
    fn system_a() {}
    fn system_b(query: Query<&Hp>) {}
    fn system_c(query: Query<(&Hp, &Mana)>) {}

    fn test_scheduler()
    {
        let mut scheduler = Scheduler { systems: Vec::new() };
        scheduler.add_system(system_a).add_system(system_b).add_system(system_c);
    }
}

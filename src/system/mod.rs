use std::marker::PhantomData;

use crate::apis::identifies::{SystemError, XynokEcsError};
use crate::apis::internal_traits::{TIntoSystem, TSystem, TSystemOutput, TSystemParam, TSystemParamFunction};
use crate::query::access_scope::AccessScope;
use crate::world::World;

pub(crate) mod function;
pub(crate) mod param;
mod into_systems;

/// Everything about a system that a scheduler needs before running it.
pub struct SystemMeta
{
    pub name:         &'static str,
    pub access_scope: AccessScope,
}

/// A plain `fn`/closure wrapped into something callable through `dyn TSystem`.
///
/// `Marker` never exists at runtime; it only picks which `TSystemParamFunction` impl applies to
/// `F`. It is held as `PhantomData<fn() -> Marker>` rather than `PhantomData<Marker>` so that a
/// marker type (which is a bare `fn` pointer type, never constructed) cannot drag `!Send`/`!Sync`
/// into the auto-trait derivation — `fn() -> T` is unconditionally `Send + Sync`, so the `Send` and
/// `Sync` this struct gets are the honest ones coming from `F` and its state.
pub struct FunctionSystem<Marker, F>
where F: TSystemParamFunction<Marker>
{
    func:    F,
    state:   Option<<F::Param as TSystemParam>::State>,
    meta:    SystemMeta,
    _marker: PhantomData<fn() -> Marker>,
}

impl<Marker, F> FunctionSystem<Marker, F>
where F: TSystemParamFunction<Marker>
{
    fn new(func: F) -> Result<Self, XynokEcsError>
    {
        Ok(Self {
            func:    func,
            state:   None,
            // computed once here so a scheduler can read it without touching the world, and so a
            // conflicting param list fails at conversion instead of on the first frame
            meta:    SystemMeta {
                name:         std::any::type_name::<F>(),
                access_scope: <F::Param as TSystemParam>::access_scope()?,
            },
            _marker: PhantomData,
        })
    }
}

impl<Marker, F> TSystem for FunctionSystem<Marker, F>
where
    Marker: 'static,
    F: TSystemParamFunction<Marker>,
    <F::Param as TSystemParam>::State: Send + Sync,
{
    fn name(&self) -> &'static str
    {
        self.meta.name
    }

    fn init(&mut self, world: &mut World) -> Result<(), XynokEcsError>
    {
        if self.state.is_none()
        {
            self.state = Some(<F::Param as TSystemParam>::init_state(world)?);
        }
        Ok(())
    }

    fn run(&mut self, world: &mut World) -> Result<(), SystemError>
    {
        self.init(world)?;
        let state = match self.state.as_mut()
        {
            Some(r) => r,
            // `init` above either filled it or bailed out, so this is unreachable in practice; it
            // is an error rather than an `unwrap` because a system failing must never abort a frame
            None => return Err(XynokEcsError::SystemStateIsNotInitialized(self.meta.name).into()),
        };

        // SAFETY: `FunctionSystem::new` validated `F::Param`'s access scope, so the params about to
        // be fetched are pairwise disjoint. `world` is borrowed mutably here, so nothing else holds
        // a view of it for the duration of the run.
        let params = unsafe { <F::Param as TSystemParam>::fetch(state, world.as_unsafe_cell())? };
        self.func.run(params).into_system_result()
    }

    fn access_scope(&self) -> &AccessScope
    {
        &self.meta.access_scope
    }
}

impl<Marker, F> TIntoSystem<Marker> for F
where
    Marker: 'static,
    F: TSystemParamFunction<Marker>,
    <F::Param as TSystemParam>::State: Send + Sync,
{
    type System = FunctionSystem<Marker, F>;

    #[track_caller]
    fn into_system(self) -> Result<Self::System, XynokEcsError>
    {
        FunctionSystem::new(self)
    }
}

#[cfg(test)]
mod tests
{
    use super::*;
    use crate::apis::identifies::StorageLocation;
    use crate::apis::internal_traits::{SystemTypeStorage, TIntoSystems};
    use crate::apis::traits::TComponent;
    use crate::query::Query;

    // hand-rolled instead of `#[component]`: the macro expands to absolute `xynok_ecs::` paths,
    // which do not resolve from inside the crate that defines them
    #[derive(Debug, PartialEq)]
    struct Hp(u32);
    impl TComponent for Hp
    {
        type QueryType = Hp;
        type StorageType = Hp;
        const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;
    }

    #[derive(Debug, PartialEq)]
    struct Mana(u32);
    impl TComponent for Mana
    {
        type QueryType = Mana;
        type StorageType = Mana;
        const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;
    }

    #[test]
    fn t_a_bare_fn_becomes_a_system_and_runs()
    {
        static RAN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn tick()
        {
            RAN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        let mut world = World::default();
        let mut sys = tick.into_system().unwrap();
        sys.run(&mut world).unwrap();
        sys.run(&mut world).unwrap();

        assert_eq!(RAN.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn t_a_fn_taking_a_query_receives_a_usable_query()
    {
        static SUM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        fn sum_hp(q: Query<&Hp>)
        {
            let total: u32 = q.into_iter().map(|hp| hp.0).sum();
            SUM.store(total, std::sync::atomic::Ordering::Relaxed);
        }

        let mut world = World::default();
        world.create(Hp(10));
        world.create(Hp(32));

        sum_hp.into_system().unwrap().run(&mut world).unwrap();
        assert_eq!(SUM.load(std::sync::atomic::Ordering::Relaxed), 42);
    }

    #[test]
    fn t_a_query_param_sees_archetypes_created_after_the_first_run()
    {
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn count_hp(q: Query<&Hp>)
        {
            COUNT.store(q.into_iter().count(), std::sync::atomic::Ordering::Relaxed);
        }

        let mut world = World::default();
        world.create(Hp(1));

        let mut sys = count_hp.into_system().unwrap();
        sys.run(&mut world).unwrap();
        assert_eq!(COUNT.load(std::sync::atomic::Ordering::Relaxed), 1);

        // a brand new archetype that also carries `Hp`: the cached archetype list from the first
        // run no longer describes the world, and `fetch` has to notice
        world.create((Hp(2), Mana(9)));
        sys.run(&mut world).unwrap();
        assert_eq!(COUNT.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn t_multiple_params_are_fetched_together()
    {
        static SEEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        fn both(hp: Query<&mut Hp>, mana: Query<&Mana>)
        {
            for h in hp
            {
                h.0 += 1;
            }
            let total: u32 = mana.into_iter().map(|m| m.0).sum();
            SEEN.store(total, std::sync::atomic::Ordering::Relaxed);
        }

        let mut world = World::default();
        world.create((Hp(1), Mana(5)));
        world.create((Hp(2), Mana(7)));

        both.into_system().unwrap().run(&mut world).unwrap();

        assert_eq!(SEEN.load(std::sync::atomic::Ordering::Relaxed), 12);
        let hp: Vec<u32> = world.create_query::<&Hp>().into_iter().map(|h| h.0).collect();
        assert_eq!(hp, vec![2, 3]);
    }

    #[test]
    fn t_a_system_may_return_an_ecs_error()
    {
        fn failing() -> Result<(), XynokEcsError>
        {
            Err(XynokEcsError::ChunkIsFull(8))
        }

        let mut world = World::default();
        let err = failing.into_system().unwrap().run(&mut world).unwrap_err();
        assert_eq!(err.to_string(), "Chunk is full of capacity(8)");
    }

    #[test]
    fn t_a_system_may_return_a_foreign_error()
    {
        // the point of the type-erased `XynokError`: this error type is unknown to the ECS, yet `?`
        // works on it inside a system body
        fn parsing() -> Result<(), std::num::ParseIntError>
        {
            "not a number".parse::<u32>()?;
            Ok(())
        }

        let mut world = World::default();
        let err = parsing.into_system().unwrap().run(&mut world).unwrap_err();
        assert!(err.source().is::<std::num::ParseIntError>());
    }

    #[test]
    fn t_conflicting_params_are_rejected_at_conversion()
    {
        fn conflict(_a: Query<&mut Hp>, _b: Query<&Hp>) {}

        let err = conflict.into_system().map(|_| ()).unwrap_err();
        assert!(matches!(err, XynokEcsError::QueryAccessScopeConflict));
    }

    #[test]
    fn t_read_only_params_on_the_same_component_are_allowed()
    {
        fn shared_read(_a: Query<&Hp>, _b: Query<&Hp>) {}

        let sys = shared_read.into_system().unwrap();
        assert!(sys.access_scope().is_read_only());
    }

    #[test]
    fn t_a_tuple_of_fns_converts_into_a_boxed_system_list()
    {
        fn a() {}
        fn b(_q: Query<&Hp>) {}
        fn c(_q: Query<&mut Mana>) {}

        let systems: Vec<SystemTypeStorage> = (a, b, c).into_systems().unwrap();
        assert_eq!(systems.len(), 3);

        let mut world = World::default();
        world.create((Hp(1), Mana(2)));
        for mut sys in systems
        {
            sys.run(&mut world).unwrap();
        }
    }

    #[test]
    fn t_a_single_fn_also_converts_through_into_systems()
    {
        fn lonely(_q: Query<&Hp>) {}

        let systems = lonely.into_systems().unwrap();
        assert_eq!(systems.len(), 1);
    }

    #[test]
    fn t_the_system_name_survives_into_the_boxed_form()
    {
        fn named_system() {}

        let boxed: SystemTypeStorage = Box::new(named_system.into_system().unwrap());
        assert!(boxed.name().contains("named_system"));
    }

    #[test]
    fn t_the_access_scope_is_computed_once_and_borrowed()
    {
        fn writes_hp(_q: Query<&mut Hp>) {}

        let sys = writes_hp.into_system().unwrap();
        let a = sys.access_scope() as *const AccessScope;
        let b = sys.access_scope() as *const AccessScope;
        assert_eq!(a, b);
        assert!(!sys.access_scope().is_read_only());
    }
}

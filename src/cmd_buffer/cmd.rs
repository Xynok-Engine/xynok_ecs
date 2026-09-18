use std::collections::VecDeque;

use xynok_std::unsafe_ptr::HeapMut;
use xynok_type_eraser::inline_fn::{InlineFnOnce, LARGE};

use crate::apis::identifies::XynokEcsError;
use crate::apis::traits::TArchetype;
use crate::entity::Entity;
use crate::system::traits::TSystemParam;
use crate::world::worker_spec::WorkerSpec;
use crate::world::World;

const PRE_ALLOCATED_ENTITIES_AMOUNT: usize = 32;
const CMD_BUFFER_CAPACITY: usize = 32;

/// Use this type in your system parameters to access world APIs, both in main and parallel contexts.
/// A thread-safe wrapper for the World.
pub struct Cmd
{
    world: HeapMut<World>,
}
unsafe impl Send for Cmd {}

type CmdBuffer = InlineFnOnce<LARGE, true>;

impl TSystemParam for Cmd
{
    fn init(world: HeapMut<World>, _last_run_tick: crate::apis::constants::ChangedTick) -> Result<Self, crate::apis::identifies::XynokEcsError>
    {
        Ok(Self { world: world })
    }

    fn prepare(_world: HeapMut<World>, _last_run_tick: crate::apis::constants::ChangedTick) -> Result<(), crate::apis::identifies::XynokEcsError>
    {
        Ok(())
    }

    fn collect_access_scope(
        _dst: &mut crate::query::access_scope::AccessScopes,
        _component_specs: &mut crate::apis::params::ComponentSpecs,
    ) -> Result<(), crate::apis::identifies::XynokEcsError>
    {
        Ok(())
    }
}
//
//thread_local! {
//static WORKER_IDX: UnsafeCell<Option<usize>> = const{UnsafeCell::new(None)};
//
//}
impl Cmd
{
    #[track_caller]
    pub fn create<T: TArchetype + 'static>(&mut self, val: T) -> Entity
    {
        match self.try_create(val)
        {
            Ok(e) => e,
            Err(e) => panic!("{}", e),
        }
    }

    pub fn try_create<T: TArchetype + 'static>(&mut self, val: T) -> Result<Entity, XynokEcsError>
    {
        if self.in_main_thread_now()
        {
            return self.world.try_create(val);
        }
        self.warm_up();
        let mut world = self.world;
        let worker_spec = match world.get_current_worker_spec()
        {
            Some(r) =>
            unsafe { &mut *r.get() },
            None => return Err(XynokEcsError::WorkerSpecIsNotCreated),
        };
        let e = loop
        {
            match worker_spec.pre_allocated_entities.pop_front()
            {
                Some(r) => break r,
                None => match world.try_pre_allocate_entities(PRE_ALLOCATED_ENTITIES_AMOUNT, &mut worker_spec.pre_allocated_entities)
                {
                    Ok(_) =>
                    {}
                    Err(e) => return Err(e),
                },
            }
        };
        let cmd = CmdBuffer::new(move || {
            if let Err(err) = world.create_components_for(e, val)
            {
                world.erase_entity(e);
                panic!("WORKER[{:?}]: {}", Self::id(), err)
            }
        });
        worker_spec.cmd_buffer.push_back(cmd);
        Ok(e)
    }
    #[track_caller]
    pub fn add_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        match self.try_add_component(e, val)
        {
            Ok(_) =>
            {}
            Err(e) => panic!("{}", e),
        }
    }
    pub fn try_add_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T) -> Result<(), XynokEcsError>
    {
        if self.in_main_thread_now()
        {
            return self.world.try_add_component(e, val);
        }
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world.try_add_component(e, val)
            {
                panic!("WORKER[{:?}]: {}", Self::id(), e)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(())
    }
    #[track_caller]
    pub fn remove_component<T: TArchetype + 'static>(&mut self, e: Entity) -> Option<T>
    {
        match self.try_remove_component::<T>(e)
        {
            Ok(r) => r,

            Err(e) => panic!("{}", e),
        }
    }
    pub fn try_remove_component<T: TArchetype + 'static>(&mut self, e: Entity) -> Result<Option<T>, XynokEcsError>
    {
        if self.in_main_thread_now()
        {
            let r = self.world.try_remove_component::<T>(e)?;
            return Ok(Some(r));
        }
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world.try_remove_component::<T>(e)
            {
                panic!("WORKER[{:?}]: {}", Self::id(), e)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(None)
    }
    #[track_caller]
    pub fn merge_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        match self.try_merge_component(e, val)
        {
            Ok(_) =>
            {}
            Err(e) => panic!("{}", e),
        }
    }
    pub fn try_merge_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T) -> Result<(), XynokEcsError>
    {
        if self.in_main_thread_now()
        {
            return self.world.try_merge_component(e, val);
        }
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world.try_merge_component(e, val)
            {
                panic!("WORKER[{:?}]: {}", Self::id(), e)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(())
    }
    #[track_caller]
    pub fn destroy(&mut self, e: Entity)
    {
        match self.try_destroy(e)
        {
            Ok(_) =>
            {}
            Err(e) => panic!("{}", e),
        }
    }
    pub fn try_destroy(&mut self, e: Entity) -> Result<(), XynokEcsError>
    {
        if self.in_main_thread_now()
        {
            return self.world.try_destroy(e);
        }
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(err) = world.try_destroy(e)
            {
                world.erase_entity(e);
                panic!("WORKER[{:?}]: {}", Self::id(), err)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(())
    }
}

impl Cmd
{
    #[inline]
    fn id() -> std::thread::ThreadId
    {
        std::thread::current().id()
    }
    #[inline]
    fn in_main_thread_now(&self) -> bool
    {
        unsafe { !self.world.in_parallel_compute() }
    }
    fn cmd_buffer(&mut self) -> Result<&mut VecDeque<CmdBuffer>, XynokEcsError>
    {
        // This thread may never have called `try_create`, in which case it has no worker spec
        // yet. Warming up here gives every deferred command the same way in, instead of asking
        // the caller to create something first before they are allowed to destroy an entity or
        // change its components.
        self.warm_up();
        match self.world.get_current_worker_spec()
        {
            Some(r) =>
            {
                let x = unsafe { &mut *r.get() };
                Ok(&mut x.cmd_buffer)
            }
            None => Err(XynokEcsError::WorkerSpecIsNotCreated),
        }
    }
    fn warm_up(&mut self)
    {
        let mut world = self.world;
        if !world.is_registered()
        {
            world.push_worker_spec(WorkerSpec::new(PRE_ALLOCATED_ENTITIES_AMOUNT, CMD_BUFFER_CAPACITY));
        }
    }
}

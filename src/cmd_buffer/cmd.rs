use core::cell::UnsafeCell;
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

thread_local! {
static WORKER_IDX: UnsafeCell<Option<usize>> = const{UnsafeCell::new(None)};

}
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
        self.warm_up();
        let mut world = self.world;
        let mut world2 = self.world;
        let worker_idx = Self::worker_idx().unwrap();
        let worker_spec = match world.get_worker_spec_mut(worker_idx)
        {
            Some(r) => r,
            None => return Err(XynokEcsError::WorkerSpecIsNotCreated),
        };
        let e = {
            match worker_spec.pre_allocated_entities.pop_front()
            {
                Some(r) => r,
                None => match world2.try_pre_allocate_entities(PRE_ALLOCATED_ENTITIES_AMOUNT, &mut worker_spec.pre_allocated_entities)
                {
                    Ok(_) => return self.try_create(val),
                    Err(e) => return Err(e),
                },
            }
        };
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world2.create_components_for(e, val)
            {
                panic!("WORKER[{}]: {}", worker_idx, e)
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
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world.try_add_component(e, val)
            {
                panic!("WORKER[{}]: {}", Self::worker_idx().unwrap(), e)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(())
    }
    #[track_caller]
    pub fn remove_component<T: TArchetype + 'static>(&mut self, e: Entity)
    {
        match self.try_remove_component::<T>(e)
        {
            Ok(_) =>
            {}
            Err(e) => panic!("{}", e),
        }
    }
    pub fn try_remove_component<T: TArchetype + 'static>(&mut self, e: Entity) -> Result<(), XynokEcsError>
    {
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world.try_remove_component::<T>(e)
            {
                panic!("WORKER[{}]: {}", Self::worker_idx().unwrap(), e)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(())
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
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(e) = world.try_merge_component(e, val)
            {
                panic!("WORKER[{}]: {}", Self::worker_idx().unwrap(), e)
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
        let mut world = self.world;
        let cmd_buffer = self.cmd_buffer()?;
        let cmd = CmdBuffer::new(move || {
            if let Err(err) = world.try_destroy(e)
            {
                world.erase_entity(e);
                panic!("WORKER[{}]: {}", Self::worker_idx().unwrap(), err)
            }
        });
        cmd_buffer.push_back(cmd);
        Ok(())
    }
}

impl Cmd
{
    fn cmd_buffer(&mut self) -> Result<&mut VecDeque<CmdBuffer>, XynokEcsError>
    {
        // Thread này có thể chưa từng gọi `try_create`, tức là chưa có worker spec nào. Warm up
        // ngay tại đây để mọi lệnh hoãn đều dùng chung một đường vào, thay vì bắt caller phải
        // create một cái gì đó trước rồi mới được destroy hay đổi component.
        self.warm_up();
        let worker_idx = Self::worker_idx().unwrap();
        match self.world.get_worker_spec_mut(worker_idx)
        {
            Some(r) => Ok(&mut r.cmd_buffer),
            None => Err(XynokEcsError::WorkerSpecIsNotCreated),
        }
    }
    fn warm_up(&mut self)
    {
        let mut world = self.world;
        let worker_idx = Self::worker_idx();
        if worker_idx.is_none()
        {
            let idx = world.push_worker_spec(WorkerSpec::new(32, 32));
            *worker_idx = Some(idx);
        }
    }
    #[inline]
    fn worker_idx() -> &'static mut Option<usize>
    {
        WORKER_IDX.with(|q| unsafe { &mut *q.get() })
    }
}

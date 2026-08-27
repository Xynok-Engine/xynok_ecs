//! Structural change recorded now, applied later.
//!
//! `World::create`, `destroy`, `add_component` and `remove_component` all take `&mut World`. A
//! parallel job has no `&mut World` to hand them, and even if it did, moving an entity between
//! archetypes pulls rows out from under the jobs currently reading them. So while the pool is
//! running, structural change is *recorded*; at the synchronisation point that ends a step, the
//! scheduler applies it on a single thread.
//!
//! This is a lasting convention, not a temporary limitation. It is what systems pay for being
//! allowed to run in parallel without taking a lock.
//!
//! # Order of application
//!
//! Each worker writes into its own [`CommandBuffer`] (see [`PerWorker`]), so the hot path has no
//! contention. Application then walks the buffers **by slot index**, not by whichever worker
//! finished first: work stealing makes the finish order differ between runs, and a replay or a
//! lockstep session cannot survive that.
//!
//! Within one buffer, commands run in the order they were written.
//!
//! # Not here yet: an entity id you can use right away
//!
//! [`CommandBuffer::create`] returns no [`Entity`], because the entity does not exist yet when the
//! command is recorded. Handing one back means reserving a slot in the entity table up front, and
//! that is its own piece of work, worth doing when something real needs it.

use xynok_concurrency::per_worker::PerWorker;

use crate::apis::traits::TArchetype;
use crate::entity::Entity;
use crate::world::World;

/// One recorded command. `FnOnce` because every command carries its component values with it, and
/// those values have to *move* into the world rather than be copied in.
type Command = Box<dyn FnOnce(&mut World) + Send>;

/// One worker's list of pending commands.
///
/// No lock and no atomic in here: exactly one thread writes to a buffer, and that is the whole
/// reason it exists instead of one shared queue.
#[derive(Default)]
pub struct CommandBuffer
{
    commands: Vec<Command>,
}

impl CommandBuffer
{
    pub fn new() -> Self
    {
        Self::default()
    }

    /// Creates an entity carrying `T`'s components, when the command is applied.
    pub fn create<T: TArchetype + Send + 'static>(&mut self, val: T)
    {
        self.commands.push(Box::new(move |world| {
            world.create(val);
        }));
    }

    /// Destroys `e`, if it is still alive when the command is applied.
    ///
    /// Skips an already dead entity instead of panicking: two jobs may perfectly well decide to
    /// destroy the same entity within one step, and neither of them is wrong.
    pub fn destroy(&mut self, e: Entity)
    {
        self.commands.push(Box::new(move |world| {
            if world.exists(e)
            {
                world.destroy(e);
            }
        }));
    }

    /// Adds `T`'s components to `e`. See [`World::add_component`] for the no-duplicate-component
    /// rule it inherits.
    pub fn add_component<T: TArchetype + Send + 'static>(&mut self, e: Entity, val: T)
    {
        self.commands.push(Box::new(move |world| {
            if world.exists(e)
            {
                world.add_component(e, val);
            }
        }));
    }

    /// Adds or overwrites `T`'s components on `e`. See [`World::merge_component`].
    pub fn merge_component<T: TArchetype + Send + 'static>(&mut self, e: Entity, val: T)
    {
        self.commands.push(Box::new(move |world| {
            if world.exists(e)
            {
                world.merge_component(e, val);
            }
        }));
    }

    /// Removes `T`'s components from `e` and drops the removed value.
    ///
    /// The value is dropped on the thread that applies the command, so the component's `Drop` runs
    /// there rather than on the worker that recorded the command.
    pub fn remove_component<T: TArchetype + Send + 'static>(&mut self, e: Entity)
    {
        self.commands.push(Box::new(move |world| {
            if world.exists(e)
            {
                drop(world.remove_component::<T>(e));
            }
        }));
    }

    /// Queues an arbitrary change, run against `&mut World` at the synchronisation point.
    ///
    /// The escape hatch for changes the calls above do not name. Same ordering guarantees.
    pub fn push<F: FnOnce(&mut World) + Send + 'static>(&mut self, f: F)
    {
        self.commands.push(Box::new(f));
    }

    pub fn len(&self) -> usize
    {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool
    {
        self.commands.is_empty()
    }

    /// Drops every command that has not been applied yet.
    pub fn clear(&mut self)
    {
        self.commands.clear();
    }

    /// Moves the command list out, so it can be run while nothing borrows the buffer any more.
    fn take(&mut self) -> Vec<Command>
    {
        std::mem::take(&mut self.commands)
    }
}

impl std::fmt::Debug for CommandBuffer
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        f.debug_struct("CommandBuffer").field("commands", &self.commands.len()).finish()
    }
}

/// One [`CommandBuffer`] per participant in lane A's pool.
///
/// It lives inside [`World`] because the world is the only thing a system parameter can reach, and
/// [`Commands`] is exactly such a parameter.
pub struct CommandBuffers
{
    buffers: PerWorker<CommandBuffer>,
}

impl Default for CommandBuffers
{
    /// A single slot, for a world not attached to any pool. Every command lands in slot 0.
    fn default() -> Self
    {
        Self {
            buffers: PerWorker::with_len(1, |_| CommandBuffer::new()),
        }
    }
}

impl CommandBuffers
{
    /// Resizes the slot array to match a pool.
    ///
    /// # Panics
    ///
    /// If any command is still pending. Resizing rebuilds the slots, and silently swallowing
    /// queued commands is the kind of bug that leaves no trace at all.
    pub(crate) fn resize(&mut self, slots: usize)
    {
        assert!(
            self.buffers.iter_mut().all(|buffer| buffer.is_empty()),
            "attaching command buffers to a pool while commands are still pending"
        );
        self.buffers = PerWorker::with_len(slots.max(1), |_| CommandBuffer::new());
    }

    #[inline]
    pub fn slots(&self) -> usize
    {
        self.buffers.len()
    }

    /// Borrows slot `index` for writing.
    #[inline]
    pub fn with<R>(&self, index: usize, f: impl FnOnce(&mut CommandBuffer) -> R) -> R
    {
        self.buffers.with(index, f)
    }

    /// How many commands are pending across every slot.
    pub fn pending(&mut self) -> usize
    {
        self.buffers.iter_mut().map(|buffer| buffer.len()).sum()
    }

    /// Moves slot `index`'s command list out of the buffer.
    fn take_at(&self, index: usize) -> Vec<Command>
    {
        self.buffers.with(index, CommandBuffer::take)
    }
}

impl std::fmt::Debug for CommandBuffers
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        f.debug_struct("CommandBuffers").field("slots", &self.buffers.len()).finish()
    }
}

impl World
{
    /// Applies every pending command, slot by slot and, within a slot, in the order written.
    ///
    /// The scheduler calls this at the end of each step. Calling it by hand is fine too, as long
    /// as the pool is idle: a slot still borrowed by a job makes [`PerWorker::with`] panic, and
    /// that check is the point rather than an accident.
    pub fn apply_commands(&mut self)
    {
        for index in 0..self.command_buffers().slots()
        {
            // Move the list out before running anything: a command receives `&mut World`, which
            // reaches this very buffer set. Holding a borrow on the buffer while commands run
            // would mean two write paths into the same `Vec` the moment one of them queues more
            // work.
            let commands = self.command_buffers().take_at(index);
            for cmd in commands
            {
                cmd(self);
            }
        }
    }
}

/// A system's door for queueing structural change.
///
/// It touches no component storage, so it contributes nothing to the system's access scope: two
/// systems both holding `Commands` still run in parallel, each writing into its own worker slot.
///
/// ```no_run
/// use xynok_ecs::cmd_buffer::Commands;
/// use xynok_ecs::query::Query;
/// # use xynok_ecs::apis::traits::TComponent;
/// # use xynok_ecs_proc_macro::component;
/// # #[component]
/// # struct Hp(u64);
/// fn spawn_a_replacement(query: Query<&Hp>, cmd: Commands)
/// {
///     for hp in query
///     {
///         if hp.0 == 0
///         {
///             cmd.create(Hp(100));
///         }
///     }
/// }
/// ```
pub struct Commands
{
    pub(crate) world: xynok_std::unsafe_ptr::HeapMut<World>,
}

impl Commands
{
    /// Writes into the calling thread's slot.
    #[inline]
    fn with<R>(&self, f: impl FnOnce(&mut CommandBuffer) -> R) -> R
    {
        // `&World`, not `&mut`: the slot is borrowed through `PerWorker`, so building a `&mut
        // World` here would only create an alias nobody needs.
        let world: &World = self.world.as_ref_with_caller_lifetime();
        world.command_buffers().with(world.worker_index(), f)
    }

    pub fn create<T: TArchetype + Send + 'static>(&self, val: T)
    {
        self.with(|buffer| buffer.create(val));
    }

    pub fn destroy(&self, e: Entity)
    {
        self.with(|buffer| buffer.destroy(e));
    }

    pub fn add_component<T: TArchetype + Send + 'static>(&self, e: Entity, val: T)
    {
        self.with(|buffer| buffer.add_component(e, val));
    }

    pub fn merge_component<T: TArchetype + Send + 'static>(&self, e: Entity, val: T)
    {
        self.with(|buffer| buffer.merge_component(e, val));
    }

    pub fn remove_component<T: TArchetype + Send + 'static>(&self, e: Entity)
    {
        self.with(|buffer| buffer.remove_component::<T>(e));
    }

    pub fn push<F: FnOnce(&mut World) + Send + 'static>(&self, f: F)
    {
        self.with(|buffer| buffer.push(f));
    }
}

impl std::fmt::Debug for Commands
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        f.debug_struct("Commands").finish_non_exhaustive()
    }
}

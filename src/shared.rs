//! Shared components: values that every entity of an archetype uses together.
//!
//! A shared component is a plain value. Its key, taken from [`TSharedComponent::shared_key`],
//! decides which archetype an entity lives in. `docs/shared_components.md` walks through the
//! whole feature.

use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::archetype_chunk::{ArchetypeChunk, SharedValue};
use crate::entity::Entity;
use crate::query::access_scope::AccessScope;
use crate::world::World;

/// Declares a shared component. One value is stored per archetype, and `shared_key` picks the
/// archetype.
///
/// The key is read once, when the archetype is created, and stored next to the value.
/// [`World::set_shared_component`] and the `&mut` query view check that a value still gives
/// that key. [`World::override_shared_component`] does not. Keeping `shared_key` in step with
/// the value is up to you. `shared_key` runs on every one of those checks, so keep it cheap.
pub trait TSharedComponent: Clone + Send + Sync + 'static
{
    type Key: Eq + Hash + Clone + Send + Sync + 'static;
    fn shared_key(&self) -> Self::Key;
}

/// Write access to the shared value of one archetype.
///
/// The value can be edited freely while the view is alive. When the view is dropped it checks
/// that the value still gives the key the archetype was created with, and panics otherwise, the
/// same rule as [`World::set_shared_component`].
pub struct SharedMut<'a, C: TSharedComponent>
{
    key:   &'a C::Key,
    value: &'a mut C,
}

impl<C: TSharedComponent> Deref for SharedMut<'_, C>
{
    type Target = C;

    fn deref(&self) -> &C
    {
        self.value
    }
}

impl<C: TSharedComponent> DerefMut for SharedMut<'_, C>
{
    fn deref_mut(&mut self) -> &mut C
    {
        self.value
    }
}

impl<C: TSharedComponent> Drop for SharedMut<'_, C>
{
    fn drop(&mut self)
    {
        // a second panic while unwinding would abort the process and hide the first one
        if std::thread::panicking()
        {
            return;
        }
        if self.value.shared_key() != *self.key
        {
            panic!(
                "shared component `{}` changed its key through a query; restore the key before the view ends, or use World::override_shared_component",
                std::any::type_name::<C>()
            );
        }
    }
}

/// The default `S` of a `Query`: no shared component
pub struct NoSharedComponent;

/// Only archetypes that do not carry `C`. Yields `()`.
pub struct WithoutShared<C: TSharedComponent>(PhantomData<C>);

mod sealed
{
    pub trait SharedParam {}
    pub trait SharedFilter {}
}

/// The `S` of `Query<T, S, F>`: `&C`, `&mut C`, `WithoutShared<C>`, `NoSharedComponent`, or a
/// tuple of up to four of them.
pub trait TSharedComponentQueryParam: sealed::SharedParam
{
    type Item<'a>;

    #[doc(hidden)]
    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>;

    /// # Safety
    /// `chunk` must carry every shared component this parameter reads or writes, and nothing
    /// else may write those values while the returned item is alive.
    #[doc(hidden)]
    #[track_caller]
    unsafe fn fetch(chunk: &ArchetypeChunk) -> Self::Item<'_>;
}

impl sealed::SharedParam for NoSharedComponent {}
impl TSharedComponentQueryParam for NoSharedComponent
{
    type Item<'a> = ();

    fn access_scope(_: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }

    unsafe fn fetch(_: &ArchetypeChunk) {}
}

impl<C: TSharedComponent> sealed::SharedParam for &C {}
impl<C: TSharedComponent> TSharedComponentQueryParam for &C
{
    type Item<'a> = &'a C;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.read.insert(component_specs.register_shared::<C>());
        Ok(scope)
    }

    #[track_caller]
    unsafe fn fetch(chunk: &ArchetypeChunk) -> &C
    {
        let Some((_, value)) = chunk.entry_of::<C>()
        else
        {
            panic!("archetype was selected without the shared component `{}`", std::any::type_name::<C>())
        };
        unsafe { &*value }
    }
}

impl<C: TSharedComponent> sealed::SharedParam for &mut C {}
impl<C: TSharedComponent> TSharedComponentQueryParam for &mut C
{
    type Item<'a> = SharedMut<'a, C>;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.write.insert(component_specs.register_shared::<C>());
        Ok(scope)
    }

    #[track_caller]
    unsafe fn fetch(chunk: &ArchetypeChunk) -> SharedMut<'_, C>
    {
        let Some((key, value)) = chunk.entry_of::<C>()
        else
        {
            panic!("archetype was selected without the shared component `{}`", std::any::type_name::<C>())
        };
        unsafe {
            SharedMut {
                key:   &*key,
                value: &mut *value,
            }
        }
    }
}

impl<C: TSharedComponent> sealed::SharedParam for WithoutShared<C> {}
impl<C: TSharedComponent> TSharedComponentQueryParam for WithoutShared<C>
{
    type Item<'a> = ();

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.exclude.insert(component_specs.register_shared::<C>());
        Ok(scope)
    }

    unsafe fn fetch(_: &ArchetypeChunk) {}
}

macro_rules! shared_tuple {
    ($($p:ident),+) => {
        impl<$($p: TSharedComponentQueryParam),+> sealed::SharedParam for ($($p,)+) {}
        impl<$($p: TSharedComponentQueryParam),+> TSharedComponentQueryParam for ($($p,)+)
        {
            type Item<'a> = ($($p::Item<'a>,)+);

            fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
            {
                let mut scope = AccessScope::default();
                $(scope.extend($p::access_scope(component_specs)?)?;)+
                Ok(scope)
            }

            unsafe fn fetch(chunk: &ArchetypeChunk) -> Self::Item<'_>
            {
                // `access_scope` already rejected a tuple naming the same component twice with a write
                unsafe { ($($p::fetch(chunk),)+) }
            }
        }
    };
}
shared_tuple!(P0, P1);
shared_tuple!(P0, P1, P2);
shared_tuple!(P0, P1, P2, P3);

/// A static shared key filter, the `F` of `Query<T, S, F>`. `filter_key` is called once per world,
/// when the query is first created.
pub trait TSharedFilter: 'static
{
    type SharedComponents: TSharedComponent;
    fn filter_key() -> <Self::SharedComponents as TSharedComponent>::Key;
}

/// The default `F` of a `Query`: no filter
pub struct NoSharedFilter;

/// What a `Query` needs from its `F`. Implemented for [`NoSharedFilter`] and for every
/// [`TSharedFilter`].
pub trait TSharedFilterParam: sealed::SharedFilter + 'static
{
    /// The shared component the filter reads the key of, registering it on first sight
    #[doc(hidden)]
    fn component_id(component_specs: &mut ComponentSpecs) -> Option<usize>;

    /// The shared value an archetype must hold to pass the filter
    #[doc(hidden)]
    fn resolve(world: &mut World) -> Option<SharedValue>;
}

impl sealed::SharedFilter for NoSharedFilter {}
impl TSharedFilterParam for NoSharedFilter
{
    fn component_id(_: &mut ComponentSpecs) -> Option<usize>
    {
        None
    }

    fn resolve(_: &mut World) -> Option<SharedValue>
    {
        None
    }
}

impl<F: TSharedFilter> sealed::SharedFilter for F {}
impl<F: TSharedFilter> TSharedFilterParam for F
{
    fn component_id(component_specs: &mut ComponentSpecs) -> Option<usize>
    {
        Some(component_specs.register_shared::<F::SharedComponents>())
    }

    fn resolve(world: &mut World) -> Option<SharedValue>
    {
        Some(world.shared_value_of::<F::SharedComponents>(&F::filter_key()))
    }
}

type SharedCommand = Box<dyn FnOnce(&mut World) -> Result<(), XynokEcsError>>;

/// Shared component changes recorded while a query is running, applied once its borrows have
/// ended. Each method queues the `World` method of the same name.
#[derive(Default)]
pub struct SharedCommands
{
    commands: Vec<SharedCommand>,
}
impl SharedCommands
{
    pub fn add_shared_component<C: TSharedComponent>(&mut self, entity: Entity, value: C)
    {
        self.commands.push(Box::new(move |world| world.add_shared_component(entity, value)));
    }

    pub fn add_shared_component_by_key<C: TSharedComponent>(&mut self, entity: Entity, key: C::Key)
    {
        self.commands.push(Box::new(move |world| world.add_shared_component_by_key::<C>(entity, key)));
    }

    pub fn remove_shared_component<C: TSharedComponent>(&mut self, entity: Entity)
    {
        self.commands.push(Box::new(move |world| world.remove_shared_component::<C>(entity)));
    }

    pub fn set_shared_component_by_key<C: TSharedComponent>(&mut self, entity: Entity, key: C::Key)
    {
        self.commands.push(Box::new(move |world| world.set_shared_component_by_key::<C>(entity, key)));
    }

    pub fn set_shared_component<C: TSharedComponent>(&mut self, entity: Entity, value: C)
    {
        self.commands.push(Box::new(move |world| world.set_shared_component(entity, value)));
    }

    pub fn override_shared_component<C: TSharedComponent>(&mut self, entity: Entity, value: C)
    {
        self.commands.push(Box::new(move |world| world.override_shared_component(entity, value)));
    }

    /// Runs the commands in the order they were recorded and returns one result per command. A
    /// failed command does not undo the ones before it.
    pub fn apply(&mut self, world: &mut World) -> Vec<Result<(), XynokEcsError>>
    {
        self.commands.drain(..).map(|command| command(world)).collect()
    }
}

/// Borrow rules the compiler enforces for shared views.
///
/// Two views of the same query cannot be alive at once:
/// ```compile_fail
/// use xynok_ecs::{shared::TSharedComponent, world::World};
/// #[derive(Clone)]
/// struct Mesh(u32);
/// impl TSharedComponent for Mesh { type Key = u32; fn shared_key(&self) -> u32 { self.0 } }
/// let mut world = World::default();
/// let mut query = world.create_shared_query::<(), &mut Mesh>();
/// let first = query.filter_shared::<Mesh>(&7);
/// let second = query.filter_shared::<Mesh>(&7);
/// drop((first, second));
/// ```
///
/// The world cannot change structure while a query is alive:
/// ```compile_fail
/// use xynok_ecs::{shared::TSharedComponent, world::World};
/// #[derive(Clone)]
/// struct Mesh(u32);
/// impl TSharedComponent for Mesh { type Key = u32; fn shared_key(&self) -> u32 { self.0 } }
/// let mut world = World::default();
/// let entity = world.create(());
/// let mut query = world.create_shared_query::<(), &mut Mesh>();
/// world.destroy(entity);
/// query.iter_archetype().next();
/// ```
///
/// A read-only view cannot write the value:
/// ```compile_fail
/// use xynok_ecs::{shared::TSharedComponent, world::World};
/// #[derive(Clone)]
/// struct Mesh(u32);
/// impl TSharedComponent for Mesh { type Key = u32; fn shared_key(&self) -> u32 { self.0 } }
/// let mut world = World::default();
/// let mut query = world.create_shared_query::<(), &Mesh>();
/// for (mesh, _) in query.iter_archetype() { mesh.0 = 8; }
/// ```
mod borrow_contract
{}

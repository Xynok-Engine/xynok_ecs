use std::any::TypeId;

use crate::apis::ComponentDescriptor;
use crate::apis::identifies::{StateDetection, StorageLocation, XynokEcsError};
use crate::chunk::Chunk;
use crate::chunk::layout::ChunkLayout;
use crate::query::mut_ref::TMutPolicy;

pub trait TComponent: Sized
{
    type QueryType: TComponent + 'static;
    type StorageType: TComponent + 'static;

    /// Picks what a `&mut Self` query hands out for one row: `TrackChanges` (a `Mut<Self>` that
    /// stamps the changed tick on write) for a `ChangeAble` component, `NoTracking` (a plain
    /// `&mut Self`) for everything else.
    ///
    /// The `#[component(...)]` macro fills this in from the flags you wrote, so you only think
    /// about it when hand-writing a `TComponent` impl, where `NoTracking` is the answer unless
    /// you also implement [`TChangeAble`].
    type MutPolicy: TMutPolicy;
    const STORAGE_LOCATION: StorageLocation;
    const STATE_DETECTION: StateDetection;

    /// The enable state this component starts out with, used only when the component is
    /// [`TEnableAble`]. It is read exactly once per row, when the component is first written
    /// into that row (see `Chunk::write_at`); after that the bit lives in the chunk and only
    /// changes when someone flips it.
    ///
    /// You rarely set this by hand. The usual way is the `Enable<T>` / `Disable<T>` wrappers,
    /// which override it so `world.create(Disable::new(Hp(10)))` spawns with `Hp` already off.
    ///
    /// This value has nothing to do with filtering at query time. Picking rows by enable state
    /// is what `Enabled<Q>` and `Disabled<Q>` are for, and they read the bit in the chunk, not
    /// this constant.
    const ENABLE_VALUE: bool = true;
}
pub trait TEnableAble {}
pub trait TChangeAble {}

pub trait TComponentDescriptor
{
    const COMPONENT_DESCRIPTOR: ComponentDescriptor;
}
impl<T: TComponent + 'static> TComponentDescriptor for T
{
    const COMPONENT_DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
        fn_name:          std::any::type_name::<T::StorageType>,
        storage_type_id:  std::any::TypeId::of::<T::StorageType>(),
        query_type_id:    std::any::TypeId::of::<T::QueryType>(),
        byte_size:        std::mem::size_of::<T::StorageType>(),
        align:            std::mem::align_of::<T::StorageType>(),
        storage_location: T::STORAGE_LOCATION,
        state_detection:  T::STATE_DETECTION,
        fn_drop:          drop_glue::<T::StorageType>,
    };
}

fn drop_glue<T>(ptr: *mut u8)
{
    unsafe {
        std::ptr::drop_in_place(ptr as *mut T);
    }
}

pub trait TArchetype: Sized
{
    const COMPONENT_DESCRIPTORS: &[ComponentDescriptor];
    const QUERY_TYPE_IDS: &[TypeId];
    const STORAGE_TYPE_IDS: &[TypeId];

    fn write_at(layout: &ChunkLayout, chunk: &mut Chunk, write_idx: usize, val: Self, tick: crate::apis::constants::ChangedTick) -> Result<(), XynokEcsError>;
    fn replace_at(layout: &ChunkLayout, chunk: &mut Chunk, row: usize, val: Self, tick: crate::apis::constants::ChangedTick) -> Result<(), XynokEcsError>;
    fn take_from(layout: &ChunkLayout, chunk: &mut Chunk, idx: usize) -> Result<Self, XynokEcsError>;
}

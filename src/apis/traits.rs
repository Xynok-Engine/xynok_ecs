use std::any::TypeId;

use crate::apis::identifies::{StateDetection, StorageLocation, XynokEcsError};
use crate::apis::ComponentDescriptor;
use crate::chunk::layout::ChunkLayout;
use crate::chunk::Chunk;

pub trait TComponent: Sized
{
    type QueryType: TComponent + 'static;
    type StorageType: TComponent + 'static;
    const STORAGE_LOCATION: StorageLocation;
    const STATE_DETECTION: StateDetection;

    /// This value is used when a component implements [`TEnableAble`] for two purposes:
    /// - When initializing this component, the enable state is set to this value.
    /// - When querying this component, the query is valid if this value matches the enable state.
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

pub trait TEnablableComponent {}
pub trait TAddedAbleComponent {}
pub trait TChangedAbleComponent {}

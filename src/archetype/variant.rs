use crate::{
    apis::{
        identifies::XynokEcsError,
        traits::{TArchetype, TComponent, TComponentDescriptor},
    },
    chunk::layout::ChunkLayout,
};
use std::any::TypeId;
impl<T: TComponent + 'static> TArchetype for T
{
    const COMPONENT_DESCRIPTORS: &[crate::apis::ComponentDescriptor] = &[T::COMPONENT_DESCRIPTOR];
    const QUERY_TYPE_IDS: &[std::any::TypeId] = &[TypeId::of::<T::QueryType>()];
    const STORAGE_TYPE_IDS: &[std::any::TypeId] = &[TypeId::of::<T::StorageType>()];

    fn write_at(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, write_idx: usize, val: Self) -> Result<(), XynokEcsError>
    {
        unsafe { chunk.write_at::<T>(layout, write_idx, val) }
    }

    fn take_from(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, idx: usize) -> Result<Self, XynokEcsError>
    {
        unsafe { chunk.take_at::<T>(layout, idx) }
    }

    fn replace_at(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, row: usize, val: Self) -> Result<(), XynokEcsError>
    {
        unsafe { chunk.replace_at::<T>(layout, row, val) }
    }
}

macro_rules! tuple_arch {
    (
        $([$component:ident,$idx:tt]),* $(,)?
    ) =>
    {
        impl<$($component,)*> TArchetype for ($($component,)*)
        where $($component: TComponent +'static,)*
        {
            const COMPONENT_DESCRIPTORS: &[crate::apis::ComponentDescriptor] = &[$($component::COMPONENT_DESCRIPTOR,)*];
            const QUERY_TYPE_IDS: &[std::any::TypeId] = &[$(TypeId::of::<$component::QueryType>(),)*];
            const STORAGE_TYPE_IDS: &[std::any::TypeId] = &[$(TypeId::of::<$component::StorageType>(),)*];

            fn write_at(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, write_idx: usize, val: Self) -> Result<(), XynokEcsError>
            {
                unsafe
                {
                    $(chunk.write_at::<$component>(layout, write_idx, val.$idx)?;)*
                }
                Ok(())
            }

            fn take_from(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, idx: usize) -> Result<Self, XynokEcsError>
            {
                unsafe { Ok(($(chunk.take_at::<$component>(layout, idx)?,)*)) }
            }

            fn replace_at(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, row: usize, val: Self) -> Result<(), XynokEcsError>
            {
                unsafe
                {
                    $(chunk.replace_at::<$component>(layout, row, val.$idx)?;)*
                }
                Ok(())
            }

        }
    };
}
#[rustfmt::skip] tuple_arch!([C0, 0]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9], [C10, 10]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9], [C10, 10], [C11, 11]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9], [C10, 10], [C11, 11], [C12, 12]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9], [C10, 10], [C11, 11], [C12, 12], [C13, 13]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9], [C10, 10], [C11, 11], [C12, 12], [C13, 13], [C14, 14]);
#[rustfmt::skip] tuple_arch!([C0, 0], [C1, 1], [C2, 2], [C3, 3], [C4, 4], [C5, 5], [C6, 6], [C7, 7], [C8, 8], [C9, 9], [C10, 10], [C11, 11], [C12, 12], [C13, 13], [C14, 14], [C15, 15]);

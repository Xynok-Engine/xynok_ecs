//#![allow(unused)]
use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::internal_traits::TQuerySrcAccess;
use crate::apis::traits::TComponent;
use crate::chunk::Chunk;
use crate::world::arch_spec::{ArchetypeSpec, ArchetypeSpecs};
use crate::world::query_spec::QuerySpecAccessor;

/// Base address of `C`'s column inside a chunk.
///
/// # Panics
///
/// If the archetype does not carry that column. Not a redundant check: every caller comes through
/// the pre-filtered archetype list, so a miss here means the filter and the read have drifted
/// apart, and silently reading a wrong offset is far worse.
#[inline]
#[track_caller]
pub(crate) fn column_ptr<C: TComponent + 'static>(arch_spec: &ArchetypeSpec, chunk: &Chunk) -> *mut u8
{
    let col_des = match arch_spec.layout.component_col_descriptors.get(&TypeId::of::<C::StorageType>())
    {
        Some(col_des) => col_des,
        None => panic!(
            "archetype does not carry a column for component `{}` even though it was pre-filtered to contain it",
            std::any::type_name::<C::StorageType>()
        ),
    };
    unsafe { chunk.ptr().add(col_des.offset) }
}

pub struct SrcAccess<'a>
{
    archetypes:        &'a ArchetypeSpecs,
    arch_indices:      &'a [usize],
    total_arch:        usize,
    current_arch_idx:  usize,
    current_chunk_idx: usize,
    current_row_idx:   usize,
    current_chunk_len: usize,
    current_col_ptr:   *const u8,
    _lifetime:         PhantomData<&'a ()>,
}
impl<'a> TQuerySrcAccess for SrcAccess<'a>
{
    fn new(accessor: &QuerySpecAccessor) -> Self
    {
        let arch_indices = unsafe { accessor.arch_indices() };
        Self {
            archetypes:        unsafe { &*accessor.archetypes },
            arch_indices:      arch_indices,
            total_arch:        arch_indices.len(),
            current_arch_idx:  0,
            current_chunk_idx: 0,
            current_row_idx:   0,
            current_chunk_len: 0,
            current_col_ptr:   std::ptr::null(),
            _lifetime:         PhantomData,
        }
    }
}
impl<'a> SrcAccess<'a>
{
    #[inline]
    #[track_caller]
    pub(crate) fn next<T: TComponent + 'static>(&mut self) -> Option<&'a T>
    {
        loop
        {
            let row = self.current_row_idx;
            if row < self.current_chunk_len
            {
                self.current_row_idx = row + 1;
                return Some(unsafe { &*(self.current_col_ptr as *const T).add(row) });
            }

            if !self.advance_to_next_chunk::<T>()
            {
                return None;
            }
        }
    }
    #[inline]
    #[track_caller]
    pub(crate) fn next_mut<T: TComponent + 'static>(&mut self) -> Option<&'a mut T>
    {
        loop
        {
            let row = self.current_row_idx;
            if row < self.current_chunk_len
            {
                self.current_row_idx = row + 1;
                return Some(unsafe { &mut *(self.current_col_ptr as *mut T).add(row) });
            }

            if !self.advance_to_next_chunk::<T>()
            {
                return None;
            }
        }
    }

    #[inline]
    #[track_caller]
    fn advance_to_next_chunk<T: TComponent + 'static>(&mut self) -> bool
    {
        while self.current_arch_idx < self.total_arch
        {
            let arch_idx = self.arch_indices[self.current_arch_idx];
            let arch_spec = match self.archetypes.value_at(arch_idx)
            {
                Some(arch_spec) => arch_spec,
                None => panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry"),
            };

            if self.current_chunk_idx >= arch_spec.arch.chunk_count()
            {
                self.current_arch_idx += 1;
                self.current_chunk_idx = 0;
                continue;
            }

            let chunk = arch_spec.arch.chunk_at(self.current_chunk_idx);
            self.current_chunk_idx += 1;

            if chunk.is_empty()
            {
                continue;
            }

            let col_des = match arch_spec.layout.component_col_descriptors.get(&TypeId::of::<T::StorageType>())
            {
                Some(col_des) => col_des,
                None => panic!(
                    "archetype does not carry a column for component `{}` even though it was pre-filtered to contain it",
                    std::any::type_name::<T::StorageType>()
                ),
            };

            self.current_col_ptr = unsafe { chunk.ptr().add(col_des.offset) };
            self.current_chunk_len = chunk.len();
            self.current_row_idx = 0;
            return true;
        }
        false
    }
}

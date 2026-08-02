//#![allow(unused)]
use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;

use crate::apis::internal_traits::TQuerySrcAccess;
use crate::apis::params::ComponentSpec;
use crate::apis::traits::TComponent;
use crate::world::arch_spec::ArchetypeSpec;

pub struct SrcAccess<'a>
{
    archetypes:        &'a mut Vec<*mut ArchetypeSpec>,
    component_specs:   &'a HashMap<TypeId, ComponentSpec>,
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
    fn new(arch: *mut Vec<*mut ArchetypeSpec>, specs: *const HashMap<TypeId, ComponentSpec>) -> Self
    {
        let archetypes = unsafe { &mut *arch };
        let total_arch = archetypes.len();
        Self {
            archetypes:        archetypes,
            component_specs:   unsafe { &*specs },
            total_arch:        total_arch,
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
            let arch_spec = unsafe { &*self.archetypes[self.current_arch_idx] };

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

//#![allow(unused)]
use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::internal_traits::{TQueryColumn, TQuerySrcAccess};
use crate::apis::traits::TComponent;
use crate::archetype::Archetype;
use crate::apis::constants::ChangedTick;
use crate::chunk::read_bit;
use crate::query::mut_ref::WriteStamp;
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::QuerySpecAccessor;

pub struct SrcAccessEnable<'a, const V: bool>
{
    archetypes:            &'a ArchetypeSpecs,
    arch_indices:          &'a [usize],
    total_arch:            usize,
    current_arch_idx:      usize,
    current_arch:          Option<&'a Archetype>,
    current_chunk_count:   usize,
    current_offset:        usize,
    current_enable_offset: usize,
    current_chunk_idx:     usize,
    current_row_idx:       usize,
    current_chunk_len:     usize,
    current_col_ptr:       *const u8,
    current_enable_ptr:    *mut u8,
    // enable state and change detection are independent, so a `&mut` row filtered by enable bit
    // still has to record its write in the changed region
    current_changed_offset: Option<usize>,
    current_changed_ptr:   *mut u8,
    this_run_tick:         ChangedTick,
    _lifetime:             PhantomData<&'a ()>,
}
impl<'a, const V: bool> TQuerySrcAccess<'a> for SrcAccessEnable<'a, V>
{
    fn new(accessor: &QuerySpecAccessor<'a>) -> Self
    {
        let arch_indices = accessor.arch_indices();
        Self {
            archetypes:            accessor.archetypes,
            arch_indices:          arch_indices,
            total_arch:            arch_indices.len(),
            current_arch_idx:      0,
            current_arch:          None,
            current_chunk_count:   0,
            current_offset:        0,
            current_enable_offset: 0,
            current_chunk_idx:     0,
            current_row_idx:       0,
            current_chunk_len:     0,
            current_col_ptr:       std::ptr::null(),
            current_enable_ptr:    std::ptr::null_mut(),
            current_changed_offset: None,
            current_changed_ptr:   std::ptr::null_mut(),
            this_run_tick:         accessor.this_run_tick,
            _lifetime:             PhantomData,
        }
    }
}
impl<'a, const V: bool> SrcAccessEnable<'a, V>
{
    /// `Q` is `&Component` or `&mut Component` (anything implementing `TQueryColumn`): the read
    /// vs. write choice only affects how the row is handed out (`Q::read_from`), not whether it
    /// passes the enable-bit filter, so one generic method covers both
    #[inline]
    #[track_caller]
    pub(crate) fn next<Q: TQueryColumn>(&mut self) -> Option<Q::QueryItem<'a>>
    {
        loop
        {
            let row = self.current_row_idx;
            if row < self.current_chunk_len
            {
                self.current_row_idx = row + 1;
                if unsafe { read_bit(self.current_enable_ptr, row) } != V
                {
                    continue;
                }
                let stamp = WriteStamp {
                    changed_ptr: self.current_changed_ptr,
                    tick:        self.this_run_tick,
                };
                return Some(unsafe { Q::read_from(self.current_col_ptr as *mut u8, row, stamp) });
            }

            if !self.advance_to_next_chunk::<Q::Component>()
            {
                return None;
            }
        }
    }

    #[inline]
    #[track_caller]
    fn advance_to_next_chunk<T: TComponent + 'static>(&mut self) -> bool
    {
        loop
        {
            while self.current_chunk_idx < self.current_chunk_count
            {
                let chunk = unsafe { self.current_arch.unwrap_unchecked() }.chunk_at(self.current_chunk_idx);
                self.current_chunk_idx += 1;

                if chunk.is_empty()
                {
                    continue;
                }

                self.current_col_ptr = unsafe { chunk.ptr().add(self.current_offset) };
                self.current_enable_ptr = unsafe { chunk.ptr().add(self.current_enable_offset) };
                self.current_changed_ptr = match self.current_changed_offset
                {
                    Some(offset) => unsafe { chunk.ptr().add(offset) },
                    None => std::ptr::null_mut(),
                };
                self.current_chunk_len = chunk.len();
                self.current_row_idx = 0;
                return true;
            }

            if self.current_arch_idx >= self.total_arch
            {
                return false;
            }

            let arch_idx = self.arch_indices[self.current_arch_idx];
            self.current_arch_idx += 1;

            let arch_spec = match self.archetypes.value_at(arch_idx)
            {
                Some(arch_spec) => arch_spec,
                None => panic!("archetype index {arch_idx} cached by the query is not in the world's archetype registry"),
            };

            let col_des = match arch_spec.layout.component_col_descriptors.get(&TypeId::of::<T::StorageType>())
            {
                Some(col_des) => col_des,
                None => panic!(
                    "archetype does not carry a column for component `{}` even though it was pre-filtered to contain it",
                    std::any::type_name::<T::StorageType>()
                ),
            };
            let enable_offset = col_des.state_offset.enable_offset.unwrap_or_else(|| {
                panic!(
                    "component `{}` does not support enable state detection",
                    std::any::type_name::<T::StorageType>()
                )
            });

            self.current_offset = col_des.offset;
            self.current_enable_offset = enable_offset;
            self.current_changed_offset = col_des.state_offset.changed_offset;
            self.current_arch = Some(&arch_spec.arch);
            self.current_chunk_count = arch_spec.arch.chunk_count();
            self.current_chunk_idx = 0;
        }
    }
}

use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::constants::ChangedTick;
use crate::apis::internal_traits::{TQueryColumn, TQuerySrcAccess};
use crate::apis::traits::TComponent;
use crate::archetype::Archetype;
use crate::chunk::read_changed_tick;
use crate::query::mut_ref::WriteStamp;
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::QuerySpecAccessor;

pub struct SrcAccessChanged<'a>
{
    archetypes:             &'a ArchetypeSpecs,
    arch_indices:           &'a [usize],
    total_arch:             usize,
    current_arch_idx:       usize,
    current_arch:           Option<&'a Archetype>,
    current_chunk_count:    usize,
    current_offset:         usize,
    current_changed_offset: usize,
    current_chunk_idx:      usize,
    current_row_idx:        usize,
    current_chunk_len:      usize,
    current_col_ptr:        *const u8,
    current_changed_ptr:    *mut u8,
    last_run_tick:          ChangedTick,
    this_run_tick:          ChangedTick,
    _lifetime:              PhantomData<&'a ()>,
}
impl<'a> TQuerySrcAccess<'a> for SrcAccessChanged<'a>
{
    fn new(accessor: &QuerySpecAccessor<'a>) -> Self
    {
        let arch_indices = accessor.arch_indices();
        Self {
            archetypes:             accessor.archetypes,
            arch_indices:           arch_indices,
            total_arch:             arch_indices.len(),
            current_arch_idx:       0,
            current_arch:           None,
            current_chunk_count:    0,
            current_offset:         0,
            current_changed_offset: 0,
            current_chunk_idx:      0,
            current_row_idx:        0,
            current_chunk_len:      0,
            current_col_ptr:        std::ptr::null(),
            current_changed_ptr:    std::ptr::null_mut(),
            last_run_tick:          accessor.last_run_tick,
            this_run_tick:          accessor.this_run_tick,
            _lifetime:              PhantomData,
        }
    }
}
impl<'a> SrcAccessChanged<'a>
{
    /// `Q` is `&Component` or `&mut Component` (anything implementing `TQueryColumn`): the read
    /// vs. write choice only affects how the row is handed out (`Q::read_from`), not whether it
    /// passes the changed-tick filter, so one generic method covers both
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
                if !crate::utils::is_newer_than(self.changed_tick_at(row), self.last_run_tick, self.this_run_tick)
                {
                    continue;
                }
                // the region this filter reads is the very one a write records into, so the
                // stamp costs nothing extra here
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
                self.current_changed_ptr = unsafe { chunk.ptr().add(self.current_changed_offset) };
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
            let changed_offset = col_des.state_offset.changed_offset.unwrap_or_else(|| {
                panic!(
                    "component `{}` does not support changed state detection",
                    std::any::type_name::<T::StorageType>()
                )
            });

            self.current_offset = col_des.offset;
            self.current_changed_offset = changed_offset;
            self.current_arch = Some(&arch_spec.arch);
            self.current_chunk_count = arch_spec.arch.chunk_count();
            self.current_chunk_idx = 0;
        }
    }

    /// Reads the changed tick of the row just handed out by `next`
    #[inline]
    pub(crate) fn changed_tick_at(&self, row: usize) -> ChangedTick
    {
        unsafe { read_changed_tick(self.current_changed_ptr, row) }
    }
}

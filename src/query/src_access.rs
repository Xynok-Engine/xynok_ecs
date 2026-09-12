use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::constants::ChangedTick;
use crate::apis::internal_traits::TQuerySrcAccess;
use crate::apis::traits::TComponent;
use crate::archetype::Archetype;
use crate::query::mut_ref::{Mut, WriteStamp};
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::QuerySpecAccessor;

pub struct SrcAccess<'a>
{
    archetypes:          &'a ArchetypeSpecs,
    arch_indices:        &'a [usize],
    total_arch:          usize,
    current_arch_idx:    usize,
    current_arch:        Option<&'a Archetype>,
    current_chunk_count: usize,
    current_offset:      usize,
    current_chunk_idx:   usize,
    current_row_idx:     usize,
    current_chunk_len:   usize,
    current_col_ptr:     *const u8,
    // resolved alongside `current_col_ptr` so `next_mut` can hand each row a `WriteStamp`
    // without touching the column descriptor again; stays null while the component tracks no
    // changes, and a `Mut` holding a null stamp simply records nothing
    current_changed_offset: Option<usize>,
    current_stamp:       WriteStamp,
    this_run_tick:       ChangedTick,
    _lifetime:           PhantomData<&'a ()>,
}
impl<'a> TQuerySrcAccess<'a> for SrcAccess<'a>
{
    fn new(accessor: &QuerySpecAccessor<'a>) -> Self
    {
        let arch_indices = accessor.arch_indices();
        Self {
            archetypes:          accessor.archetypes,
            arch_indices:        arch_indices,
            total_arch:          arch_indices.len(),
            current_arch_idx:    0,
            current_arch:        None,
            current_chunk_count: 0,
            current_offset:      0,
            current_chunk_idx:   0,
            current_row_idx:     0,
            current_chunk_len:   0,
            current_col_ptr:     std::ptr::null(),
            current_changed_offset: None,
            current_stamp:       WriteStamp::NONE,
            this_run_tick:       accessor.this_run_tick,
            _lifetime:           PhantomData,
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
    pub(crate) fn next_mut<T: TComponent + 'static>(&mut self) -> Option<Mut<'a, T>>
    {
        loop
        {
            let row = self.current_row_idx;
            if row < self.current_chunk_len
            {
                self.current_row_idx = row + 1;
                return Some(unsafe { Mut::new(&mut *(self.current_col_ptr as *mut T).add(row), self.current_stamp, row) });
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
        loop
        {
            while self.current_chunk_idx < self.current_chunk_count
            {
                // SAFETY: `current_chunk_count` is only ever set right after `current_arch` is,
                // so it being non-zero implies `current_arch` is `Some`
                let chunk = unsafe { self.current_arch.unwrap_unchecked() }.chunk_at(self.current_chunk_idx);
                self.current_chunk_idx += 1;

                if chunk.is_empty()
                {
                    continue;
                }

                self.current_col_ptr = unsafe { chunk.ptr().add(self.current_offset) };
                self.current_stamp = WriteStamp {
                    changed_ptr: match self.current_changed_offset
                    {
                        Some(offset) => unsafe { chunk.ptr().add(offset) },
                        None => std::ptr::null_mut(),
                    },
                    tick:        self.this_run_tick,
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

            self.current_offset = col_des.offset;
            self.current_changed_offset = col_des.state_offset.changed_offset;
            self.current_arch = Some(&arch_spec.arch);
            self.current_chunk_count = arch_spec.arch.chunk_count();
            self.current_chunk_idx = 0;
        }
    }
}

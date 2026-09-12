use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryParam, TQueryParamFiltered, TQuerySrcAccess, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::query::mut_ref::WriteStamp;
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::QuerySpecAccessor;

macro_rules! impl_tuple_query_param {
    ($src:ident; $($q:ident : $ptr:ident : $state_ptr:ident : $changed_ptr:ident),+) => {
        pub struct $src<'a, $($q: TQueryParamFiltered),+>
        {
            archetypes:        &'a ArchetypeSpecs,
            arch_indices:      &'a [usize],
            total_arch:        usize,
            current_arch_idx:  usize,
            current_chunk_idx: usize,
            current_row_idx:   usize,
            current_chunk_len: usize,
            $($ptr: *mut u8,)+
            // one extra pointer per element, into whatever state region (added/changed/enable)
            // that element needs to check `accepts`; null (and never dereferenced) for a plain
            // column, which always accepts every row
            $($state_ptr: *mut u8,)+
            // and one more per element, into that column's changed region, so a `&mut` element
            // can record its write; null when the component tracks no changes
            $($changed_ptr: *mut u8,)+
            // this system's own last-run tick and the tick as of this run (see
            // `QuerySpecAccessor::last_run_tick`/`this_run_tick`), shared by every element's
            // `accepts` check
            last_run_tick:     ChangedTick,
            this_run_tick:     ChangedTick,
            _p: PhantomData<($($q,)+)>,
        }
        impl<'a, $($q: TQueryParamFiltered),+> TQuerySrcAccess<'a> for $src<'a, $($q,)+>
        {
            fn new(accessor: &QuerySpecAccessor<'a>) -> Self
            {
                let arch_indices = accessor.arch_indices();
                Self {
                    archetypes:        accessor.archetypes,
                    arch_indices:      arch_indices,
                    total_arch:        arch_indices.len(),
                    current_arch_idx:  0,
                    current_chunk_idx: 0,
                    current_row_idx:   0,
                    current_chunk_len: 0,
                    $($ptr: std::ptr::null_mut(),)+
                    $($state_ptr: std::ptr::null_mut(),)+
                    $($changed_ptr: std::ptr::null_mut(),)+
                    last_run_tick:     accessor.last_run_tick,
                    this_run_tick:     accessor.this_run_tick,
                    _p: PhantomData,
                }
            }
        }
        impl<'a, $($q: TQueryParamFiltered),+> $src<'a, $($q,)+>
        {
            #[inline]
            #[track_caller]
            pub(crate) fn next(&mut self) -> Option<($($q::QueryItem<'a>,)+)>
            {
                loop
                {
                    let row = self.current_row_idx;
                    if row < self.current_chunk_len
                    {
                        self.current_row_idx = row + 1;

                        let accepted = unsafe { $($q::accepts(self.$state_ptr, row, self.last_run_tick, self.this_run_tick) &&)+ true };
                        if !accepted
                        {
                            continue;
                        }
                        return Some(($(unsafe {
                            $q::read_from(self.$ptr, row, WriteStamp { changed_ptr: self.$changed_ptr, tick: self.this_run_tick })
                        },)+));
                    }

                    if !self.advance_to_next_chunk()
                    {
                        return None;
                    }
                }
            }

            /// Crosses into the next non-empty chunk, resolving every column's base pointer (and
            /// every filter element's state pointer) in one pass so the archetype/chunk pointer
            /// chain is only walked once per chunk, not once per component.
            #[inline]
            #[track_caller]
            fn advance_to_next_chunk(&mut self) -> bool
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

                    let chunk_ptr = chunk.ptr();
                    $(
                        // a marker element (`IS_IGNORE_FILTER`) leaves all three pointers null:
                        // its component is the one the archetype is guaranteed *not* to carry, so
                        // there is no descriptor to look up. The branch is const, it costs nothing
                        // at runtime.
                        if !<$q as TQueryParamFiltered>::IS_IGNORE_FILTER
                        {
                            // every archetype in `self.archetypes` was pre-filtered (see
                            // `build_archetype_which_contains`) to carry all of the tuple's columns
                            let col_des = match arch_spec.layout.component_col_descriptors.get(&TypeId::of::<<<$q as TQueryParamFiltered>::Component as TComponent>::StorageType>())
                            {
                                Some(col_des) => col_des,
                                None => panic!(
                                    "archetype does not carry a column for component `{}` even though it was pre-filtered to contain it",
                                    std::any::type_name::<<<$q as TQueryParamFiltered>::Component as TComponent>::StorageType>()
                                ),
                            };
                            self.$ptr = unsafe { chunk_ptr.add(col_des.offset) };
                            self.$state_ptr = match $q::state_offset(col_des)
                            {
                                Some(state_offset) => unsafe { chunk_ptr.add(state_offset) },
                                None => std::ptr::null_mut(),
                            };
                            self.$changed_ptr = match col_des.state_offset.changed_offset
                            {
                                Some(changed_offset) => unsafe { chunk_ptr.add(changed_offset) },
                                None => std::ptr::null_mut(),
                            };
                        }
                    )+
                    self.current_chunk_len = chunk.len();
                    self.current_row_idx = 0;
                    return true;
                }
                false
            }
        }
        impl<$($q: TQueryParamFiltered),+> TQueryParam for ($($q,)+)
        {
            type QueryItem<'a> = ($($q::QueryItem<'a>,)+);
            type SrcAccess<'a> = $src<'a, $($q,)+>;
            type Shape = ($($q::Shape,)+);

            fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
            {
                let mut scope = AccessScope::default();
                $(scope.extend($q::access_scope(component_specs)?)?;)+
                Ok(scope)
            }

            #[track_caller]
            fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
            {
                src_access.next()
            }
        }
        // SAFETY: a tuple is read-only exactly when every element in it is read-only
        unsafe impl<$($q: TQueryParamFiltered + TReadOnlyQueryParam),+> TReadOnlyQueryParam for ($($q,)+) {}
    };
}

#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess2; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess3; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess4; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess5; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess6; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess7; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess8; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess9; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess10; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess11; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9, Q10: col_10: state_10: changed_10);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess12; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9, Q10: col_10: state_10: changed_10, Q11: col_11: state_11: changed_11);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess13; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9, Q10: col_10: state_10: changed_10, Q11: col_11: state_11: changed_11, Q12: col_12: state_12: changed_12);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess14; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9, Q10: col_10: state_10: changed_10, Q11: col_11: state_11: changed_11, Q12: col_12: state_12: changed_12, Q13: col_13: state_13: changed_13);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess15; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9, Q10: col_10: state_10: changed_10, Q11: col_11: state_11: changed_11, Q12: col_12: state_12: changed_12, Q13: col_13: state_13: changed_13, Q14: col_14: state_14: changed_14);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess16; Q0: col_0: state_0: changed_0, Q1: col_1: state_1: changed_1, Q2: col_2: state_2: changed_2, Q3: col_3: state_3: changed_3, Q4: col_4: state_4: changed_4, Q5: col_5: state_5: changed_5, Q6: col_6: state_6: changed_6, Q7: col_7: state_7: changed_7, Q8: col_8: state_8: changed_8, Q9: col_9: state_9: changed_9, Q10: col_10: state_10: changed_10, Q11: col_11: state_11: changed_11, Q12: col_12: state_12: changed_12, Q13: col_13: state_13: changed_13, Q14: col_14: state_14: changed_14, Q15: col_15: state_15: changed_15);

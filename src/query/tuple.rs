use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryColumn, TQueryParam, TQuerySrcAccess};
use crate::apis::params::ComponentSpec;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpec;

macro_rules! impl_tuple_query_param {
    ($src:ident; $($q:ident : $ptr:ident),+) => {
        pub struct $src<'a, $($q: TQueryColumn),+>
        {
            archetypes:        &'a mut Vec<*mut ArchetypeSpec>,
            //component_specs:   &'a HashMap<TypeId, ComponentSpec>,
            total_arch:        usize,
            current_arch_idx:  usize,
            current_chunk_idx: usize,
            current_row_idx:   usize,
            current_chunk_len: usize,
            $($ptr: *mut u8,)+
            _p: PhantomData<($($q,)+)>,
        }
        impl<'a, $($q: TQueryColumn),+> TQuerySrcAccess for $src<'a, $($q,)+>
        {
            fn new(arch: *mut Vec<*mut ArchetypeSpec>, _specs: *const HashMap<TypeId, ComponentSpec>) -> Self
            {
                let archetypes = unsafe { &mut *arch };
                let total_arch = archetypes.len();
                Self {
                    archetypes:        archetypes,
                    //component_specs:   unsafe { &*specs },
                    total_arch:        total_arch,
                    current_arch_idx:  0,
                    current_chunk_idx: 0,
                    current_row_idx:   0,
                    current_chunk_len: 0,
                    $($ptr: std::ptr::null_mut(),)+
                    _p: PhantomData,
                }
            }
        }
        impl<'a, $($q: TQueryColumn),+> $src<'a, $($q,)+>
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
                        return Some(($(unsafe { $q::read_from(self.$ptr, row) },)+));
                    }

                    if !self.advance_to_next_chunk()
                    {
                        return None;
                    }
                }
            }

            /// Crosses into the next non-empty chunk, resolving every column's base pointer in
            /// one pass so the archetype/chunk pointer chain is only walked once per chunk, not
            /// once per component.
            #[inline]
            #[track_caller]
            fn advance_to_next_chunk(&mut self) -> bool
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

                    let chunk_ptr = chunk.ptr();
                    $(
                        // every archetype in `self.archetypes` was pre-filtered (see
                        // `build_archetype_which_contains`) to carry all of the tuple's columns
                        let col_des = match arch_spec.layout.component_col_descriptors.get(&TypeId::of::<<<$q as TQueryColumn>::Component as TComponent>::StorageType>())
                        {
                            Some(col_des) => col_des,
                            None => panic!(
                                "archetype does not carry a column for component `{}` even though it was pre-filtered to contain it",
                                std::any::type_name::<<<$q as TQueryColumn>::Component as TComponent>::StorageType>()
                            ),
                        };
                        self.$ptr = unsafe { chunk_ptr.add(col_des.offset) };
                    )+
                    self.current_chunk_len = chunk.len();
                    self.current_row_idx = 0;
                    return true;
                }
                false
            }
        }
        impl<$($q: TQueryColumn),+> TQueryParam for ($($q,)+)
        {
            type QueryItem<'a> = ($($q::QueryItem<'a>,)+);
            type SrcAccess<'a> = $src<'a, $($q,)+>;
            const TYPE_ID: TypeId = TypeId::of::<($(<<$q as TQueryColumn>::Component as TComponent>::StorageType,)+)>();

            fn access_scope() -> Result<AccessScope, XynokEcsError>
            {
                let mut scope = AccessScope::default();
                $(scope.extend($q::access_scope()?)?;)+
                Ok(scope)
            }

            #[track_caller]
            fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
            {
                src_access.next()
            }
        }
    };
}

#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess2; Q0: col_0, Q1: col_1);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess3; Q0: col_0, Q1: col_1, Q2: col_2);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess4; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess5; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess6; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess7; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess8; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess9; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess10; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess11; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9, Q10: col_10);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess12; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9, Q10: col_10, Q11: col_11);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess13; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9, Q10: col_10, Q11: col_11, Q12: col_12);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess14; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9, Q10: col_10, Q11: col_11, Q12: col_12, Q13: col_13);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess15; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9, Q10: col_10, Q11: col_11, Q12: col_12, Q13: col_13, Q14: col_14);
#[rustfmt::skip] impl_tuple_query_param!(TupleSrcAccess16; Q0: col_0, Q1: col_1, Q2: col_2, Q3: col_3, Q4: col_4, Q5: col_5, Q6: col_6, Q7: col_7, Q8: col_8, Q9: col_9, Q10: col_10, Q11: col_11, Q12: col_12, Q13: col_13, Q14: col_14, Q15: col_15);

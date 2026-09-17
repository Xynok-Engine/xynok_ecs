use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{QueryTicks, TQueryParam, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::chunk::layout::ChunkLayout;
use crate::query::access_scope::AccessScope;

// A tuple shares one row cursor across every element. The shared iterators ask every element
// `accepts` for the *same* row before reading any of them, and skip the row unless all agree.
macro_rules! impl_tuple_query_param {
    ($($q:ident : $i:tt),+) => {
        impl<$($q: TQueryParam),+> TQueryParam for ($($q,)+)
        {
            type QueryItem<'a> = ($($q::QueryItem<'a>,)+);
            type ArchState = ($($q::ArchState,)+);
            type Fetch = ($($q::Fetch,)+);
            type Shape = ($($q::Shape,)+);

            // a tuple has nothing to walk only when every element has nothing to walk
            const EXCLUDE_ONLY: bool = $($q::EXCLUDE_ONLY &&)+ true;

            fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
            {
                let mut scope = AccessScope::default();
                $(scope.extend($q::access_scope(component_specs)?)?;)+
                Ok(scope)
            }

            #[inline]
            #[track_caller]
            fn arch_state(layout: &ChunkLayout) -> Self::ArchState
            {
                ($($q::arch_state(layout),)+)
            }

            #[inline]
            unsafe fn fetch_init(state: Self::ArchState, chunk_ptr: *mut u8) -> Self::Fetch
            {
                unsafe { ($($q::fetch_init(state.$i, chunk_ptr),)+) }
            }

            #[inline]
            unsafe fn accepts(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> bool
            {
                unsafe { $($q::accepts(&fetch.$i, row, ticks) &&)+ true }
            }

            #[inline]
            unsafe fn fetch<'a>(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> Self::QueryItem<'a>
            {
                unsafe { ($($q::fetch(&fetch.$i, row, ticks),)+) }
            }
        }
        // SAFETY: a tuple is read-only exactly when every element in it is read-only
        unsafe impl<$($q: TQueryParam + TReadOnlyQueryParam),+> TReadOnlyQueryParam for ($($q,)+) {}
    };
}

#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9, Q10: 10);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9, Q10: 10, Q11: 11);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9, Q10: 10, Q11: 11, Q12: 12);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9, Q10: 10, Q11: 11, Q12: 12, Q13: 13);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9, Q10: 10, Q11: 11, Q12: 12, Q13: 13, Q14: 14);
#[rustfmt::skip] impl_tuple_query_param!(Q0: 0, Q1: 1, Q2: 2, Q3: 3, Q4: 4, Q5: 5, Q6: 6, Q7: 7, Q8: 8, Q9: 9, Q10: 10, Q11: 11, Q12: 12, Q13: 13, Q14: 14, Q15: 15);

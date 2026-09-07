//! `(a, b, c)` -> three boxed systems, so `add_system_parallel` can take a whole group in one call.
//!
//! Every element of the tuple carries its own parameter marker, so the marker of the group is a
//! tuple of those markers. It sounds roundabout, but it is exactly what keeps inference working: a
//! given `fn` matches exactly one `TIntoSystem<P>`, so each element's `P` is determined, and so is
//! the tuple of them.

use crate::apis::identifies::XynokEcsError;
use crate::system::traits::{SystemTypeStorage, TIntoSystem, TIntoSystems};

macro_rules! tuple_into_systems {
    ($(($sys:ident, $param:ident, $slot:ident)),+ $(,)?) =>
    {
        impl<$($sys, $param,)+> TIntoSystems<($($param,)+)> for ($($sys,)+)
        where $($sys: TIntoSystem<$param>,)+
        {
            fn into_systems(self) -> Result<Vec<SystemTypeStorage>, XynokEcsError>
            {
                let ($($slot,)+) = self;
                Ok(vec![$($slot.into_system()?,)+])
            }
        }
    };
}

#[rustfmt::skip] tuple_into_systems!((S0, P0, s0));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9), (S10, P10, s10));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9), (S10, P10, s10), (S11, P11, s11));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9), (S10, P10, s10), (S11, P11, s11), (S12, P12, s12));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9), (S10, P10, s10), (S11, P11, s11), (S12, P12, s12), (S13, P13, s13));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9), (S10, P10, s10), (S11, P11, s11), (S12, P12, s12), (S13, P13, s13), (S14, P14, s14));
#[rustfmt::skip] tuple_into_systems!((S0, P0, s0), (S1, P1, s1), (S2, P2, s2), (S3, P3, s3), (S4, P4, s4), (S5, P5, s5), (S6, P6, s6), (S7, P7, s7), (S8, P8, s8), (S9, P9, s9), (S10, P10, s10), (S11, P11, s11), (S12, P12, s12), (S13, P13, s13), (S14, P14, s14), (S15, P15, s15));

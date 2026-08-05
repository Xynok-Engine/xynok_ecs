use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{SystemTypeStorage, TIntoSystem, TIntoSystems};

impl<Marker, S> TIntoSystems<(Marker,)> for S
where
    Marker: 'static,
    S: TIntoSystem<Marker>,
{
    #[track_caller]
    fn into_systems(self) -> Result<Vec<SystemTypeStorage>, XynokEcsError>
    {
        Ok(vec![Box::new(self.into_system()?)])
    }
}

macro_rules! impl_tuple_into_systems {
    ($($s:ident : $m:ident),+) => {
        // the parameter is `($(($m, $m),)+)` rather than `($($m,)+)` purely for coherence: a
        // one-element tuple of that shape is `((M, M),)`, which can never equal the `(Marker,)` the
        // single-system impl above claims, so the two impls stay disjoint at arity 1
        #[allow(non_snake_case)]
        impl<$($m: 'static, $s: TIntoSystem<$m>),+> TIntoSystems<($(($m, $m),)+)> for ($($s,)+)
        {
            #[track_caller]
            fn into_systems(self) -> Result<Vec<SystemTypeStorage>, XynokEcsError>
            {
                let ($($s,)+) = self;
                let systems: Vec<SystemTypeStorage> = vec![$(Box::new($s.into_system()?),)+];
                Ok(systems)
            }
        }
    };
}

#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5, S6: M6);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5, S6: M6, S7: M7);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5, S6: M6, S7: M7, S8: M8);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5, S6: M6, S7: M7, S8: M8, S9: M9);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5, S6: M6, S7: M7, S8: M8, S9: M9, S10: M10);
#[rustfmt::skip] impl_tuple_into_systems!(S0: M0, S1: M1, S2: M2, S3: M3, S4: M4, S5: M5, S6: M6, S7: M7, S8: M8, S9: M9, S10: M10, S11: M11);

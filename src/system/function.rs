use crate::apis::internal_traits::{TSystemOutput, TSystemParam, TSystemParamFunction};

impl<Out, Func> TSystemParamFunction<fn(Out)> for Func
where
    Out: TSystemOutput,
    Func: Send + Sync + 'static + FnMut() -> Out,
{
    type Param = ();
    type Out = Out;

    #[inline]
    fn run(&mut self, _params: ()) -> Self::Out
    {
        self()
    }
}

macro_rules! impl_system_param_function {
    ($($p:ident),+) => {
        // `Out` lives in the marker, not just in the associated type: a type parameter that appears
        // only in an associated position is unconstrained (E0207) and the impl would be rejected
        #[allow(non_snake_case)]
        impl<Out, Func, $($p: TSystemParam),+> TSystemParamFunction<fn(Out, $($p,)+)> for Func
        where
            Out: TSystemOutput,
            Func: Send
                + Sync
                + 'static
                + FnMut($($p,)+) -> Out
                + for<'w> FnMut($(<$p as TSystemParam>::Item<'w>,)+) -> Out,
        {
            type Param = ($($p,)+);
            type Out = Out;

            #[inline]
            fn run(&mut self, params: <Self::Param as TSystemParam>::Item<'_>) -> Self::Out
            {
                // calling `self(...)` directly makes rustc pick the *first* `FnMut` bound, which is
                // written in terms of `$p` rather than `$p::Item<'w>`. Routing through a generic
                // function whose only bound is the one we want forces the higher-ranked one to be
                // selected instead, with `'w` inferred from the actual params
                #[inline]
                fn call_inner<Out, $($p,)+>(mut f: impl FnMut($($p,)+) -> Out, $($p: $p,)+) -> Out
                {
                    f($($p,)+)
                }
                let ($($p,)+) = params;
                call_inner(self, $($p,)+)
            }
        }
    };
}

#[rustfmt::skip] impl_system_param_function!(P0);
#[rustfmt::skip] impl_system_param_function!(P0, P1);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14);
#[rustfmt::skip] impl_system_param_function!(P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, P10, P11, P12, P13, P14, P15);

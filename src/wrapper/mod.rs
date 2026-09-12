use crate::apis::traits::{TComponent, TEnableAble};

macro_rules! define_enable {
    ($name:ident, $val:expr) => {
        /// Use this wrapper to initialize a component with enable value = `$name`
        pub struct $name<T: TComponent + TEnableAble + 'static>
        {
            pub val: T,
        }
        impl<T: TComponent + TEnableAble + 'static> $name<T>
        {
            pub fn new(val: T) -> Self
            {
                Self { val: val }
            }
        }
        impl<T: TComponent + TEnableAble + 'static> TComponent for $name<T>
        {
            type QueryType = T::QueryType;
            type StorageType = T::StorageType;

            const STORAGE_LOCATION: crate::apis::identifies::StorageLocation = T::STORAGE_LOCATION;
            const STATE_DETECTION: crate::apis::identifies::StateDetection = T::STATE_DETECTION;
            const ENABLE_VALUE: bool = $val;
        }
    };
}
define_enable!(Enable, true);
define_enable!(Disable, false);

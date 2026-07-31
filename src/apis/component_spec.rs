use crate::apis::{fn_ptr::FnComponentDropItSelf, ComponentDescriptor};

pub struct ComponentSpec
{
    pub id:         usize,
    pub descriptor: ComponentDescriptor,
}

use crate::apis::fn_ptr::FnComponentDropItSelf;

pub struct ComponentSpec
{
    pub id:      usize,
    pub fn_drop: FnComponentDropItSelf,
}

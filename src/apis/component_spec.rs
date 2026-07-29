use crate::apis::FnComponentDropItSelf;

pub struct ComponentSpec
{
    pub id:      usize,
    pub fn_drop: FnComponentDropItSelf,
}

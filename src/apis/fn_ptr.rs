use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{
        identifies::XynokEcsError,
        params::{ComponentSpec, SwappedRow},
    },
    chunk::{layout::ChunkLayout, Chunk},
};

pub type FnComponentDropItSelf = fn(*mut u8);
pub type FnArchtypeRemoveEntity = fn(&ChunkLayout, &HashMap<TypeId, ComponentSpec>, &mut Chunk, usize) -> Result<Option<SwappedRow>, XynokEcsError>;
pub type FnCloneComponent = fn(*const u8, *mut u8);

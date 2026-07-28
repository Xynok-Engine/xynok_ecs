use crate::{
    apis::{swapped_row::SwappedRow, XynokEcsError},
    chunk::{layout::ChunkLayout, Chunk},
};

pub type FnDropComponent = fn(*mut u8);
pub type FnRemoveEntity = fn(&ChunkLayout, &mut Chunk, usize) -> Result<Option<SwappedRow>, XynokEcsError>;
pub type FnCloneComponent = fn(*const u8, *mut u8);

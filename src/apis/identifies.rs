use std::alloc::LayoutError;

use thiserror::Error;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageLocation
{
    Chunk,
    Archetype,
}

#[derive(Error, Debug)]
pub enum XynokEcsError
{
    #[error("Archetype contains ZERO component")]
    EmptyArchetype,

    #[error("Archetype's component total size exceeds 16kB")]
    ArchetypeIsTooLarge,

    #[error("Chunk Layout allocation creation failed: {0}")]
    ChunkLayoutAllocation(LayoutError),

    #[error("Chunk does not contain component: Query<{0}> + Storage<{1}>")]
    ChunkDoesNotContainComponent(&'static str, &'static str),

    #[error("Idx({0}) is out of chunk len [0 ,{1}]")]
    IdxIsOutOfChunkLen(usize, usize),

    #[error("Chunk is full of capacity({0})")]
    ChunkIsFull(usize),

    #[error("Chunk idx({0}) is not in range [0, {1}]")]
    ChunkIdxIsNotInRange(usize, usize),

    #[error("Conflict sub Archetype indices")]
    ConflictSubArchetype,
}
// src: https://crates.io/crates/thiserror
//#[derive(Error, Debug)]
//pub enum DataStoreError
//{
//    #[error("data store disconnected")]
//    Disconnect(#[from] std::io::Error),
//    #[error("the data for key `{0}` is not available")]
//    Redaction(String),
//    #[error("invalid header (expected {expected:?}, found {found:?})")]
//    InvalidHeader
//    {
//        expected: String, found: String
//    },
//    #[error("unknown data store error")]
//    Unknown,
//}

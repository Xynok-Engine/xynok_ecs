use std::alloc::LayoutError;
use std::fmt::Debug;

use thiserror::Error;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageLocation
{
    Chunk,
    Archetype,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum XynokEcsError
{
    #[error("entity does not exist")]
    EntityDoesNotExist,

    #[error("entity does not carry this shared component")]
    SharedComponentDoesNotExist,

    #[error("entity already carries this shared component")]
    SharedComponentAlreadyExists,

    #[error("no archetype holds this shared key for the entity's components yet; create it with add_shared_component")]
    UnresolvedSharedKey,

    #[error("an archetype already holds a value for this shared key; join it with add_shared_component_by_key")]
    SharedDestinationExists,

    #[error("the new value has another shared key than the current one; move with set_shared_component_by_key, or use override_shared_component")]
    SharedKeyMismatch,

    #[error("Exceeded the maximum number of entities: {0}")]
    EntityAmountOverflow(usize),

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

    #[error("ComponentSpec is not registered")]
    ComponentSpecIsNotRegistered,

    #[error("Duplicated component detected in this pair of Archetypes")]
    DuplicatedComponent,

    #[error("Different Entity !")]
    EntityIsNotTheSame,

    #[error("Query contains duplicated components !")]
    QueryAccessScopeConflict,

    #[error("Two parameters of the same system access a component in conflicting ways !")]
    SystemAccessScopeConflict,

    #[error("`{0}` and `{1}` were declared to run in parallel but their access scopes conflict !")]
    ParallelGroupConflict(&'static str, &'static str),

    #[error("System `{0}` has no registered spec !")]
    SystemSpecIsNotRegistered(&'static str),

    #[error("Query<{0}> was not prepared before the system ran !")]
    QueryIsNotPrepared(&'static str),
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

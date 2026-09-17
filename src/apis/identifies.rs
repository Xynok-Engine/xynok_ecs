use std::alloc::LayoutError;
use std::fmt::Debug;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageLocation
{
    Chunk,
    Archetype,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateDetection
{
    None,
    EnableAble,
    ChangeAble,
    EnableAbleAndChangeAble,
}

#[derive(Error, Debug)]
pub enum XynokEcsError
{
    #[error("Exceeded the maximum number of entities: {0}")]
    EntityAmountOverflow(usize),

    #[error("Entity slot {0} has used up all {1} of its versions")]
    EntityVersionOverflow(usize, usize),

    #[error("Archetype's component total size exceeds its chunk size")]
    ArchetypeIsTooLarge,

    #[error("Invalid chunk size {0} bytes: it must be greater than 0 and a multiple of {1}")]
    InvalidChunkSize(usize, usize),

    #[error("Archetype `{0}` already exists with a chunk size of {1} bytes, cannot register it again with {2} bytes")]
    ArchetypeAlreadyCreatedWithDifferentChunkSize(&'static str, usize, usize),

    #[error("Archetype `{0}` already exists with allow_structure_change = {1}, cannot register it again with {2}")]
    ArchetypeAlreadyCreatedWithDifferentStructureChange(&'static str, bool, bool),

    #[error("Cannot add/remove `{2}` on Entity(idx: {0}, version: {1}): the source or target archetype does not allow structure changes")]
    StructureChangeNotAllowed(usize, usize, &'static str),

    #[error(
        "Cannot add component `{2}` for Entity(idx: {0}, version: {1}): a component with this type already exists. add_component() only accepts components the entity does not have yet, use merge_component() to overwrite them"
    )]
    ComponentAlreadyExists(usize, usize, &'static str),

    #[error("Cannot remove component `{2}` from Entity(idx: {0}, version: {1}): the entity does not have this component")]
    ComponentNotFound(usize, usize, &'static str),

    #[error("Singleton archetype `{0}` already has its entity, destroy it before spawning another one")]
    SingletonAlreadyExists(String),

    #[error("Archetype `{0}` already exists as a regular archetype, it cannot become a singleton")]
    ArchetypeIsNotSingleton(String),

    #[error("Archetype `{0}` is a singleton, it cannot be registered as a regular archetype")]
    ArchetypeIsSingleton(String),

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

    #[error(
        "Archetype declares component `{0}` more than once. A chunk keeps one column per component, \
         so the second value would overwrite the first without dropping it. Name each component once."
    )]
    DuplicateComponentInArchetype(&'static str),

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

    #[error("Component `{0}` does not support `{1}` state detection")]
    ComponentStateNotAvailable(&'static str, &'static str),

    #[error("Entity(idx: {0}, version: {1}) does not exist: it was never created, or it was destroyed and this handle is stale")]
    EntityDoesNotExist(usize, usize),

    #[error("query spec for {0} vanished right after it was prepared")]
    QuerySpecVanishedAfterPrepared(&'static str),

    #[error("The pre-allocated entity count must be greater than 0")]
    PreAllocateEntityAmountMustGreaterThanZero,

    #[error("WorkerSpec is not created for current thread !")]
    WorkerSpecIsNotCreated,
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

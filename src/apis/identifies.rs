use std::alloc::LayoutError;
use std::backtrace::{Backtrace, BacktraceStatus};
use std::error::Error;
use std::fmt::{Debug, Display};

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

    #[error("This Archetype contains shareable component")]
    ThisArchetypeContainsShareAbleComponent,

    #[error("This Archetype does not contain shareable component")]
    ThisArchetypeDoesNotContainShareAbleComponent,

    #[error("System `{0}` was run before its param state was initialized")]
    SystemStateIsNotInitialized(&'static str),
}

/// The error a system body is allowed to return.
///
/// A system is user code: it calls into files, sockets, parsers, whatever the game needs, and those
/// return their own error types. Forcing every such error into `XynokEcsError` would mean growing
/// that enum for every user crate, so this type erases the error instead and captures a backtrace
/// at the conversion point (the only place we still know where it came from).
///
/// Deliberately **not** an `impl std::error::Error`: doing so would make the blanket
/// `impl<E: Error> From<E> for XynokError` below overlap core's reflexive `impl<T> From<T> for T`,
/// and the compiler rejects that. Read the cause through [`XynokError::source`] instead.
pub struct SystemError
{
    inner: Box<InnerSystemError>,
}

// Boxed so `XynokError` stays one pointer wide: it rides inside `Result<(), XynokError>` on every
// system return, and a `Backtrace` inline would bloat the ok-path of every single system call.
struct InnerSystemError
{
    error:     Box<dyn Error + Send + Sync + 'static>,
    backtrace: Backtrace,
}

impl SystemError
{
    pub fn source(&self) -> &(dyn Error + Send + Sync + 'static)
    {
        &*self.inner.error
    }

    pub fn backtrace(&self) -> &Backtrace
    {
        &self.inner.backtrace
    }
}

impl<E: Error + Send + Sync + 'static> From<E> for SystemError
{
    // errors are the cold path: keep the boxing and the backtrace capture out of the caller's
    // instruction cache
    #[cold]
    fn from(error: E) -> Self
    {
        Self {
            inner: Box::new(InnerSystemError {
                error:     Box::new(error),
                backtrace: Backtrace::capture(),
            }),
        }
    }
}

impl Display for SystemError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        Display::fmt(&self.inner.error, f)
    }
}

impl Debug for SystemError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        Debug::fmt(&self.inner.error, f)?;

        if self.inner.backtrace.status() == BacktraceStatus::Captured
        {
            writeln!(f, "\nBacktrace:")?;
            write!(f, "{}", self.inner.backtrace)?;
        }
        Ok(())
    }
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
